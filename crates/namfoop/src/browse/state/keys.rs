//! Le **clavier** de `browse` : qui prend la main, et dans quel ordre. C'est le seul endroit où
//! l'ordre des couches est écrit — fenêtres (quitter, bascule de preset), saisies de texte (nom,
//! filtre des listes, **filtre du catalogue** : une saisie passe toujours avant les touches
//! communes, #175), banque, « mettre dans la chaîne » et mode pose, sélecteur sur place, puis les
//! touches de l'espace.
//!
//! Chaque couche modale rend `Some(Action)` quand elle a consommé la touche (voir `picker`,
//! `send`, `preset`), ce qui évite qu'une touche agisse à deux endroits.

use super::*;

impl State {
    pub fn on_key(&mut self, key: Key) -> Action {
        if self.confirm_quit {
            return self.confirm_quit_key(key);
        }
        if self.confirm_switch {
            return self.confirm_switch_key(key);
        }
        // l'aide : n'importe quelle touche la referme, et `h` l'ouvre hors saisie
        if self.help {
            self.help = false;
            return Action::None;
        }
        if matches!(key, Key::Char('h') | Key::Char('?')) && !self.typing() {
            self.help = true;
            self.message = None;
            return Action::None;
        }
        if self.editor.is_some() {
            return self.editor_key(key);
        }
        // saisie du nom du mélange
        if let Some(name) = self.naming.as_mut() {
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
            return Action::None;
        }
        // saisie du filtre
        if self.filtering {
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
            return Action::None;
        }
        // le filtre du catalogue de blocs se tape au clavier : il prend les lettres avant les
        // touches communes, sinon `n` (chaîne vierge) mangerait le « n » de `nam` et de `gain`
        // et effacerait la chaîne au lieu de filtrer (#175)
        if let Some(a) = self.catalog_key(key) {
            return a;
        }
        // la banque de presets, la fenêtre « mettre dans la chaîne » et le mode pose prennent la main
        if let Some(a) = self.bank_key(key) {
            return a;
        }
        if let Some(a) = self.send_key(key) {
            return a;
        }
        if let Some(a) = self.place_key(key) {
            return a;
        }
        // le sélecteur sur place prend toutes les touches : valider ou annuler d'abord (§ 5)
        if let Some(a) = self.picker_key(key) {
            return a;
        }
        // `1` / `2` changent d'espace, `S` enregistre, `P` ouvre la banque : depuis les deux espaces
        match key {
            Key::Char('1') => return self.go_to(space::Space::Library),
            Key::Char('2') => return self.go_to(space::Space::Chain),
            Key::Char('S') => return self.save_preset(),
            // `*` : le tap tempo (ADR 0008). À droite de `ù` sur AZERTY FR, sans Maj, et depuis
            // les deux espaces — comme toute touche commune, après les couches de saisie (#175).
            Key::Char('*') => return self.tap_tempo(),
            // `n` : repartir d'une chaîne vierge, depuis les deux espaces (#170)
            Key::Char('n') => return self.new_chain_or_confirm(),
            Key::Char('P') => {
                self.bank = Some(preset::Bank::default());
                self.message = if self.presets.is_empty() {
                    Some("aucun preset enregistré : S enregistre la chaîne".into())
                } else {
                    None
                };
                return Action::None;
            }
            _ => {}
        }
        if self.space == space::Space::Chain {
            if let Some(a) = self.fx_key(key) {
                return a;
            }
        }
        let c = self.cursor();
        match key {
            // Échap ne quitte jamais : ici, aucun niveau à remonter (filtre, nom, éditeur et
            // réglages d'un effet l'ont déjà consommé).
            Key::Esc => self.message = Some("Échap ne quitte pas : q pour quitter".into()),
            Key::Char('q') => return self.quit_or_confirm(),
            // `z` : le panneau Bandes. Gardé par l'espace, parce qu'une touche non traitée par la
            // Chaîne retombe ici (`fx_key` rend `None`) et ne doit pas agir à son insu.
            Key::Char('z') if self.space == space::Space::Library => self.toggle_bands(),
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
            Key::Enter => return self.send_to_chain(),
            Key::Char('e') if self.tab == Tab::Models => return self.open_editor(),
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
                        return self.play();
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
                        return self.play();
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
                    return a;
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
                    return self.blend_changed();
                }
            }
            Key::Char('+') | Key::Char('=') if self.tab == Tab::Blend => {
                return self.adjust(|p| p.gain_db += 1.0)
            }
            Key::Char('-') if self.tab == Tab::Blend => return self.adjust(|p| p.gain_db -= 1.0),
            Key::Char('p') if self.tab == Tab::Blend => return self.adjust(|p| p.invert = !p.invert),
            Key::Char('x') | Key::Backspace if self.tab == Tab::Blend => {
                if c < self.blend.len() {
                    self.blend.remove(c);
                    return self.blend_changed();
                }
            }
            Key::Char('a') if self.tab == Tab::Blend => {
                self.align = !self.align;
                return self.blend_changed();
            }
            Key::Char('s') if self.tab == Tab::Blend => {
                if self.blend.is_empty() {
                    self.message = Some("mélange vide : espace sur une IR (onglet IR) pour l'ajouter".into());
                } else {
                    self.naming = Some(self.default_blend_name());
                }
            }
            _ => {}
        }
        Action::None
    }
}
