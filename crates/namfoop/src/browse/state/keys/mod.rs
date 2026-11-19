//! Le **clavier** de `browse` : qui prend la main, et dans quel ordre. C'est le seul endroit où
//! l'ordre des couches est écrit — fenêtres (quitter, bascule de preset), saisies de texte (nom,
//! filtre des listes, **filtre du catalogue** : une saisie passe toujours avant les touches
//! communes), banque, « mettre dans la chaîne » et mode pose, sélecteur sur place, puis les
//! touches de l'espace.
//!
//! Chaque couche modale rend `Some(Action)` quand elle a consommé la touche (voir `picker`,
//! `send`, `preset`), ce qui évite qu'une touche agisse à deux endroits.

use super::*;

impl State {
    /// **Qui reçoit la touche.** `State::context()` est la **seule autorité d'ordre** : c'est elle
    /// qui dit où l'on est, et ce `match` dit qui traite chaque contexte. Il est **exhaustif**,
    /// donc ajouter un `Context` sans dire qui le prend ne compile plus.
    ///
    /// Les contextes d'**espace** (et les panneaux qui en font partie) descendent les **quatre
    /// étages** de l'ADR 0009, dans [`State::tiers`]. Tous les autres sont des **modes** : un mode
    /// ne retombe jamais, et c'est ce que dit chaque bras qui rend `Action::None` à défaut.
    ///
    /// Une saisie de texte peut *rejouer depuis le haut* (`on_key` récursif) : ce n'est pas une
    /// retombée — la saisie se termine, et la touche est relue dans un clavier devenu différent.
    pub fn on_key(&mut self, key: Key) -> Action {
        use super::context::Context as C;
        let ctx = self.context();
        // Les **murs**, qui passent avant l'aide elle-même : la scène verrouillée (`run --set`,
        // § 9) n'a pas de parent, et une confirmation garde l'état jusqu'à la réponse.
        match ctx {
            C::Stage | C::StageQuit => return self.locked_key(key),
            // l'aide se referme sur n'importe quelle touche
            C::Help => {
                self.help = false;
                return Action::None;
            }
            C::ConfirmQuit | C::ConfirmQuitEmpty => return self.confirm_quit_key(key),
            C::ConfirmSwitch => return self.confirm_switch_key(key),
            _ => {}
        }
        // `h` **ouvre** l'aide : au-dessus des modales — une fenêtre doit pouvoir s'expliquer —,
        // mais sous les saisies, où `h` est une lettre.
        if matches!(key, Key::Char('h') | Key::Char('?')) && !self.typing() {
            self.help = true;
            self.message = None;
            return Action::None;
        }
        // **L'étage pédale passe au-dessus des fenêtres** (ADR 0009 : `*` et `k` sont des gestes
        // physiques, ils répondent pareil partout). La touche **referme** la fenêtre — comme
        // `Échap`, donc avec l'annulation que cette fenêtre connaît — **puis** agit : elle n'agit
        // jamais *derrière* elle, ce qui est la règle des modales. Restent au-dessus de la pédale
        // les **saisies** (une lettre y est une lettre) et les **murs** (une confirmation attend sa
        // réponse). Aucune de ces fenêtres ne lie `1 2 3 q * k` : rien ne lui est volé.
        if Self::pedal_owns(key)
            && !self.typing()
            && matches!(ctx, C::Bank | C::Send | C::Place | C::Picker | C::SetPicker)
        {
            match ctx {
                C::Bank => self.bank_key(Key::Esc),
                C::Send => self.send_key(Key::Esc),
                C::Place => self.place_key(Key::Esc),
                C::Picker => self.picker_key(Key::Esc),
                C::SetPicker => self.set_picker_key(Key::Esc),
                _ => None,
            };
            // le message d'annulation de la fenêtre n'a plus d'intérêt : le geste demandé parle
            self.message = None;
            return self.pedal_key(key).unwrap_or(Action::None);
        }
        match ctx {
            // déjà traités plus haut : le `match` reste exhaustif pour que le compilateur nomme
            // tout contexte nouveau
            C::Stage | C::StageQuit | C::Help => Action::None,
            C::ConfirmQuit | C::ConfirmQuitEmpty | C::ConfirmSwitch => Action::None,
            C::Editor | C::EditorTyping => self.editor_key(key),
            C::Naming => self.naming_key(key),
            // le sélecteur sur place prend toutes les touches : valider ou annuler d'abord (§ 5)
            C::Picker | C::PickerFiltering => self.picker_key(key).unwrap_or(Action::None),
            // le filtre du catalogue se tape au clavier : il prend les lettres avant les touches
            // communes, sinon `n` (chaîne vierge) mangerait le « n » de `nam` et de `gain`
            C::Catalog => self.catalog_key(key).unwrap_or(Action::None),
            C::Bank => self.bank_key(key).unwrap_or(Action::None),
            C::Send => self.send_key(key).unwrap_or(Action::None),
            C::Place => self.place_key(key).unwrap_or(Action::None),
            // la fenêtre `L` (choisir le set) est une modale **comme les autres** : elle se classe
            // par sa priorité propre et non par l'espace qui la tient. Logée dans `set_key`, elle
            // passerait sous les communes, et `n` effacerait la chaîne derrière elle.
            C::SetPicker => self.set_picker_key(key).unwrap_or(Action::None),
            // les sous-modes de l'édition d'un set (§ 8) : une saisie, la liste des presets, la
            // confirmation de `X`. Ce sont des modes, donc ils passent avant la pédale — sans quoi
            // taper `1` dans un titre changerait d'espace et `q` quitterait au milieu d'un nom.
            C::SetEditTyping | C::SetEditPicking | C::SetEditRemove => {
                self.set_edit_mode_key(key).unwrap_or(Action::None)
            }
            C::Filtering => self.filtering_key(key),
            // les espaces, et les panneaux qui en font partie : les quatre étages. La **table** de
            // l'édition d'un set en fait partie, parce que la pédale doit y répondre.
            C::Set | C::SetEdit | C::Chain | C::ChainMerge | C::Params => self.tiers(key),
            C::Models | C::Irs | C::Blend => self.tiers(key),
        }
    }

    /// **Les touches de l'étage pédale**, pour la garde qui les fait passer au-dessus des
    /// fenêtres. `h` / `?` y sont par exactitude, mais l'aide est traitée avant d'arriver ici :
    /// elle doit pouvoir expliquer la fenêtre elle-même. Le test
    /// `the_pedal_list_matches_the_dispatch` vérifie que cette liste est exactement celle de
    /// [`State::pedal_key`] — deux listes qui divergent sont le défaut que l'ADR 0009 a chassé.
    fn pedal_owns(key: Key) -> bool {
        matches!(key, Key::Char('1' | '2' | '3' | '*' | 'k' | 'q' | 'h' | '?'))
    }

    /// **Les quatre étages** de l'ADR 0009, pour les seuls contextes d'espace. Une touche les
    /// descend dans cet ordre, et le **premier qui la possède** la consomme.
    fn tiers(&mut self, key: Key) -> Action {
        // ÉTAGE 1 — LA PÉDALE : les gestes qu'un espace ne peut pas éclipser.
        if let Some(a) = self.pedal_key(key) {
            return a;
        }
        // ÉTAGE 2 — L'ESPACE où l'on est. Il passe **avant** les communes : c'est ce qui
        // permet à un espace de s'approprier une touche, seule issue tenable avec 20 lettres
        // sur 26 déjà prises.
        if let Some(a) = self.space_key(key) {
            return a;
        }
        // ÉTAGE 3 — LES COMMUNES, éclipsables par un espace qui le déclare.
        if let Some(a) = self.common_key(key) {
            return a;
        }
        // ÉTAGE 4 — LE DERNIER RECOURS.
        self.global_key(key)
    }

    /// **La saisie du nom** d'un mélange ou d'un preset : toute lettre y va, et c'est ce qui
    /// empêche une touche commune de lui être volée.
    fn naming_key(&mut self, key: Key) -> Action {
        let Some(name) = self.naming.as_mut() else { return Action::None };
        match key {
            Key::Enter => {
                let n = name.trim().to_string();
                let preset = std::mem::take(&mut self.naming_preset);
                self.naming = None;
                if n.is_empty() {
                    self.message = Some(
                        if preset {
                            "nom vide : preset non enregistré"
                        } else {
                            "nom vide : mélange non sauvé"
                        }
                        .into(),
                    );
                    return Action::None;
                }
                return if preset { Action::SavePreset(n) } else { Action::SaveBlend(n) };
            }
            Key::Esc => {
                self.naming = None;
                self.naming_preset = false;
            }
            Key::Backspace => {
                name.pop();
            }
            Key::Char(c) => name.push(c),
            _ => {}
        }
        Action::None
    }

    /// **Le filtre d'une liste de la Bibliothèque.** Les touches de navigation le **terminent** et
    /// sont **rejouées depuis le haut** : la saisie finit, et la touche est relue dans un clavier
    /// devenu différent — ce n'est pas une retombée.
    fn filtering_key(&mut self, key: Key) -> Action {
        match key {
            Key::Enter | Key::Esc | Key::Tab | Key::BackTab | Key::Up | Key::Down => {
                self.filtering = false;
                if !matches!(key, Key::Enter | Key::Esc) {
                    return self.on_key(key);
                }
            }
            Key::Backspace => {
                self.filter.pop();
                self.set_cursor(0);
            }
            Key::Char(c) => {
                self.filter.push(c);
                self.set_cursor(0);
            }
            _ => {}
        }
        Action::None
    }

    /// **L'étage pédale** : les gestes qu'un espace ne peut **pas** éclipser (ADR 0009).
    ///
    /// Ce n'est pas de l'esthétique, c'est de la fiabilité : se retrouver coincé dans un espace,
    /// ou sans tap tempo, est un risque en concert. `*` et `k` sont des gestes physiques (le pied,
    /// l'oreille) qui doivent répondre pareil partout — quand ils n'ont rien à faire là, ils le
    /// **disent** (`tap_tempo` le fait) au lieu de ne rien faire.
    fn pedal_key(&mut self, key: Key) -> Option<Action> {
        Some(match key {
            Key::Char('1') => self.go_to(space::Space::Library),
            Key::Char('2') => self.go_to(space::Space::Chain),
            Key::Char('3') => self.go_to(space::Space::Set),
            // `*` : le tap tempo (ADR 0008). À droite de `ù` sur AZERTY FR, sans Maj.
            Key::Char('*') => self.tap_tempo(),
            // `k` : le clic du métronome (ADR 0008)
            Key::Char('k') => self.toggle_metronome(),
            Key::Char('q') => self.quit_or_confirm(),
            Key::Char('h') | Key::Char('?') => {
                self.help = true;
                self.message = None;
                Action::None
            }
            _ => return None,
        })
    }

    /// **L'étage espace** : le clavier de l'espace où l'on est, et de lui seul.
    ///
    /// Il existe pour qu'une touche qu'un espace ne consomme pas **ne retombe pas** dans le
    /// clavier d'un autre écran : sans cet étage, le clavier de la Bibliothèque serait la dernière
    /// couche, et une touche y agirait sur un écran qu'on ne regarde même pas.
    fn space_key(&mut self, key: Key) -> Option<Action> {
        match self.space {
            space::Space::Set => self.set_key(key),
            space::Space::Chain => self.fx_key(key),
            space::Space::Library => self.library_key(key),
        }
    }

    /// **L'étage commun** : ce qui vaut dans tous les espaces, mais qu'un espace peut **éclipser**
    /// en le déclarant (il suffit qu'il consomme la touche à l'étage du dessus).
    fn common_key(&mut self, key: Key) -> Option<Action> {
        Some(match key {
            Key::Char('S') => self.save_preset(),
            // `n` : repartir d'une chaîne vierge
            Key::Char('n') => self.new_chain_or_confirm(),
            Key::Char('P') => {
                self.bank = Some(preset::Bank::default());
                self.message = if self.presets.is_empty() {
                    Some("aucun preset enregistré : S enregistre la chaîne".into())
                } else {
                    None
                };
                Action::None
            }
            _ => return None,
        })
    }

    /// **Le dernier recours.** `Échap` ne quitte jamais : arrivé ici, il n'y a plus de niveau à
    /// remonter (filtre, nom, éditeur et réglages l'ont déjà consommé).
    ///
    /// Une touche que **personne** ne possède est retenue dans `unhandled` : la boucle l'écrit dans
    /// `NAMFOOP_BROWSE_TRACE`. Sur scène, « j'ai appuyé et rien ne s'est passé » devient alors
    /// diagnosticable — et rien ne s'affiche à l'écran, pour ne pas apprendre à l'utilisateur à
    /// ignorer la zone de message — elle ne doit annoncer que ce qui est vrai.
    fn global_key(&mut self, key: Key) -> Action {
        match key {
            Key::Esc => self.message = Some("Échap ne quitte pas : q pour quitter".into()),
            _ => self.unhandled = Some(key),
        }
        Action::None
    }

    /// **Le clavier de la Bibliothèque** : les listes, le banc d'essai et le mélange. C'est un
    /// étage d'espace comme les deux autres, atteint depuis la seule Bibliothèque.
    ///
    /// `_ => None` est **obligatoire** : cette couche rend un `Option`, et un `match` qui
    /// consommerait tout (`_ => {}`) ferait mourir ici les étages du dessous — les communes, le
    /// dernier recours.
    fn library_key(&mut self, key: Key) -> Option<Action> {
        let c = self.cursor();
        match key {
            // `Échap` et `q` ne sont plus ici : ils sont montés aux étages global et pédale, où
            // ils valent **partout** (ADR 0009).
            // `z` : le panneau Bandes. Il n'a pas besoin de garde d'espace — cette couche n'est
            // atteinte que depuis la Bibliothèque, et l'étage s'en charge pour toutes les touches.
            Key::Char('z') => self.toggle_bands(),
            Key::Char('/') => {
                if matches!(self.tab, Tab::Models | Tab::Irs) {
                    self.filtering = true;
                }
            }
            // Tab / Maj+Tab : tous les onglets, en boucle ; → / ← : de proche en proche, sans boucler
            Key::Tab => self.switch_tab(1, true),
            Key::BackTab => self.switch_tab(-1, true),
            Key::Right => self.switch_tab(1, false),
            Key::Left => self.switch_tab(-1, false),
            Key::Up => self.set_cursor(c.saturating_sub(1)),
            Key::Down => self.set_cursor(c + 1),
            Key::PageUp => self.set_cursor(c.saturating_sub(10)),
            Key::PageDown => self.set_cursor(c + 10),
            // Entrée met le son du banc dans la chaîne (§ 4), il ne « choisit » plus
            Key::Enter => return Some(self.send_to_chain()),
            Key::Char('e') if self.tab == Tab::Models => return Some(self.open_editor()),
            Key::Char('c') if self.tab == Tab::Models => {
                if let Some(m) = self.model_at_cursor() {
                    let was_b = self.listen_ref;
                    self.reference = None;
                    self.listen_ref = false;
                    if self.ref_model == Some(m) {
                        self.ref_model = None;
                        self.message = Some("modèle de référence retiré".into());
                    } else {
                        self.ref_model = Some(m);
                        self.message = Some(if self.model == Some(m) {
                            format!(
                                "◆ modèle de référence (B) : {} — choisis maintenant un autre modèle (A), puis r",
                                self.models[m].label
                            )
                        } else {
                            format!(
                                "◆ modèle de référence (B) : {} — r pour basculer A ↔ B",
                                self.models[m].label
                            )
                        });
                    }
                    if was_b {
                        return Some(self.play());
                    }
                }
            }
            Key::Char('c') if self.tab == Tab::Irs => {
                if self.ref_model.take().is_some() && self.listen_ref {
                    self.listen_ref = false;
                }
                if let Some(i) = self.ir_at_cursor() {
                    if self.reference == Some(i) {
                        self.reference = None;
                        self.listen_ref = false;
                        self.message = Some("référence retirée".into());
                        return Some(self.play());
                    }
                    self.reference = Some(i);
                    self.listen_ref = false;
                    self.message = Some(if self.ir == IrChoice::Item(i) {
                        format!(
                            "◆ référence (B) : {} — choisis maintenant une autre IR (A) avec ↑↓, puis r pour comparer",
                            self.irs[i].label
                        )
                    } else {
                        format!("◆ référence (B) : {} — r pour basculer A ↔ B", self.irs[i].label)
                    });
                }
            }
            Key::Char('b') | Key::Char('r') => {
                if let Some(a) = self.ab_key(key) {
                    return Some(a);
                }
            }
            Key::Char(' ') if self.tab == Tab::Irs => {
                if let Some(IrRow::Ir { index, .. }) = self.ir_rows().get(c) {
                    let index = *index;
                    if let Some(pos) = self.blend.iter().position(|p| p.ir == index) {
                        self.blend.remove(pos);
                    } else {
                        self.blend.push(BlendPart { ir: index, gain_db: 0.0, invert: false });
                    }
                    return Some(self.blend_changed());
                }
            }
            Key::Char('+') | Key::Char('=') if self.tab == Tab::Blend => {
                return Some(self.adjust(|p| p.gain_db += 1.0))
            }
            Key::Char('-') if self.tab == Tab::Blend => return Some(self.adjust(|p| p.gain_db -= 1.0)),
            Key::Char('p') if self.tab == Tab::Blend => return Some(self.adjust(|p| p.invert = !p.invert)),
            Key::Char('x') | Key::Backspace if self.tab == Tab::Blend => {
                if c < self.blend.len() {
                    self.blend.remove(c);
                    return Some(self.blend_changed());
                }
            }
            Key::Char('a') if self.tab == Tab::Blend => {
                self.align = !self.align;
                return Some(self.blend_changed());
            }
            Key::Char('s') if self.tab == Tab::Blend => {
                if self.blend.is_empty() {
                    self.message = Some("mélange vide : espace sur une IR (onglet IR) pour l'ajouter".into());
                } else {
                    self.naming = Some(self.default_blend_name());
                }
            }
            _ => return None,
        }
        Some(Action::None)
    }
}

impl State {
    /// **La scène verrouillée** (`run --set`, § 9 de `docs/design/set.md`) : on ne joue que les
    /// sons. Rien ne s'édite, aucun espace ne s'ouvre, aucun preset ne s'enregistre — en concert,
    /// une touche de trop ne doit pas pouvoir changer autre chose que le son.
    ///
    /// La liste est **blanche** (ce qui est permis), jamais noire : une touche ajoutée ailleurs
    /// dans `browse` n'arrive donc pas par accident sur scène.
    fn locked_key(&mut self, key: Key) -> Action {
        // quitter la scène demande confirmation — mais pas celle de `browse` : ici rien ne
        // s'enregistre, donc `Q` quitte et tout le reste reste
        if self.confirm_quit {
            self.confirm_quit = false;
            return if key == Key::Char('Q') { Action::Quit } else { Action::None };
        }
        if self.help {
            self.help = false;
            return Action::None;
        }
        let (song, prompt, play) = (self.song_keys, self.prompt_keys, self.play_key);
        match key {
            Key::PageDown => self.step_set(true),
            Key::PageUp => self.step_set(false),
            k if k == song.0 => self.step_set_song(true),
            k if k == song.1 => self.step_set_song(false),
            // le prompteur au pied **passe** le verrou : il ne change rien, ni au son ni au set,
            // et suivre son texte est exactement ce qu'on fait sur scène (§ 10). `t`, lui, non :
            // il écrit un fichier et rend la main au terminal.
            k if k == prompt.0 => self.step_prompt(true),
            k if k == prompt.1 => self.step_prompt(false),
            // **la battue passe le verrou** pour la même raison (§ 11) : elle ne change ni le son
            // ni le set, et sur scène c'est elle qu'on lance en comptant le groupe.
            k if k == play => self.toggle_beat(),
            Key::Up | Key::Down | Key::Enter | Key::Char('i') => self.set_key(key).unwrap_or(Action::None),
            Key::Char('*') => self.tap_tempo(),
            Key::Char('k') => self.toggle_metronome(),
            Key::Char('h') | Key::Char('?') => {
                self.help = true;
                self.message = None;
                Action::None
            }
            Key::Char('q') => {
                // la scène se quitte volontairement : la fenêtre le demande (§ 9)
                self.confirm_quit = true;
                self.message = None;
                Action::None
            }
            _ => {
                // la liste des touches qui marchent vient de la **table**, seule déclaration :
                // écrite ici, elle serait une copie de plus, à mettre à jour à la main
                let ok: Vec<&str> = super::bindings::bindings(super::context::Context::Stage)
                    .iter()
                    .filter(|b| !b.bar.is_empty())
                    .map(|b| b.bar)
                    .collect();
                self.message = Some(format!("🔒 run --set : rien ne s'édite ici ({})", ok.join(" · ")));
                Action::None
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
