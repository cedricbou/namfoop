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
        let (song, prompt) = (self.song_keys, self.prompt_keys);
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
mod locked_tests {
    use super::*;
    use crate::browse::state::set::ActiveSet;
    use crate::browse::state::space::Space;
    use crate::browse::state::tests::with_chain;
    use library::set::{SetSpec, Song, Sound};

    /// La scène de `run --set` : un set de deux morceaux, verrouillée.
    fn stage() -> State {
        let mut s = with_chain(vec![]);
        let spec = SetSpec {
            name: "Concert".into(),
            songs: vec![
                Song {
                    title: Some("A".into()),
                    pc: Some(1),
                    sounds: vec![
                        Sound { preset: "p".into(), volume_db: 0.0 },
                        Sound { preset: "p".into(), volume_db: -1.5 },
                    ],
                    ..Default::default()
                },
                Song {
                    title: Some("B".into()),
                    pc: Some(2),
                    sounds: vec![Sound { preset: "p".into(), volume_db: 0.0 }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        s.set = Some(ActiveSet::new("s".into(), "concert".into(), spec, &|q| (q == "p").then_some(0)));
        s.locked = true;
        s.space = Space::Set;
        s.play_set_sound((0, 0));
        s.message = None;
        s
    }

    /// **Rien ne s'édite sur scène.** Toute touche que `browse` lie **ailleurs** et que la scène
    /// ne déclare pas doit être refusée *avec* la liste de celles qui marchent. C'est la garantie
    /// du mode verrouillé : en concert, une touche de trop ne doit pas pouvoir enregistrer,
    /// charger ou vider.
    ///
    /// La liste des touches à essayer vient de la **table** : écrite à la main ici, elle ne verrait
    /// pas une touche ajoutée ailleurs dans `browse`, et ce test passerait pour rien.
    #[test]
    fn every_editing_key_is_refused() {
        use crate::browse::state::bindings::bindings;
        use crate::browse::state::context::Context;
        let mut s = stage();
        // on compare des touches RÉSOLUES : la scène déclare `PrevSong`, l'éditeur `Left`, et
        // avec les réglages par défaut c'est la même touche
        let allowed: Vec<Key> = bindings(Context::Stage)
            .iter()
            .flat_map(|b| b.keys.iter().map(|k| k.resolve(s.song_keys, s.prompt_keys)))
            .collect();
        let before = format!("{:?}", s.fx.chain);
        let mut tried = 0;
        for ctx in Context::ALL {
            if matches!(ctx, Context::Stage | Context::StageQuit) {
                continue;
            }
            for b in bindings(*ctx) {
                for spec in b.keys {
                    let key = spec.resolve(s.song_keys, s.prompt_keys);
                    if allowed.contains(&key) {
                        continue;
                    }
                    // `Q` et `Échap` servent à la fenêtre de sortie, pas à éditer
                    if matches!(key, Key::Char('Q') | Key::Esc) {
                        continue;
                    }
                    tried += 1;
                    s.message = None;
                    let action = s.on_key(key);
                    assert!(matches!(action, Action::None), "{key:?} ({ctx:?}) : {action:?}");
                    assert!(
                        s.message.as_deref().is_some_and(|m| m.contains("rien ne s'édite ici")),
                        "{key:?} ({ctx:?}) : {:?}",
                        s.message
                    );
                }
            }
        }
        assert!(tried > 20, "seulement {tried} touches essayées : la table a-t-elle maigri ?");
        assert_eq!(before, format!("{:?}", s.fx.chain), "la chaîne n'a pas bougé");
        assert!(s.naming.is_none() && s.bank.is_none() && s.editor.is_none() && s.set_picker.is_none());
        assert_eq!(s.space, Space::Set, "on n'a pas quitté la scène");
    }

    /// Les gestes de scène, eux, répondent : son suivant, morceau, détail, tap, clic, aide.
    #[test]
    fn the_stage_gestures_still_work() {
        let mut s = stage();
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 1)), "son suivant");
        s.on_key(Key::PageUp);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)), "son précédent");
        s.on_key(s.song_keys.0);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)), "morceau suivant");
        s.on_key(Key::Up);
        assert_eq!(s.set.as_ref().unwrap().cursor, 0, "viser remonte");
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)), "mais ne joue rien");
        s.on_key(Key::Enter);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)), "Entrée joue le morceau visé");
        s.on_key(Key::Char('i'));
        assert!(s.set.as_ref().unwrap().is_expanded(0));
        s.on_key(Key::Char('h'));
        assert!(s.help, "l'aide s'ouvre");
        s.on_key(Key::Char('x'));
        assert!(!s.help, "et n'importe quelle touche la referme");
    }

    /// **`q` demande, `Q` quitte, tout le reste reste.** Couper le son d'un concert sur une seule
    /// touche serait pire que de devoir en taper deux.
    #[test]
    fn leaving_the_stage_takes_two_keys() {
        let mut s = stage();
        assert!(matches!(s.on_key(Key::Char('q')), Action::None));
        assert!(s.confirm_quit, "la fenêtre s'ouvre");
        assert!(matches!(s.on_key(Key::Esc), Action::None));
        assert!(!s.confirm_quit, "Échap referme");
        // et on est toujours là
        s.on_key(Key::Char('q'));
        assert!(matches!(s.on_key(Key::Char('Q')), Action::Quit), "Q quitte");

        // `S` ne propose PAS d'enregistrer en partant : il n'y a rien à enregistrer sur scène
        let mut s = stage();
        s.on_key(Key::Char('q'));
        assert!(matches!(s.on_key(Key::Char('S')), Action::None));
        assert!(s.naming.is_none(), "aucune saisie de nom");
        assert!(!s.confirm_quit, "la fenêtre est refermée");
    }

    /// Le tap marche sur scène (c'est son usage principal) — et il n'y marque rien comme « non
    /// enregistré », puisque rien ne s'y enregistre.
    #[test]
    fn the_tap_works_on_stage() {
        let mut s = stage();
        s.on_key(Key::Char('*'));
        s.on_key(Key::Char('*'));
        assert!(s.message.as_deref().is_some_and(|m| m.contains("tempo") || m.contains("tape")));
    }
}

#[cfg(test)]
mod tier_tests {
    use super::*;
    use crate::browse::state::space::Space;
    use crate::browse::state::tests::{fx_block, sample, with_chain};

    /// **La couche de la Bibliothèque laisse passer.** Son `_` doit rendre `None` : s'il rendait
    /// `Some(Action::None)`, elle avalerait tout et les étages du dessous — les communes, le
    /// dernier recours — mourraient dans la Bibliothèque. Ce test est là pour ça.
    #[test]
    fn the_library_layer_lets_the_lower_tiers_through() {
        let mut s = sample();
        assert!(s.library_key(Key::Char('S')).is_none(), "S appartient aux communes");
        assert!(s.library_key(Key::Char('n')).is_none());
        assert!(s.library_key(Key::Char('P')).is_none());
        assert!(s.library_key(Key::Char('*')).is_none(), "* appartient à la pédale");
        assert!(s.library_key(Key::Char('k')).is_none());
        assert!(s.library_key(Key::Char('q')).is_none());
        assert!(s.library_key(Key::Esc).is_none(), "Échap appartient au dernier recours");
        assert!(s.library_key(Key::Char('é')).is_none(), "une touche inconnue retombe");
        // et de bout en bout, elles marchent toujours depuis la Bibliothèque
        let mut s = sample();
        s.on_key(Key::Char('P'));
        assert!(s.bank.is_some(), "P ouvre la banque depuis la Bibliothèque");
        let mut s = sample();
        s.on_key(Key::Esc);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("Échap ne quitte pas")));
    }

    /// **L'étage pédale n'est pas éclipsable par un espace** : `1 2 3 q h * k` répondent depuis les
    /// trois espaces. C'est une règle de fiabilité live — se retrouver coincé dans un espace, ou
    /// sans tap tempo, n'est pas un désagrément mais un risque.
    #[test]
    fn the_pedal_tier_answers_from_every_space() {
        for space in [Space::Library, Space::Chain, Space::Set] {
            let start = || {
                let mut s = with_chain(vec![fx_block("reverb")]);
                s.space = space;
                s
            };
            let mut s = start();
            s.on_key(Key::Char('1'));
            assert_eq!(s.space, Space::Library, "1 depuis {space:?}");
            let mut s = start();
            s.on_key(Key::Char('3'));
            assert_eq!(s.space, Space::Set, "3 depuis {space:?}");
            let mut s = start();
            s.on_key(Key::Char('h'));
            assert!(s.help, "h depuis {space:?}");
            let mut s = start();
            s.on_key(Key::Char('*'));
            assert!(s.message.is_some(), "* répond (ou dit pourquoi) depuis {space:?}");
            let mut s = start();
            s.on_key(Key::Char('k'));
            assert!(s.message.is_some(), "k répond depuis {space:?}");
        }
    }

    /// **Une modale ne retombe jamais.** Tant qu'une fenêtre est ouverte, elle prend tout : sinon
    /// une touche agit **derrière** elle, sur un écran que la fenêtre recouvre — `n` effacerait la
    /// chaîne, `P` ouvrirait la banque, `1` changerait d'espace. La garde ne tient que si la modale
    /// est appelée à l'étage des modes, et non dans le clavier de l'espace qui la possède.
    #[test]
    fn an_open_window_takes_everything() {
        let mut s = with_chain(vec![fx_block("reverb")]);
        s.mark_start();
        s.on_key(Key::Char('3'));
        s.on_key(Key::Char('L'));
        assert!(s.set_picker.is_some());
        let before = format!("{:?}", (&s.fx.chain, s.space, s.bank.is_some(), &s.naming));
        for key in [
            Key::Char('n'),
            Key::Char('P'),
            Key::Char('S'),
            Key::Char('1'),
            Key::Char('2'),
            Key::Char('3'),
            Key::Char('*'),
            Key::Char('k'),
            Key::Char('q'),
            Key::Enter, // liste encore vide : la boucle n'a pas répondu à ListSets
        ] {
            assert!(matches!(s.on_key(key), Action::None), "{key:?} a produit une action");
            assert!(s.set_picker.is_some(), "{key:?} a fermé la fenêtre");
            assert_eq!(
                before,
                format!("{:?}", (&s.fx.chain, s.space, s.bank.is_some(), &s.naming)),
                "{key:?} a agi derrière la fenêtre"
            );
        }
        // Échap, lui, la referme : c'est sa seule sortie avec `L` et `Entrée`
        s.on_key(Key::Esc);
        assert!(s.set_picker.is_none());
    }

    /// **Un panneau ne retombe pas non plus**. Les réglages d'un effet sont un quasi-modal : ce
    /// qu'ils laisseraient passer agirait sur la chaîne **derrière** eux — `Retour` et `x`
    /// retireraient le bloc dont on règle justement les paramètres, en laissant le focus sur un
    /// bloc qui n'en a aucun, `s` le mettrait en parallèle, `a` ouvrirait le catalogue, `n`
    /// viderait la chaîne.
    ///
    /// La garde est une liste **blanche** : une liste de refus échouerait *ouverte*, et la
    /// prochaine touche d'édition agirait de nouveau derrière le panneau.
    #[test]
    fn the_params_panel_does_not_fall_back_to_the_chain() {
        use crate::browse::state::fx::FxFocus;
        let start = || {
            let mut s = with_chain(vec![fx_block("reverb")]);
            s.on_key(Key::Enter);
            assert_eq!(s.fx.focus, FxFocus::Params, "les réglages sont ouverts");
            s
        };
        let before = format!("{:?}", start().fx.chain);
        for key in [
            Key::Backspace,
            Key::Char('x'),
            Key::Char('s'),
            Key::Char('+'),
            Key::Char('a'),
            Key::Char('i'),
            Key::Char('b'),
            Key::Char('['),
            Key::Char('{'),
            Key::Char('m'),
            Key::Char('n'),
            Key::Char('t'),
            Key::Char('é'),
        ] {
            let mut s = start();
            assert!(matches!(s.on_key(key), Action::None), "{key:?} a produit une action");
            assert_eq!(before, format!("{:?}", s.fx.chain), "{key:?} a édité la chaîne");
            assert_eq!(s.fx.focus, FxFocus::Params, "{key:?} a quitté les réglages");
            assert!(s.fx.catalog.is_none(), "{key:?} a ouvert le catalogue");
            assert!(!s.show_toml && s.show_meters, "{key:?} a changé l'écran derrière le panneau");
            assert_eq!(s.unhandled, None, "{key:?} : le panneau la possède, elle n'est pas orpheline");
        }
        // la liste blanche, elle, passe : ce qui **écoute** ou **montre** traverse le panneau,
        // parce que c'est ce qu'on fait en réglant un paramètre
        let mut s = start();
        s.on_key(Key::Char('.'));
        assert!(s.fx.chain.volume_db > 0.0, "le volume du preset traverse le panneau");
        assert_eq!(s.fx.focus, FxFocus::Params, "et on reste dans les réglages");
        let mut s = start();
        s.on_key(Key::Char('r'));
        assert!(s.fx.bypass_all, "l'A/B sans effets traverse : on compare en réglant");
        assert_eq!(s.fx.focus, FxFocus::Params);
        let mut s = start();
        s.on_key(Key::Char('v'));
        assert!(!s.show_meters, "les vumètres traversent");
        assert_eq!(s.fx.focus, FxFocus::Params);
        // ...et `S` / `P` descendent aux communes
        let mut s = start();
        s.on_key(Key::Char('P'));
        assert!(s.bank.is_some(), "P ouvre la banque");
        // **le geste du pédalier ne se bloque jamais** : PgDn / PgUp changent de preset, et le
        // panneau se referme parce que son sujet — le bloc qu'on réglait — a disparu avec la
        // chaîne. Le refuser aurait été une régression live, et la fiabilité live prime.
        let mut s = with_chain(vec![fx_block("reverb")]);
        s.adopt_presets(vec![(
            "p1".into(),
            "Punk".into(),
            "/x/a.preset.toml".into(),
            with_chain(vec![fx_block("chorus")]).to_preset("Punk"),
        )]);
        s.mark_start();
        s.on_key(Key::Enter);
        assert_eq!(s.fx.focus, FxFocus::Params);
        s.on_key(Key::PageDown);
        assert_eq!(s.fx.focus, FxFocus::Chain, "le panneau se referme : son bloc n'est plus là");
        assert!(s.preset.is_some(), "et le preset a bien été chargé");
        // l'étage pédale, lui, n'a jamais traversé le panneau : il est au-dessus
        let mut s = start();
        s.on_key(Key::Char('1'));
        assert_eq!(s.space, Space::Library, "1 change d'espace depuis les réglages");
    }

    /// Une touche que **personne** ne possède est retenue pour la trace, et **rien** ne s'affiche :
    /// on n'apprend pas à l'utilisateur à ignorer la zone de message, qui ne dit que du vrai.
    #[test]
    fn an_orphan_key_is_kept_for_the_trace_and_says_nothing() {
        let mut s = sample();
        s.message = None;
        s.on_key(Key::Char('é'));
        assert_eq!(s.unhandled, Some(Key::Char('é')));
        assert_eq!(s.message, None, "rien à l'écran");
        // une touche possédée ne la remplit pas
        s.unhandled = None;
        s.on_key(Key::Char('1'));
        assert_eq!(s.unhandled, None);
    }
}
