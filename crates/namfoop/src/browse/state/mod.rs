//! Logique du navigateur, sans terminal ni audio : touches + horloge -> actions.
//! L'horloge est fournie par l'appelant (`tick(ms)`), ce qui rend l'écoute différée testable.

use std::path::PathBuf;

use crate::tui::Key;
use library::recommend::NeedsIr;

/// Délai sans mouvement avant d'écouter l'élément sélectionné (évite de recharger à chaque touche).
pub const AUDITION_DELAY_MS: u64 = 150;

/// Délai sans changement d'entrée avant de mesurer le grain dans l'éditeur.
pub const GRAIN_DELAY_MS: u64 = 300;

/// Les onglets de la **Bibliothèque** (la Chaîne est un espace, pas un onglet : § 2 et § 6 de
/// `docs/design/browse-espaces.md`).
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
    pub profile: Option<library::profile::Described>,
    /// IR : c'est un **mélange enregistré** (`library/cabs/`). Ils passent en tête de la liste du
    /// sélecteur de la Chaîne (§ 5).
    pub blend: bool,
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

/// Ce que l'A/B fait entendre (indicateur permanent de l'en-tête).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbState {
    /// Pas de référence épinglée.
    NoReference,
    /// La référence est l'IR choisie : rien à comparer.
    Same,
    /// On entend ton choix.
    A,
    /// On entend la référence.
    B,
    /// IR coupée (`b`).
    Bypass,
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
    /// Mesure le grain de ce modèle avec cette entrée propre (éditeur, hors temps réel).
    MeasureGrain {
        model: usize,
        input_db: f32,
    },
    Quit,
}

pub struct State {
    /// L'espace où l'on travaille, et donc ce qu'on entend (§ 2).
    pub space: space::Space,
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
    /// IR de référence épinglée (`c`), superposée dans le panneau spectre.
    pub reference: Option<usize>,
    /// Modèle de référence épinglé (`c` dans l'onglet Modèles) ; exclusif avec `reference`.
    pub ref_model: Option<usize>,
    /// Éditeur : grain mesuré pour l'entrée en cours d'essai.
    pub editor_grain: Option<String>,
    grain_due: Option<u64>,
    /// A/B : on écoute la référence au lieu de l'IR choisie (`r`).
    pub listen_ref: bool,
    /// A/B : IR coupée, modèle seul ou son direct (`b`).
    pub bypass: bool,
    /// Profil du mélange en cours (calculé par la boucle à chaque changement).
    pub blend_profile: Option<std::sync::Arc<dsp::profile::IrProfile>>,
    /// Onglet Effets : catalogue, chaîne pré / post, réglages.
    pub fx: fx::FxState,
    /// Sélecteur sur place d'un bloc `nam` / `ir` de la chaîne (§ 5), quand il est ouvert.
    pub picker: Option<picker::Picker>,
    /// Fenêtre « Mettre « … » dans la chaîne » (§ 4), quand elle est ouverte.
    pub send: Option<send::Send>,
    /// Mode pose : où poser ce qu'on envoie dans la chaîne (§ 4).
    pub place: Option<send::Place>,
    /// Effets changés : écoute à reconstruire après `AUDITION_DELAY_MS` sans touche.
    fx_due: Option<u64>,
    /// Vumètres par bloc et par branche de la chaîne **en écoute** (#107) : la table arrive avec la
    /// chaîne construite, `meters` garde ce que les barres montrent.
    pub chain_meters: Option<dsp::meter::ChainMeters>,
    pub meters: meters::MeterDisplay,
    /// `v` : vumètres du synoptique affichés ou masqués.
    pub show_meters: bool,
    /// Fenêtre « Modifications non enregistrées » ouverte par `q` : seul `Q` quitte.
    pub confirm_quit: bool,
    /// Chaîne d'effets au lancement et dernier mélange enregistré : ce qui est « modifié » se
    /// décide en **comparant des états** (jamais un drapeau posé à la main), pour que revenir à
    /// l'état de départ efface l'alerte.
    start_fx: chain::Chain,
    saved_blend: (Vec<BlendPart>, bool),
    pending: Option<u64>,
    now: u64,
}

impl State {
    pub fn new(models: Vec<Item>, irs: Vec<Item>) -> Self {
        Self {
            space: space::Space::Library,
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
            reference: None,
            ref_model: None,
            editor_grain: None,
            grain_due: None,
            listen_ref: false,
            bypass: false,
            blend_profile: None,
            fx: fx::FxState::default(),
            picker: None,
            send: None,
            place: None,
            fx_due: None,
            chain_meters: None,
            meters: meters::MeterDisplay::default(),
            show_meters: true,
            confirm_quit: false,
            start_fx: chain::Chain::default(),
            saved_blend: (Vec::new(), true),
            now: 0,
            pending: None,
        }
    }

    fn tab_index(&self) -> usize {
        self.tab as usize
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

    /// Demande l'écoute de ce qu'on a choisi. **Parcourir n'écrit jamais dans la chaîne** (§ 2,
    /// règle 1) : le modèle et l'IR du banc restent au banc, et c'est `plan()` qui décide de la
    /// chaîne réellement installée selon l'espace.
    fn play(&mut self) -> Action {
        Action::Play { model: self.effective_model(), ir: self.effective_ir() }
    }

    /// IR sous le curseur de l'onglet IR.
    pub fn ir_at_cursor(&self) -> Option<usize> {
        match self.ir_rows().get(self.cursor()) {
            Some(IrRow::Ir { index, .. }) if self.tab == Tab::Irs => Some(*index),
            _ => None,
        }
    }

    /// Applique la sélection courante (modèle ou IR sous le curseur) et demande l'écoute.
    fn select_current(&mut self) -> Action {
        self.apply_cursor();
        self.play()
    }

    /// Pose au banc le modèle (ou l'IR) sous le curseur, sans demander l'écoute.
    pub(super) fn apply_cursor(&mut self) {
        self.pending = None;
        match self.tab {
            Tab::Models => {
                let m = match self.model_list().get(self.cursor()) {
                    Some(ModelRow::Model(m)) => *m,
                    Some(ModelRow::Direct) => {
                        self.model = None;
                        self.fresh();
                        self.message = Some(match self.ir {
                            IrChoice::None => {
                                "son direct de la basse : choisis une IR (onglet IR) pour l'écouter seule"
                                    .into()
                            }
                            _ => "son direct de la basse + IR (sans modèle)".into(),
                        });
                        return;
                    }
                    None => return,
                };
                self.model = Some(m);
                self.fresh();
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
            }
            Tab::Irs => {
                self.fresh();
                match self.ir_rows().get(self.cursor()) {
                    Some(IrRow::NoIr) => self.ir = IrChoice::None,
                    Some(IrRow::Ir { index, .. }) => self.ir = IrChoice::Item(*index),
                    None => {}
                }
            }
            Tab::Blend => {}
        }
    }

    /// Horloge (ms) : déclenche l'écoute différée et la mesure du grain de l'éditeur.
    pub fn tick(&mut self, now_ms: u64) -> Action {
        self.now = now_ms;
        if let (Some(t), Some(ed)) = (self.grain_due, &self.editor) {
            if now_ms >= t + GRAIN_DELAY_MS {
                self.grain_due = None;
                return Action::MeasureGrain { model: ed.model, input_db: ed.draft.input_db };
            }
        }
        if self.fx_due.is_some_and(|t| now_ms >= t + AUDITION_DELAY_MS) {
            self.fx_due = None;
            return self.play();
        }
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

    pub fn on_key(&mut self, key: Key) -> Action {
        if self.confirm_quit {
            return self.confirm_quit_key(key);
        }
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
        // la fenêtre « mettre dans la chaîne » et le mode pose prennent la main (§ 4)
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
        // `1` / `2` changent d'espace depuis n'importe où (hors saisie et hors choix en cours)
        match key {
            Key::Char('1') => return self.go_to(space::Space::Library),
            Key::Char('2') => return self.go_to(space::Space::Chain),
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

mod blend;
pub mod chain;
mod compare;
mod edit;
mod editor;
pub mod fx;
mod label;
mod lists;
pub mod meters;
pub mod picker;
mod quit;
pub mod send;
pub mod space;
mod target;

#[cfg(test)]
pub mod tests;
