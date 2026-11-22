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
    /// Modèles : la courbe du **modèle nu** (sans IR), mesurée avec la basse de l'utilisateur
    /// (`library::model_profile::ModelProfile::voicing`). `profile`, lui, y a déjà ajouté l'IR
    /// conseillée quand le modèle en a besoin : pour montrer la chaîne **réellement écoutée** il
    /// faut repartir du modèle nu et y poser l'IR du moment (panneau Bandes).
    pub voicing: Option<std::sync::Arc<dsp::profile::IrProfile>>,
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
    /// Écrit la chaîne en preset sous ce nom (`library/presets/<nom>.preset.toml`).
    SavePreset(String),
    /// Enregistre les métadonnées de ces modèles (`library/overrides.toml`).
    SaveMeta(Vec<(usize, ModelMeta)>),
    /// Mesure le grain de ce modèle avec cette entrée propre (éditeur, hors temps réel).
    MeasureGrain {
        model: usize,
        input_db: f32,
    },
    /// `L` : la boucle doit lister les sets de la bibliothèque (§ 7).
    ListSets,
    /// Ouvre ce set (son identifiant court) : la boucle le lit et précharge sa banque.
    OpenSet(String),
    /// **`S` en édition de set** : la boucle écrit `library/sets/<nom>.set.toml`.
    SaveSet(String),
    /// **`t`** : ouvrir le prompteur de ce morceau dans `$EDITOR`. La boucle rend la main
    /// au terminal, puis relit le fichier — l'état, lui, ne touche à aucun fichier.
    EditPrompt(usize),
    /// Le set actif a changé (ou disparu) : la banque préchargée doit être refaite.
    SetChanged,
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
    /// La banque de presets (`P`) : ce que la bibliothèque contient, chaîne déjà résolue.
    pub presets: Vec<preset::PresetItem>,
    /// Le preset dont la chaîne vient, s'il y en a un.
    pub preset: Option<preset::PresetRef>,
    /// La fenêtre de la banque, quand elle est ouverte.
    pub bank: Option<preset::Bank>,
    /// La saisie en cours nomme un **preset** (et non un mélange d'IR).
    pub naming_preset: bool,
    /// `t` : le TOML du preset, replié par défaut.
    pub show_toml: bool,
    /// Fichier du mélange d'IR enregistré (chemin relatif, identifiant) : un preset le cite.
    blend_file: Option<(String, String)>,
    /// Ce qu'on veut mettre à la place de la chaîne en cours dès qu'elle est réglée (fenêtre de
    /// bascule) : un preset, ou une chaîne vierge (`n`).
    pub pending_switch: Option<preset::Pending>,
    /// Fenêtre « la chaîne n'est pas enregistrée » d'une bascule (preset ou chaîne vierge).
    pub confirm_switch: bool,
    /// `S` depuis la fenêtre de `q` : enregistrer **puis** quitter.
    pub quit_after_save: bool,
    /// Fenêtre « Mettre « … » dans la chaîne » (§ 4), quand elle est ouverte.
    pub send: Option<send::Send>,
    /// Mode pose : où poser ce qu'on envoie dans la chaîne (§ 4).
    pub place: Option<send::Place>,
    /// Effets changés : écoute à reconstruire après `AUDITION_DELAY_MS` sans touche.
    fx_due: Option<u64>,
    /// Vumètres par bloc et par branche de la chaîne **en écoute** : la table arrive avec la
    /// chaîne construite, `meters` garde ce que les barres montrent.
    pub chain_meters: Option<dsp::meter::ChainMeters>,
    pub meters: meters::MeterDisplay,
    /// La config aligne-t-elle le volume sur le DI (`level = "match"`) ? Alors un bloc `gain`
    /// posé dans la chaîne est annulé par cet alignement : l'écran doit le dire. La **valeur** de
    /// l'alignement, elle, arrive avec la chaîne installée (`chain_meters`), donc plus tard.
    pub level_match: bool,
    /// `v` : vumètres du synoptique affichés ou masqués.
    pub show_meters: bool,
    /// `z` : panneau Bandes montré ou masqué. `None` : le choix par défaut de l'écran — affiché
    /// quand il ne coûte aucune ligne de liste (écran large), masqué sinon.
    pub bands: Option<bool>,
    /// Largeur du terminal à la dernière image, posée par la boucle d'affichage : elle décide des
    /// panneaux qui ne tiennent que sur un écran large.
    pub width: u16,
    /// Overlay d'aide (`h` ou `?`) : la liste des touches de l'écran où l'on est. N'importe quelle
    /// touche le referme.
    pub help: bool,
    /// Fenêtre « Modifications non enregistrées » ouverte par `q` : seul `Q` quitte.
    pub confirm_quit: bool,
    /// Chaîne d'effets au lancement et dernier mélange enregistré : ce qui est « modifié » se
    /// décide en **comparant des états** (jamais un drapeau posé à la main), pour que revenir à
    /// l'état de départ efface l'alerte.
    start_fx: chain::Chain,
    saved_blend: (Vec<BlendPart>, bool),
    pending: Option<u64>,
    now: u64,
    /// **Le set actif** (§ 4, US 2) : quand il y en a un, `PgDn` / `PgUp` suivent sa liste à plat
    /// au lieu de la banque entière, et la banque préchargée est celle du set. `None` : tout est
    /// exactement comme avant (§ 13).
    pub set: Option<set::ActiveSet>,
    /// Les touches **morceau suivant / précédent** du set, réglables pour coller au pédalier
    /// (`[set] next_song` / `prev_song`, § 9). Elles agiront dans l'espace Set (US 3) ; `PgDn` /
    /// `PgUp`, eux, suivent déjà le set.
    pub song_keys: (Key, Key),
    /// La fenêtre **`L` : choisir le set** (§ 7). `None` : fermée.
    pub set_picker: Option<set::screen::Picker>,
    /// **L'édition du set** (`e`, § 8). `None` : on regarde la set list, on ne la change pas.
    pub set_edit: Option<set::edit::Edit>,
    /// Les **prompteurs** lus, par chemin de fichier : la boucle les lit, l'écran les
    /// affiche. Indexés par chemin et non par numéro de morceau, pour qu'une édition du set
    /// (ajouter, retirer, déplacer un morceau) ne les désaligne pas.
    pub prompts: std::collections::HashMap<std::path::PathBuf, set::prompt::Loaded>,
    /// Les touches **réglables** du prompteur (`[set] prompt_next` / `prompt_prev`, § 10).
    pub prompt_keys: (Key, Key),
    /// La touche **réglable** qui lance et arrête la battue (`[set] prompt_play`, défaut
    /// `Espace`, § 11) : c'est elle que le pédalier envoie, et son appui est le 1.
    pub play_key: Key,
    /// La dernière touche que **personne** n'a possédée (ADR 0009). La boucle l'écrit dans
    /// `NAMFOOP_BROWSE_TRACE` puis la remet à `None` : sur scène, « j'ai appuyé et rien ne s'est
    /// passé » devient diagnosticable, sans rien afficher à l'utilisateur.
    pub unhandled: Option<crate::tui::Key>,
    /// **`run --set` : la scène verrouillée** (§ 9). Seuls les gestes de scène répondent —
    /// changer de son, de morceau, voir un détail, taper le tempo. Tout le reste affiche
    /// `🔒 run --set : rien ne s'édite ici`. On ne modifie pas un preset par mégarde en concert.
    pub locked: bool,
    /// Le **clic du métronome** est-il allumé ? Éteint au démarrage, toujours : c'est une aide
    /// d'écoute, pas un son, et il ne vit donc pas dans un preset (ADR 0008). `k` le bascule.
    pub metronome: bool,
    /// Une sortie reçoit-elle le clic (`click = true` dans le routage) ? Sinon l'allumer ne
    /// s'entendrait nulle part, et il faut le **dire** plutôt que de laisser croire à une panne.
    pub metronome_routed: bool,
    /// Les **numéros de message MIDI** écoutés (ADR 0011), réglables par `[midi]` de la config.
    pub midi: midi::MidiKeys,
    /// Un Program Change reçu qui attend son CC de son (§ 12 du mode Set) : Ableton envoie
    /// souvent les deux en tête de clip, et deux changements de son pour un geste s'entendraient.
    midi_pc: Option<midi::PendingPc>,
    /// **L'arbitre de tempo** (ADR 0008) : il tranche entre le preset, le tap et — plus tard —
    /// l'horloge MIDI. Le tempo **entendu** est le sien ([`State::heard_tempo_bpm`]) ; celui
    /// qui s'**enregistre** est `fx.chain.tempo_bpm`, et seuls un tap ou un chargement de preset
    /// le changent. Les deux diffèrent dès qu'une horloge tourne, et l'en-tête le dit.
    tempo: engine::tempo::Arbiter,
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
            presets: Vec::new(),
            preset: None,
            bank: None,
            naming_preset: false,
            show_toml: false,
            blend_file: None,
            pending_switch: None,
            confirm_switch: false,
            quit_after_save: false,
            send: None,
            place: None,
            fx_due: None,
            chain_meters: None,
            meters: meters::MeterDisplay::default(),
            level_match: false,
            show_meters: true,
            bands: None,
            width: 0,
            help: false,
            confirm_quit: false,
            start_fx: chain::Chain::default(),
            saved_blend: (Vec::new(), true),
            now: 0,
            midi: midi::MidiKeys::default(),
            midi_pc: None,
            tempo: engine::tempo::Arbiter::new(),
            set: None,
            song_keys: (Key::Right, Key::Left),
            set_picker: None,
            set_edit: None,
            prompts: std::collections::HashMap::new(),
            prompt_keys: (Key::End, Key::Home),
            play_key: Key::Char(' '),
            unhandled: None,
            locked: false,
            metronome: false,
            metronome_routed: false,
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

    /// Le tempo **entendu** et d'où il vient : ce que l'en-tête affiche et ce que la case lue par
    /// le temps réel reçoit. Ce n'est pas forcément celui du preset (horloge MIDI, tap non
    /// enregistré).
    pub fn heard_tempo_bpm(&self) -> f32 {
        self.tempo.bpm()
    }

    pub fn tempo_source(&self) -> engine::tempo::TempoSource {
        self.tempo.source()
    }

    /// Le compteur de recalage du **temps 1** : le métronome n'a qu'à le suivre.
    pub fn beat_epoch(&self) -> u64 {
        self.tempo.beat_epoch()
    }

    /// Quel top d'horloge porte le temps (`engine::tempo::Arbiter::clock_phase`) : la boucle le
    /// pose dans la case que lit le clic. C'est le numéro du top d'ancrage, donc une constante
    /// entre deux ancrages — rien à lire au bon moment, rien à verrouiller.
    pub fn clock_phase(&self) -> f32 {
        self.tempo.clock_phase()
    }

    /// La phase de l'horloge est-elle confirmée ? Faux quand on s'est raccroché au milieu d'un
    /// flux : l'en-tête le dit, parce que c'est ce qui indique qu'il faut taper `*` ou relancer la
    /// source.
    pub fn clock_phase_confirmed(&self) -> bool {
        self.tempo.clock_phase_confirmed()
    }

    /// `k` : allume ou éteint le clic. S'il n'a nulle part où aller, on le dit au lieu de laisser
    /// croire à une panne.
    fn toggle_metronome(&mut self) -> Action {
        if !self.metronome_routed {
            self.message = Some(
                "aucune sortie ne reçoit le clic : déclare une sortie « in-ear » dans namfoop setup".into(),
            );
            return Action::None;
        }
        self.metronome = !self.metronome;
        self.message =
            Some(if self.metronome { "clic allumé (dans les in-ears)" } else { "clic éteint" }.into());
        Action::None
    }

    /// L'aide du réglage sous le curseur. Dynamique pour une **division** : la fraction nue se lit
    /// mal (`1/4` est une *noire*, donc **un temps**, et non « un quart de temps »), donc on nomme
    /// la note, on dit ce qu'elle vaut en temps, et on donne la durée au tempo courant.
    pub fn field_help(&self, f: fx::Field) -> String {
        let fx::Field::Param(p) = f else { return f.help().to_string() };
        if p.unit != dsp::fx::Unit::Division {
            return f.help().to_string();
        }
        let v = match self.fx.chain.leaf(self.fx.cursor) {
            Some(chain::Leaf::Fx(s)) => s.spec.get(p.name).unwrap_or(p.default),
            _ => p.default,
        };
        let d = dsp::tempo::at(v);
        let bpm = self.heard_tempo_bpm();
        let Some(ms) = d.ms_at(bpm) else {
            return "« libre » : time agit en millisecondes · choisis une note pour suivre le tempo"
                .to_string();
        };
        let max = dsp::fx::delay::MAX_SECONDS * 1000.0;
        let capped = if ms > max {
            format!(" → écrêté à {max:.0} ms, le tampon du delay")
        } else {
            String::new()
        };
        format!("{} = {}, soit {} ({ms:.0} ms à {bpm:.0} BPM){capped}", d.label, d.name, d.beats_label)
    }

    /// Un **tap** (`*`) : il donne le tempo *et* modifie le preset, comme tout autre réglage
    /// — l'étoile « non enregistré » apparaît, `S` l'enregistre, une bascule de preset
    /// restaure celui du preset suivant.
    fn tap_tempo(&mut self) -> Action {
        self.tap_tempo_at(self.now)
    }

    /// Le tap, à un instant **donné** : une frappe au clavier tombe au tour de boucle, un tap du
    /// pédalier porte l'instant où le message est arrivé — à 30 ms près, un tap à 120 BPM se
    /// tromperait de 7 BPM.
    fn tap_tempo_at(&mut self, now_ms: u64) -> Action {
        // Le banc d'essai n'est pas un preset : il n'a ni volume ni tempo propres, et y taper un
        // **tempo** ferait apparaître « non enregistré » alors qu'on écoute seulement des modèles.
        // Mais un tap fait deux choses, et la seconde — recaler le temps 1 — ne touche à aucun
        // preset : le clic n'est enregistré nulle part et `k` l'allume depuis n'importe quel écran
        // (étage pédale). Le refuser entièrement laissait un clic qu'on ne pouvait plus recaler.
        if self.space == space::Space::Library {
            self.tempo.realign_downbeat(now_ms * 1000);
            if let Some(set) = self.set.as_mut() {
                set.beat.realign(now_ms);
            }
            self.message = Some("temps 1 recalé — le tempo se tape dans l'espace Chaîne (2)".into());
            return Action::None;
        }
        self.tempo.tap(now_ms * 1000);
        // **le tap recale le tempo et le temps, pas la mesure** (§ 11 du mode Set) : on se
        // raccroche au groupe sans perdre le compte des mesures de la section.
        let now = now_ms;
        if let Some(set) = self.set.as_mut() {
            set.beat.realign(now);
        }
        let bpm = self.tempo.bpm();
        if self.tempo.source() == engine::tempo::TempoSource::Tap {
            self.fx.chain.tempo_bpm = bpm;
            self.message = Some(format!("tempo {bpm:.1} BPM (tap)"));
        } else {
            // une seule frappe ne mesure rien : le dire plutôt que de ne rien faire
            self.message = Some(format!("tempo {bpm:.1} BPM — tape encore pour le donner"));
        }
        Action::None
    }

    /// Horloge (ms) : déclenche l'écoute différée et la mesure du grain de l'éditeur.
    pub fn tick(&mut self, now_ms: u64) -> Action {
        self.now = now_ms;
        // une horloge MIDI qui se tait doit se voir : le tempo, lui, est gardé
        self.tempo.poll(now_ms * 1000);
        // **la battue** (§ 11 du mode Set) : la position avance avec l'horloge fournie, et rien
        // n'en descend dans le fil audio — la bande est dessinée par le fil de l'interface
        self.advance_beat(now_ms);
        // la fenêtre PC + CC du mode Set : un Program Change seul finit par jouer son morceau
        if let Some(a) = self.flush_midi_pc(now_ms) {
            return a;
        }
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

    /// Une saisie de texte est en cours : les lettres s'y écrivent, elles ne sont pas des
    /// commandes (filtre d'un onglet ou du sélecteur, nom d'un mélange, texte de l'éditeur,
    /// filtre du catalogue, titre / tempo / PC d'un morceau du set).
    pub fn typing(&self) -> bool {
        self.filtering
            || self.naming.is_some()
            || self.fx.catalog.is_some()
            || self.editor.as_ref().is_some_and(|e| e.typing.is_some())
            || self.set_edit.as_ref().is_some_and(|e| e.typing.is_some())
            || self.picker.as_ref().is_some_and(|p| p.filtering)
            || self.bank.as_ref().is_some_and(|b| b.filtering)
    }
}

pub mod bands;
/// La table des touches : ce que chaque contexte possède, déclaré une fois.
pub mod bindings;
mod blend;
pub mod chain;
mod compare;
/// Le contexte : qui a le clavier, en une valeur.
pub mod context;
mod edit;
mod editor;
pub mod fx;
/// L'oracle du clavier : la table de référence de ce que fait chaque touche.
#[cfg(test)]
mod keyboard;
mod keys;
mod label;
mod lists;
pub mod meters;
/// Ce que namfoop fait d'un message MIDI (ADR 0011).
pub mod midi;
pub mod picker;
pub mod preset;
mod quit;
pub mod send;
pub mod set;
pub mod space;
mod target;

#[cfg(test)]
pub mod tests;
