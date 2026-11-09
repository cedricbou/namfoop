//! Logique du navigateur, sans terminal ni audio : touches + horloge -> actions.
//! L'horloge est fournie par l'appelant (`tick(ms)`), ce qui rend l'écoute différée testable.

use std::path::PathBuf;

use crate::recommend::NeedsIr;
use crate::tui::Key;

/// Délai sans mouvement avant d'écouter l'élément sélectionné (évite de recharger à chaque touche).
pub const AUDITION_DELAY_MS: u64 = 150;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Models,
    Irs,
    Blend,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    /// Nom affiché (préfixe commun du pack retiré pour les IR).
    pub label: String,
    pub pack: String,
    pub path: PathBuf,
    /// Chemin relatif à la bibliothèque (pour les mélanges sauvés).
    pub rel: String,
    /// Résumé (type de capture...).
    pub detail: String,
    pub needs: NeedsIr,
    /// Modèles : index des IR conseillées, meilleure d'abord.
    pub recommended: Vec<usize>,
    /// Modèles : métadonnées effectives (corrections comprises) et celles du fichier.
    pub meta: Option<Box<(ModelMeta, ModelMeta)>>,
    /// IR et mélanges : profil sonore (mini-spectre, étiquettes, courbe).
    pub profile: Option<crate::profile::Described>,
}

impl Item {
    /// Le modèle porte des corrections de l'utilisateur.
    pub fn corrected(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| m.0 != m.1)
    }
}

/// Métadonnées modifiables d'un modèle (éditeur `e`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelMeta {
    pub gear_type: String,
    pub make: String,
    pub name: String,
    /// Gain d'entrée propre au modèle (dB).
    pub input_db: f32,
    /// IR préférée (index dans `irs`).
    pub ir: Option<usize>,
}

/// Types de capture proposés par l'éditeur.
pub const GEAR_TYPES: &[&str] = &["amp", "amp_cab", "full_rig", "pedal", "preamp"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Type,
    Input,
    Ir,
    Make,
    Name,
}

pub const FIELDS: [Field; 5] = [Field::Type, Field::Input, Field::Ir, Field::Make, Field::Name];

/// Éditeur des métadonnées du modèle en écoute.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub model: usize,
    pub field: usize,
    pub draft: ModelMeta,
    /// Saisie de texte en cours (marque, nom).
    pub typing: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrChoice {
    None,
    Item(usize),
    Blend,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendPart {
    pub ir: usize,
    pub gain_db: f32,
    pub invert: bool,
}

/// Ligne de l'onglet Modèles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelRow {
    /// Son direct de la basse (DI), sans modèle : pour écouter les IR seules.
    Direct,
    Model(usize),
}

/// Ligne de l'onglet IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrRow {
    NoIr,
    Ir { index: usize, recommended: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Play {
        model: Option<usize>,
        ir: IrChoice,
    },
    SaveBlend(String),
    /// Enregistre les métadonnées de ces modèles (`library/overrides.toml`).
    SaveMeta(Vec<(usize, ModelMeta)>),
    Quit,
}

pub struct State {
    pub tab: Tab,
    pub models: Vec<Item>,
    pub irs: Vec<Item>,
    pub cursor: [usize; 3],
    pub filter: String,
    pub filtering: bool,
    pub model: Option<usize>,
    pub ir: IrChoice,
    pub blend: Vec<BlendPart>,
    pub align: bool,
    pub naming: Option<String>,
    pub editor: Option<Editor>,
    pub message: Option<String>,
    pending: Option<u64>,
    now: u64,
}

fn matches(item: &Item, words: &[String]) -> bool {
    let hay = format!("{} {} {}", item.id, item.label, item.pack).to_lowercase();
    words.iter().all(|w| hay.contains(w))
}

impl State {
    pub fn new(models: Vec<Item>, irs: Vec<Item>) -> Self {
        Self {
            tab: Tab::Models,
            models,
            irs,
            cursor: [0; 3],
            filter: String::new(),
            filtering: false,
            model: None,
            ir: IrChoice::None,
            blend: Vec::new(),
            align: true,
            naming: None,
            editor: None,
            message: None,
            pending: None,
            now: 0,
        }
    }

    fn tab_index(&self) -> usize {
        self.tab as usize
    }

    fn words(&self) -> Vec<String> {
        self.filter.to_lowercase().split_whitespace().map(String::from).collect()
    }

    /// Modèles visibles (index dans `models`).
    pub fn model_rows(&self) -> Vec<usize> {
        let w = self.words();
        (0..self.models.len()).filter(|&i| matches(&self.models[i], &w)).collect()
    }

    /// Lignes de l'onglet Modèles : « son direct (DI) » (sans filtre), puis les modèles visibles.
    pub fn model_list(&self) -> Vec<ModelRow> {
        let mut rows = Vec::new();
        if self.filter.trim().is_empty() {
            rows.push(ModelRow::Direct);
        }
        rows.extend(self.model_rows().into_iter().map(ModelRow::Model));
        rows
    }

    /// Modèle sous le curseur de l'onglet Modèles (aucun sur la ligne « son direct »).
    fn model_at_cursor(&self) -> Option<usize> {
        match self.model_list().get(self.cursor()) {
            Some(ModelRow::Model(m)) => Some(*m),
            _ => None,
        }
    }

    /// Lignes de l'onglet IR : « aucune IR », les conseillées pour le modèle courant, puis le reste.
    pub fn ir_rows(&self) -> Vec<IrRow> {
        let w = self.words();
        let rec: Vec<usize> = self
            .model
            .filter(|&m| self.models[m].needs != NeedsIr::No)
            .map(|m| self.models[m].recommended.iter().copied().take(5).collect())
            .unwrap_or_default();
        let mut rows = Vec::new();
        if w.is_empty() {
            rows.push(IrRow::NoIr);
        }
        rows.extend(
            rec.iter()
                .filter(|&&i| matches(&self.irs[i], &w))
                .map(|&index| IrRow::Ir { index, recommended: true }),
        );
        rows.extend(
            (0..self.irs.len())
                .filter(|i| !rec.contains(i) && matches(&self.irs[*i], &w))
                .map(|index| IrRow::Ir { index, recommended: false }),
        );
        rows
    }

    fn row_count(&self) -> usize {
        match self.tab {
            Tab::Models => self.model_list().len(),
            Tab::Irs => self.ir_rows().len(),
            Tab::Blend => self.blend.len(),
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor[self.tab_index()].min(self.row_count().saturating_sub(1))
    }

    fn play(&self) -> Action {
        Action::Play { model: self.model, ir: self.ir }
    }

    /// Applique la sélection courante (modèle ou IR sous le curseur) et demande l'écoute.
    fn select_current(&mut self) -> Action {
        self.pending = None;
        match self.tab {
            Tab::Models => {
                let m = match self.model_list().get(self.cursor()) {
                    Some(ModelRow::Model(m)) => *m,
                    Some(ModelRow::Direct) => {
                        self.model = None;
                        self.message = Some(match self.ir {
                            IrChoice::None => {
                                "son direct de la basse : choisis une IR (onglet IR) pour l'écouter seule"
                                    .into()
                            }
                            _ => "son direct de la basse + IR (sans modèle)".into(),
                        });
                        return self.play();
                    }
                    None => return Action::None,
                };
                self.model = Some(m);
                let needs = self.models[m].needs;
                match (needs, self.ir) {
                    (NeedsIr::No, IrChoice::Item(_) | IrChoice::Blend) => {
                        self.ir = IrChoice::None;
                        self.message = Some("baffle inclus dans la capture : IR retirée".into());
                    }
                    (NeedsIr::Yes, IrChoice::None) => {
                        if let Some(&best) = self.models[m].recommended.first() {
                            self.ir = IrChoice::Item(best);
                            self.message = Some(format!("IR conseillée : {}", self.irs[best].label));
                        }
                    }
                    _ => {}
                }
                self.play()
            }
            Tab::Irs => {
                match self.ir_rows().get(self.cursor()) {
                    Some(IrRow::NoIr) => self.ir = IrChoice::None,
                    Some(IrRow::Ir { index, .. }) => self.ir = IrChoice::Item(*index),
                    None => return Action::None,
                }
                self.play()
            }
            Tab::Blend => Action::None,
        }
    }

    /// Horloge (ms) : déclenche l'écoute différée.
    pub fn tick(&mut self, now_ms: u64) -> Action {
        self.now = now_ms;
        match self.pending {
            Some(t) if now_ms >= t + AUDITION_DELAY_MS => self.select_current(),
            _ => Action::None,
        }
    }

    fn moved(&mut self) {
        if matches!(self.tab, Tab::Models | Tab::Irs) {
            self.pending = Some(self.now);
        }
    }

    fn set_cursor(&mut self, c: usize) {
        let n = self.row_count();
        self.cursor[self.tab_index()] = c.min(n.saturating_sub(1));
        self.moved();
    }

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

    fn open_editor(&mut self) -> Action {
        let Some(m) = self.model_at_cursor() else {
            self.message = Some("son direct : pas de modèle à modifier".into());
            return Action::None;
        };
        let draft = self.saved_meta(m);
        // un modèle nouvellement choisi prend sa meilleure IR conseillée (la préférée en tête) ;
        // s'il joue déjà, on garde l'IR en écoute (Entrée sur « IR préférée » la retient)
        let action = if self.model != Some(m) { self.select_current() } else { Action::None };
        self.editor = Some(Editor { model: m, field: 0, draft, typing: None });
        self.message = None;
        action
    }

    fn editor_key(&mut self, key: Key) -> Action {
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
        match key {
            Key::Esc | Key::Char('q') => {
                let revert = ed.draft.input_db != self.saved_meta(m).input_db;
                self.editor = None;
                if revert {
                    return self.play();
                }
            }
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

    pub fn on_key(&mut self, key: Key) -> Action {
        if self.editor.is_some() {
            return self.editor_key(key);
        }
        // saisie du nom du mélange
        if let Some(name) = self.naming.as_mut() {
            match key {
                Key::Enter => {
                    let n = name.trim().to_string();
                    self.naming = None;
                    if n.is_empty() {
                        self.message = Some("nom vide : mélange non sauvé".into());
                        return Action::None;
                    }
                    return Action::SaveBlend(n);
                }
                Key::Esc => self.naming = None,
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
                Key::Enter | Key::Esc | Key::Tab | Key::Up | Key::Down => {
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
        let c = self.cursor();
        match key {
            Key::Esc | Key::Char('q') => return Action::Quit,
            Key::Char('/') => {
                if self.tab != Tab::Blend {
                    self.filtering = true;
                }
            }
            Key::Tab | Key::Right if self.tab != Tab::Blend || key == Key::Tab => {
                self.tab = match self.tab {
                    Tab::Models => Tab::Irs,
                    Tab::Irs => Tab::Blend,
                    Tab::Blend => Tab::Models,
                };
                self.filter.clear();
                self.pending = None;
            }
            Key::Left if self.tab != Tab::Models => {
                self.tab = if self.tab == Tab::Blend { Tab::Irs } else { Tab::Models };
                self.filter.clear();
                self.pending = None;
            }
            Key::Up => self.set_cursor(c.saturating_sub(1)),
            Key::Down => self.set_cursor(c + 1),
            Key::PageUp => self.set_cursor(c.saturating_sub(10)),
            Key::PageDown => self.set_cursor(c + 10),
            Key::Enter => return self.select_current(),
            Key::Char('e') if self.tab == Tab::Models => return self.open_editor(),
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

    fn adjust(&mut self, f: impl FnOnce(&mut BlendPart)) -> Action {
        let c = self.cursor();
        if let Some(p) = self.blend.get_mut(c) {
            f(p);
            p.gain_db = p.gain_db.clamp(-30.0, 12.0);
            return self.blend_changed();
        }
        Action::None
    }

    /// Le mélange a changé : on l'écoute (ou on revient à la dernière IR s'il est vide).
    fn blend_changed(&mut self) -> Action {
        if self.blend.is_empty() {
            if self.ir == IrChoice::Blend {
                self.ir = IrChoice::None;
            }
        } else {
            self.ir = IrChoice::Blend;
        }
        self.message = Some(format!("mélange : {} IR", self.blend.len()));
        self.play()
    }

    pub fn default_blend_name(&self) -> String {
        let first = self.blend.first().map(|p| &self.irs[p.ir]);
        let pack = first
            .map(|i| i.pack.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        format!("{pack} mix {}", self.blend.len())
    }

    /// Ligne de commande équivalente (affichée en quittant).
    pub fn command_line(&self, saved_blend_id: Option<&str>) -> Option<String> {
        if self.model.is_none() && self.ir == IrChoice::None {
            return None;
        }
        let mut cmd = "namfoop run".to_string();
        if let Some(m) = self.model {
            cmd += &format!(" -m {}", self.models[m].id);
        }
        match self.ir {
            IrChoice::Item(i) => cmd += &format!(" -i {}", self.irs[i].id),
            IrChoice::Blend => match saved_blend_id {
                Some(id) => cmd += &format!(" -i {id}"),
                None => cmd += "   (mélange non sauvé : s dans l'onglet Mélange)",
            },
            IrChoice::None => {}
        }
        Some(cmd)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn item(id: &str, label: &str, pack: &str, needs: NeedsIr, rec: Vec<usize>) -> Item {
        Item {
            id: id.into(),
            label: label.into(),
            pack: pack.into(),
            path: PathBuf::from(format!("/lib/{pack}/{label}")),
            rel: format!("ir/{pack}/{label}.wav"),
            detail: String::new(),
            needs,
            recommended: rec,
            meta: None,
            profile: None,
        }
    }

    pub fn sample() -> State {
        let models = vec![
            item("m001", "B7K Ultra", "Darkglass", NeedsIr::No, vec![]),
            item("m002", "800D Modern", "MESA 800D", NeedsIr::Yes, vec![2, 0]),
            item("m003", "Angel_v2", "Trace", NeedsIr::Yes, vec![1]),
        ];
        let irs = vec![
            item("i001", "121 Lower - Cap", "1970 Fender", NeedsIr::Maybe, vec![]),
            item("i002", "Beta52 Lower - Cap", "1970 Fender", NeedsIr::Maybe, vec![]),
            item("i003", "Powerhouse 410", "Mesa Powerhouse", NeedsIr::Maybe, vec![]),
        ];
        State::new(models, irs)
    }

    #[test]
    fn audition_after_delay_and_auto_ir() {
        let mut s = sample();
        s.tick(1000);
        s.on_key(Key::Down); // ligne « son direct »
        s.on_key(Key::Down); // 800D
        assert_eq!(s.tick(1100), Action::None, "pas encore : délai de 150 ms");
        let a = s.tick(1160);
        assert_eq!(a, Action::Play { model: Some(1), ir: IrChoice::Item(2) }, "IR conseillée posée d'office");
        assert_eq!(s.tick(1500), Action::None, "une seule écoute");
        s.on_key(Key::Up); // B7K : baffle inclus -> IR retirée
        assert_eq!(s.tick(2000), Action::Play { model: Some(0), ir: IrChoice::None });
    }

    #[test]
    fn ir_tab_lists_recommended_first_and_filters() {
        let mut s = sample();
        s.on_key(Key::Down); // ligne « son direct »
        s.on_key(Key::Down);
        s.on_key(Key::Enter); // 800D
        s.on_key(Key::Tab);
        assert_eq!(s.tab, Tab::Irs);
        let rows = s.ir_rows();
        assert_eq!(rows[0], IrRow::NoIr);
        assert_eq!(rows[1], IrRow::Ir { index: 2, recommended: true });
        assert_eq!(rows[2], IrRow::Ir { index: 0, recommended: true });
        s.on_key(Key::Char('/'));
        for c in "beta".chars() {
            s.on_key(Key::Char(c));
        }
        assert!(s.filtering);
        assert_eq!(s.ir_rows(), vec![IrRow::Ir { index: 1, recommended: false }]);
        s.on_key(Key::Enter); // sort du filtre
        assert!(!s.filtering && s.filter == "beta");
        assert_eq!(s.on_key(Key::Enter), Action::Play { model: Some(1), ir: IrChoice::Item(1) });
    }

    #[test]
    fn letters_are_commands_outside_the_filter() {
        let mut s = sample();
        assert_eq!(s.on_key(Key::Char('q')), Action::Quit);
        s.on_key(Key::Char('/'));
        assert_eq!(s.on_key(Key::Char('q')), Action::None, "dans le filtre, q s'écrit");
        assert_eq!(s.filter, "q");
    }

    #[test]
    fn build_and_save_a_blend() {
        let mut s = sample();
        s.on_key(Key::Down); // ligne « son direct »
        s.on_key(Key::Down);
        s.on_key(Key::Enter);
        s.on_key(Key::Tab); // IR
        s.on_key(Key::Char('/'));
        for c in "fender".chars() {
            s.on_key(Key::Char(c));
        }
        s.on_key(Key::Enter);
        assert_eq!(s.on_key(Key::Char(' ')), Action::Play { model: Some(1), ir: IrChoice::Blend });
        s.on_key(Key::Down);
        s.on_key(Key::Char(' '));
        assert_eq!(s.blend.len(), 2);
        s.on_key(Key::Tab); // Mélange
        assert_eq!(s.tab, Tab::Blend);
        s.on_key(Key::Down);
        s.on_key(Key::Char('-'));
        s.on_key(Key::Char('-'));
        s.on_key(Key::Char('p'));
        assert_eq!(s.blend[1], BlendPart { ir: 1, gain_db: -2.0, invert: true });
        assert_eq!(s.on_key(Key::Char('a')), Action::Play { model: Some(1), ir: IrChoice::Blend });
        assert!(!s.align);
        s.on_key(Key::Char('s'));
        assert_eq!(s.naming.as_deref(), Some("1970 Fender mix 2"));
        for _ in 0..5 {
            s.on_key(Key::Backspace);
        }
        for c in "121+b52".chars() {
            s.on_key(Key::Char(c));
        }
        assert_eq!(s.on_key(Key::Enter), Action::SaveBlend("1970 Fender 121+b52".into()));
        assert_eq!(s.command_line(Some("c9zz")).unwrap(), "namfoop run -m m002 -i c9zz");
        // retirer tout le mélange revient à « aucune IR »
        s.on_key(Key::Char('x'));
        s.on_key(Key::Char('x'));
        assert_eq!(s.ir, IrChoice::None);
    }

    fn with_meta(mut s: State) -> State {
        for m in s.models.iter_mut() {
            let meta = ModelMeta { gear_type: "amp".into(), name: m.label.clone(), ..Default::default() };
            m.meta = Some(Box::new((meta.clone(), meta)));
        }
        s
    }

    #[test]
    fn editor_changes_type_input_and_ir_live() {
        let mut s = with_meta(sample());
        s.on_key(Key::Down); // ligne « son direct »
        s.on_key(Key::Down); // 800D
        let a = s.on_key(Key::Char('e'));
        assert_eq!(a, Action::Play { model: Some(1), ir: IrChoice::Item(2) }, "ouvre et joue le modèle");
        assert!(s.editor.is_some());
        // type : amp -> amp_cab
        assert_eq!(s.on_key(Key::Right), Action::None);
        assert_eq!(s.editor.as_ref().unwrap().draft.gear_type, "amp_cab");
        // entrée : -2 dB, écoutée tout de suite
        s.on_key(Key::Down);
        s.on_key(Key::Left);
        assert_eq!(s.on_key(Key::Left), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
        assert_eq!(s.input_offset(), -2.0);
        // IR : aucune -> 1re conseillée (2) -> 2e (0), jouée
        s.on_key(Key::Down);
        assert_eq!(s.on_key(Key::Right), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
        assert_eq!(s.on_key(Key::Right), Action::Play { model: Some(1), ir: IrChoice::Item(0) });
        // marque : saisie
        s.on_key(Key::Down);
        s.on_key(Key::Enter);
        for c in "Mesa".chars() {
            s.on_key(Key::Char(c));
        }
        assert_eq!(s.on_key(Key::Char('q')), Action::None, "q s'écrit pendant la saisie");
        s.on_key(Key::Backspace);
        s.on_key(Key::Enter);
        let Action::SaveMeta(v) = s.on_key(Key::Char('s')) else { panic!("sauvegarde attendue") };
        assert_eq!(
            v,
            vec![(
                1,
                ModelMeta {
                    gear_type: "amp_cab".into(),
                    make: "Mesa".into(),
                    name: "800D Modern".into(),
                    input_db: -2.0,
                    ir: Some(0),
                }
            )]
        );
        assert!(s.editor.is_none());
    }

    #[test]
    fn editor_escape_reverts_audition_and_apply_to_filtered() {
        let mut s = with_meta(sample());
        s.on_key(Key::Char('/'));
        for c in "a".chars() {
            s.on_key(Key::Char(c)); // « a » : 800D Modern (MESA), Angel_v2 (Trace), B7K Ultra (Darkglass)
        }
        s.on_key(Key::Enter);
        s.on_key(Key::Char('e'));
        s.on_key(Key::Down);
        s.on_key(Key::Right); // entrée +1
        assert_eq!(s.input_offset(), 1.0);
        assert!(matches!(s.on_key(Key::Esc), Action::Play { .. }), "Échap : retour au réglage enregistré");
        assert_eq!(s.input_offset(), 0.0);
        let n = s.model_rows().len();
        assert_eq!(n, 3);
        s.on_key(Key::Char('e'));
        s.on_key(Key::Right); // amp -> amp_cab
        let Action::SaveMeta(v) = s.on_key(Key::Char('a')) else { panic!() };
        assert_eq!(v.len(), n);
        assert!(v.iter().all(|(_, m)| m.gear_type == "amp_cab" && m.input_db == 0.0), "type seul");
        // entrée seule : le type de chaque modèle est gardé
        s.models[2].meta.as_mut().unwrap().0.gear_type = "pedal".into();
        s.on_key(Key::Char('e'));
        assert_eq!(s.on_key(Key::Char('a')), Action::None, "rien de modifié");
        s.on_key(Key::Down);
        s.on_key(Key::Left);
        let Action::SaveMeta(v) = s.on_key(Key::Char('a')) else { panic!() };
        assert!(v.iter().all(|(_, m)| m.input_db == -1.0));
        assert!(v.iter().any(|(_, m)| m.gear_type == "pedal") && v.iter().any(|(_, m)| m.gear_type == "amp"));
    }

    #[test]
    fn ir_chosen_in_ir_tab_becomes_preferred() {
        let mut s = with_meta(sample());
        s.on_key(Key::Down); // ligne « son direct »
        s.on_key(Key::Down);
        s.on_key(Key::Enter); // 800D, IR conseillée 2
        s.on_key(Key::Tab);
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Enter); // IR 1 (non conseillée)
        assert_eq!(s.ir, IrChoice::Item(1));
        s.on_key(Key::Left);
        assert_eq!(s.on_key(Key::Char('e')), Action::None, "même modèle : le son ne change pas");
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Enter); // champ IR préférée : prend l'IR en écoute
        assert_eq!(s.editor.as_ref().unwrap().draft.ir, Some(1));
    }

    #[test]
    fn direct_row_plays_irs_without_model() {
        let mut s = sample();
        assert_eq!(s.model_list()[0], ModelRow::Direct);
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Enter); // 800D + IR conseillée
        assert_eq!(s.model, Some(1));
        s.on_key(Key::Up);
        s.on_key(Key::Up);
        assert_eq!(
            s.on_key(Key::Enter),
            Action::Play { model: None, ir: IrChoice::Item(2) },
            "IR gardée, sans modèle"
        );
        s.on_key(Key::Tab);
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        assert_eq!(s.on_key(Key::Enter), Action::Play { model: None, ir: IrChoice::Item(1) });
        assert_eq!(s.command_line(None).unwrap(), "namfoop run -i i002");
        s.on_key(Key::Left);
        assert_eq!(s.on_key(Key::Char('e')), Action::None, "pas d'éditeur sans modèle");
        assert!(s.editor.is_none());
        // avec un filtre, la ligne disparaît
        s.filter = "mesa".into();
        assert_eq!(s.model_list(), vec![ModelRow::Model(1)]);
    }
}
