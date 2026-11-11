//! Éditeur `e` des métadonnées d'un modèle : type, entrée (écoute directe, grain remesuré),
//! IR préférée, marque, nom ; application aux modèles filtrés.

use super::*;

impl State {
    /// Gain d'entrée à ajouter à l'écoute : brouillon de l'éditeur moins la valeur enregistrée
    /// (déjà appliquée par la chaîne).
    pub fn input_offset(&self) -> f32 {
        match &self.editor {
            Some(e) if self.model == Some(e.model) => e.draft.input_db - self.saved_meta(e.model).input_db,
            _ => 0.0,
        }
    }

    pub fn saved_meta(&self, m: usize) -> ModelMeta {
        self.models[m].meta.as_ref().map(|b| b.0.clone()).unwrap_or_default()
    }

    /// Choix d'IR proposés par l'éditeur : aucune, la préférée, puis les conseillées.
    pub fn ir_options(&self, m: usize, current: Option<usize>) -> Vec<Option<usize>> {
        let mut v = vec![None];
        for i in current.into_iter().chain(self.models[m].recommended.iter().copied().take(8)) {
            if !v.contains(&Some(i)) {
                v.push(Some(i));
            }
        }
        v
    }

    pub(super) fn open_editor(&mut self) -> Action {
        let Some(m) = self.model_at_cursor() else {
            self.message = Some("son direct : pas de modèle à modifier".into());
            return Action::None;
        };
        let draft = self.saved_meta(m);
        // un modèle nouvellement choisi prend sa meilleure IR conseillée (la préférée en tête) ;
        // s'il joue déjà, on garde l'IR en écoute (Entrée sur « IR préférée » la retient)
        let action = if self.model != Some(m) { self.select_current() } else { Action::None };
        self.editor_grain = self.models[m].profile.as_ref().and_then(|d| d.grain.clone());
        self.grain_due = None;
        self.editor = Some(Editor { model: m, field: 0, draft, typing: None });
        self.message = None;
        action
    }

    pub(super) fn editor_key(&mut self, key: Key) -> Action {
        let Some(ed) = self.editor.as_mut() else { return Action::None };
        if let Some(text) = ed.typing.as_mut() {
            match key {
                Key::Enter => {
                    let t = text.trim().to_string();
                    match FIELDS[ed.field] {
                        Field::Make => ed.draft.make = t,
                        Field::Name => ed.draft.name = t,
                        _ => {}
                    }
                    ed.typing = None;
                }
                Key::Esc => ed.typing = None,
                Key::Backspace => {
                    text.pop();
                }
                Key::Char(c) => text.push(c),
                _ => {}
            }
            return Action::None;
        }
        let m = ed.model;
        if let Some(a) = self.ab_key(key) {
            return a;
        }
        let Some(ed) = self.editor.as_mut() else { return Action::None };
        match key {
            // Échap remonte d'un niveau (ferme l'éditeur) ; q veut quitter `browse`, et le
            // brouillon non enregistré de l'éditeur fait partie de ce qu'on alerterait de perdre.
            Key::Esc => {
                let revert = ed.draft.input_db != self.saved_meta(m).input_db;
                self.editor = None;
                if revert {
                    return self.play();
                }
            }
            Key::Char('q') => return self.quit_or_confirm(),
            Key::Up => ed.field = ed.field.saturating_sub(1),
            Key::Down | Key::Tab => ed.field = (ed.field + 1).min(FIELDS.len() - 1),
            Key::Left | Key::Right | Key::Char('+') | Key::Char('-') | Key::Char('=') => {
                let dir: i32 = if matches!(key, Key::Left | Key::Char('-')) { -1 } else { 1 };
                match FIELDS[ed.field] {
                    Field::Type => {
                        let pos = GEAR_TYPES.iter().position(|t| *t == ed.draft.gear_type);
                        let n = GEAR_TYPES.len() as i32;
                        let next = match pos {
                            Some(p) => (p as i32 + dir).rem_euclid(n),
                            None => 0,
                        };
                        ed.draft.gear_type = GEAR_TYPES[next as usize].to_string();
                    }
                    Field::Input => {
                        ed.draft.input_db = (ed.draft.input_db + dir as f32).clamp(-36.0, 24.0);
                        self.grain_due = Some(self.now);
                        self.editor_grain = Some("mesure…".into());
                        return self.play();
                    }
                    Field::Ir => {
                        let cur = ed.draft.ir;
                        let saved = self.saved_meta(m).ir;
                        let opts = self.ir_options(m, saved.or(cur));
                        let pos = opts.iter().position(|o| *o == cur).unwrap_or(0) as i32;
                        let next = opts[(pos + dir).rem_euclid(opts.len() as i32) as usize];
                        let ed = self.editor.as_mut().expect("ouvert");
                        ed.draft.ir = next;
                        self.ir = next.map_or(IrChoice::None, IrChoice::Item);
                        self.fresh();
                        return self.play();
                    }
                    Field::Make | Field::Name => {}
                }
            }
            Key::Enter => match FIELDS[ed.field] {
                Field::Make => ed.typing = Some(ed.draft.make.clone()),
                Field::Name => ed.typing = Some(ed.draft.name.clone()),
                Field::Ir => {
                    // prend l'IR en écoute (choisie dans l'onglet IR) comme préférée
                    if let IrChoice::Item(i) = self.ir {
                        ed.draft.ir = Some(i);
                    }
                }
                _ => {}
            },
            Key::Char('x') if FIELDS[ed.field] == Field::Ir => ed.draft.ir = None,
            Key::Char('s') => {
                let draft = ed.draft.clone();
                self.editor = None;
                self.message = Some(format!("{} enregistré", self.models[m].id));
                return Action::SaveMeta(vec![(m, draft)]);
            }
            Key::Char('a') => {
                // seuls les champs type / entrée modifiés dans l'éditeur sont appliqués à tous les
                // modèles visibles (filtre) : changer l'entrée ne touche pas au type, et inversement
                let draft = ed.draft.clone();
                let saved = self.saved_meta(m);
                let set_type = draft.gear_type != saved.gear_type;
                let set_input = draft.input_db != saved.input_db;
                if !set_type && !set_input {
                    self.message = Some("rien à appliquer : change d'abord le type ou l'entrée".into());
                    return Action::None;
                }
                let all: Vec<(usize, ModelMeta)> = self
                    .model_rows()
                    .iter()
                    .map(|&i| {
                        let mut meta = if i == m { draft.clone() } else { self.saved_meta(i) };
                        if set_type {
                            meta.gear_type = draft.gear_type.clone();
                        }
                        if set_input {
                            meta.input_db = draft.input_db;
                        }
                        (i, meta)
                    })
                    .collect();
                let mut what = Vec::new();
                if set_type {
                    what.push(format!("type « {} »", draft.gear_type));
                }
                if set_input {
                    what.push(format!("entrée {:+.0} dB", draft.input_db));
                }
                self.editor = None;
                self.message = Some(format!("{} appliqué(s) à {} modèles", what.join(" et "), all.len()));
                return Action::SaveMeta(all);
            }
            _ => {}
        }
        Action::None
    }
}
