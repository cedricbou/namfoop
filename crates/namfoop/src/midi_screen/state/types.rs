//! Les types de l'écran MIDI : les lignes qu'on vise, les fenêtres, ce que la boucle doit faire.
//! L'état lui-même (`State`) est dans `state.rs`.

use super::*;

/// Une ligne qu'on peut viser.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    Device(String),
    /// « Tous les canaux » (aucun canal réglé), ou un canal réglé.
    Channel(Option<u8>),
    /// Le gabarit d'un bloc : déduit de ses numéros, choisi dans une liste.
    Template(Option<u8>),
    Gesture(Option<u8>, Gesture),
}

/// Le ton d'un texte : il devient une couleur dans `ui.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Dim,
    Ok,
    Warn,
    Error,
    /// « basse coupée » : le violet de la charte, et lui seul.
    Cut,
}

/// Un appareil tel que le port le voit, rafraîchi à chaque tour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceLive {
    pub name: String,
    pub present: bool,
    /// Tops écartés par le port (un autre appareil donne l'horloge).
    pub ignored_clock: u64,
}

/// Ce qui a été reçu en dernier pour une ligne.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Last {
    pub at_ms: u64,
    pub text: String,
    pub device: String,
}

/// Une ligne de la zone « reçu ».
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub device: String,
    pub channel: Option<u8>,
    pub message: String,
    pub effect: String,
    pub tone: Tone,
}

/// Ce que règle la fenêtre « Régler ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Ajouter un canal (`a`, ou `Entrée` sur « Tous les canaux »).
    AddChannel,
    /// Changer le numéro d'un canal réglé.
    ChangeChannel(u8),
    /// Le numéro de CC d'un geste, sur un canal (`None` : tous les canaux).
    Number(Option<u8>, Gesture),
    /// Le numéro de Program Change d'un morceau du set (onglet Plan du set).
    SongPc(usize),
}

/// Les onglets de l'écran, dans l'ordre (§ 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Devices,
    Plan,
    Fiche,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Devices, Tab::Plan, Tab::Fiche];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Devices => "Appareils et canaux",
            Tab::Plan => "Plan du set",
            Tab::Fiche => "Fiche",
        }
    }
}

/// Une fenêtre de liste (§ 5, « Les listes ») : pas une saisie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListWin {
    pub kind: ListKind,
    pub items: Vec<String>,
    pub idx: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListKind {
    /// Un gabarit pour le bloc (`None` : tous les canaux).
    Template(Option<u8>),
    /// Le canal dont le Plan du set montre les CC (`C`).
    Channel(Vec<u8>),
    /// Un autre set (`L`) : leurs chemins, dans l'ordre des noms.
    Set(Vec<std::path::PathBuf>),
}

/// La fenêtre « Régler » (§ 5) : une saisie, donc un mode (ADR 0009).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputWin {
    pub target: Target,
    pub buf: String,
    /// Ce qu'un appui a apporté, et son ton.
    pub learned: Option<(String, Tone)>,
    /// Le refus en cours, s'il y en a un.
    pub err: Option<String>,
    /// Un appui reçu sur un canal non réglé : `Entrée` ajoute ce canal avec ce numéro.
    pub add_channel: Option<u8>,
    /// Vrai tant que la saisie montre la valeur d'avant : le premier chiffre tapé la remplace.
    pub fresh: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modal {
    Input(InputWin),
    /// « Modifications non enregistrées » (§ 9).
    Quit,
    Help,
    List(ListWin),
}

/// Ce que la boucle doit faire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// Écrire la config, puis [`State::mark_saved`].
    Save,
    /// Écrire le set, puis [`State::mark_set_saved`].
    SaveSet,
    /// Ouvrir un autre set : la boucle le lit, puis [`State::open_set`].
    OpenSet(std::path::PathBuf),
    /// Écrire la fiche dans `fiche-midi.txt`, dans le dossier courant.
    WriteFiche,
    /// Écrire la config et le set (`S` sur la Fiche).
    SaveBoth,
    Quit,
    /// Écrire ce qui n'est pas enregistré (config et set), puis quitter.
    SaveAndQuit,
}

/// Un port imposé : ce qui a été demandé, et par qui.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pinned {
    /// La valeur demandée, telle qu'écrite (« 32:0 », « MC6 »).
    pub asked: String,
    /// Vrai : par `--midi-in`, pour cette fois ; faux : par `in` dans `[midi]` de la config.
    pub by_option: bool,
}
