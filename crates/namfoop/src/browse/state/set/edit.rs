//! **Éditer un set à l'écran** (`e`, § 8 de `docs/design/set.md`, #220).
//!
//! Pourquoi à l'écran et pas dans le TOML : un set se bâtit **en écoutant**. On met un preset dans
//! un morceau, on l'entend, on le remplace. Le fichier reste lisible et éditable à la main, mais ce
//! n'est pas là qu'on travaille.
//!
//! Deux règles qui gouvernent tout le reste :
//!
//! - **viser n'écoute pas.** `↑↓` et `←→` déplacent le cadre jaune sans rien changer à l'oreille.
//!   Seuls `+` et `Entrée` ouvrent une liste où `↑↓` **essaie** un preset — et `Échap` rend le son
//!   qu'on jouait. C'est le patron du sélecteur de la Chaîne (§ 5 de `browse-espaces.md`) ;
//! - **une saisie passe avant toutes les touches** (#175) : tant qu'on tape un titre, un tempo ou
//!   un numéro de PC, `a`, `r`, `x` sont des lettres, pas des commandes.

use super::{ActiveSet, At};
use crate::browse::state::{Action, State};
use crate::tui::Key;
use library::set::{Song, Sound};

#[cfg(test)]
mod tests;

/// Le volume propre d'un son se règle par pas de 0,5 dB (§ 8).
pub const VOLUME_STEP_DB: f32 = 0.5;

/// Ce qu'on **vise** : la ligne d'un morceau, ou l'un de ses sons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    Song(usize),
    Sound(At),
}

impl Aim {
    pub fn song(self) -> usize {
        match self {
            Aim::Song(i) | Aim::Sound((i, _)) => i,
        }
    }
    pub fn sound(self) -> Option<usize> {
        match self {
            Aim::Song(_) => None,
            Aim::Sound((_, k)) => Some(k),
        }
    }
}

/// Le champ qu'on est en train de taper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// Nom du set — il devient le **nom du fichier** (`L` › « ＋ nouveau set… »).
    Name,
    /// Titre du morceau (`a` à la création, `r` ensuite).
    Title,
    /// Tempo du morceau ; **vide = celui des presets** (§ 8).
    Tempo,
    /// Numéro de Program Change.
    Pc,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Name => "nom du set (il nomme aussi son fichier)",
            Field::Title => "titre",
            Field::Tempo => "tempo (vide : celui des presets)",
            Field::Pc => "n° de Program Change",
        }
    }
}

/// Une saisie en cours. `fresh` : la valeur est **pré-remplie**, et la première frappe la remplace
/// — sinon renommer un morceau obligerait à effacer son titre caractère par caractère.
#[derive(Clone, Debug)]
pub struct Typing {
    pub field: Field,
    pub text: String,
    pub fresh: bool,
    /// Le morceau tout juste créé par `a` : annuler la saisie le retire, pour qu'un `a` suivi
    /// d'un Échap ne laisse pas un morceau sans titre.
    pub new_song: bool,
    /// Le set tout juste créé par `L` › « ＋ nouveau set… » : même règle, à l'échelle du set —
    /// un set sans nom n'aurait pas de nom de fichier. Les deux ne sont jamais vrais ensemble.
    pub new_set: bool,
}

impl Typing {
    /// Une saisie ordinaire : rien n'a été créé pour elle.
    fn of(field: Field, text: String, fresh: bool) -> Self {
        Typing { field, text, fresh, new_song: false, new_set: false }
    }
}

/// La liste des presets, ouverte par `+` (ajouter un son) ou `Entrée` (remplacer).
#[derive(Clone, Debug)]
pub struct Picking {
    /// Le son qu'on remplit.
    pub at: At,
    /// `true` : `+` vient de créer ce son, donc Échap le retire.
    pub is_new: bool,
    /// Le preset du son **avant** d'ouvrir la liste : `↑↓` essaient en l'écrivant vraiment (c'est
    /// ce qui les fait entendre), donc Échap a quelque chose à défaire. Le sélecteur de la Chaîne,
    /// lui, n'a pas ce problème : son essai n'est posé que par `plan()`, jamais dans la chaîne.
    pub was: String,
    pub cursor: usize,
    /// Ce qu'on jouait avant d'ouvrir la liste : Échap y revient (§ 8).
    pub played: Option<At>,
}

/// L'édition d'un set.
#[derive(Clone, Debug)]
pub struct Edit {
    pub aim: Aim,
    pub typing: Option<Typing>,
    pub picking: Option<Picking>,
    /// `X` : retirer un morceau demande confirmation (§ 8).
    pub confirm_remove: bool,
}

impl Edit {
    fn new(aim: Aim) -> Self {
        Edit { aim, typing: None, picking: None, confirm_remove: false }
    }
}

impl ActiveSet {
    /// Le **premier numéro de PC libre**, proposé à la création d'un morceau (§ 8). On ne propose
    /// jamais un numéro déjà pris : le refuser après coup serait une surprise.
    pub fn free_pc(&self) -> u8 {
        let taken: Vec<u8> = self.spec.songs.iter().filter_map(|s| s.pc).collect();
        (1..=128).find(|n| !taken.contains(n)).unwrap_or(1)
    }

    /// Le morceau qui occupe déjà ce PC, s'il y en a un (hors `except`).
    pub fn pc_owner(&self, pc: u8, except: usize) -> Option<&Song> {
        self.spec.songs.iter().enumerate().find(|(i, s)| *i != except && s.pc == Some(pc)).map(|(_, s)| s)
    }

    /// `true` si le set a changé depuis son dernier enregistrement.
    pub fn unsaved(&self) -> bool {
        self.spec != self.saved
    }
}

impl State {
    /// **`e`** : entrer dans l'édition, ou en sortir. On vise le morceau joué s'il y en a un, pour
    /// que l'édition commence là où l'oreille est.
    pub(in crate::browse::state) fn toggle_set_edit(&mut self) -> Action {
        if self.set_edit.take().is_some() {
            self.message = Some("édition du set fermée".into());
            return Action::None;
        }
        let Some(set) = self.set.as_ref() else {
            self.message = Some("aucun set à éditer : L pour en choisir un".into());
            return Action::None;
        };
        let aim = match set.at {
            Some(at) => Aim::Sound(at),
            None => Aim::Song(set.cursor),
        };
        self.set_edit = Some(Edit::new(aim));
        self.message = Some("✎ édition du set : a morceau · + son · S enregistrer · Échap sortir".into());
        Action::None
    }

    /// Les **sous-modes** de l'édition : une saisie, la liste des presets, la confirmation de `X`.
    ///
    /// Ils sont appelés à l'étage des **modes** (`keys.rs`), donc **avant la pédale** : pendant une
    /// saisie, `1` est un chiffre du tempo et non un changement d'espace, et `q` une lettre du
    /// titre et non la sortie de `browse` (#175, ADR 0009). Les deux autres sont des fenêtres, et
    /// une fenêtre ne retombe jamais.
    pub(in crate::browse::state) fn set_edit_mode_key(&mut self, key: Key) -> Option<Action> {
        let e = self.set_edit.as_ref()?;
        if e.typing.is_some() {
            return Some(self.set_typing_key(key));
        }
        if e.picking.is_some() {
            return Some(self.set_picking_key(key));
        }
        if e.confirm_remove {
            return Some(self.set_remove_key(key));
        }
        None
    }

    /// La **table** de l'édition (§ 8). `None` : pas à l'édition — l'espace Set reprend la main.
    pub(in crate::browse::state) fn set_edit_key(&mut self, key: Key) -> Option<Action> {
        self.set_edit.as_ref()?;
        Some(match key {
            Key::Esc | Key::Char('e') => self.toggle_set_edit(),
            Key::Up => self.aim_song(-1),
            Key::Down => self.aim_song(1),
            Key::Left => self.aim_sound(-1),
            Key::Right => self.aim_sound(1),
            Key::Char('a') => self.add_song(),
            Key::Char('r') => self.type_field(Field::Title),
            Key::Char('T') => self.type_field(Field::Tempo),
            Key::Char('p') => self.type_field(Field::Pc),
            Key::Char('X') => {
                self.edit_mut().confirm_remove = true;
                self.message = Some("retirer ce morceau ? X confirmer · Échap annuler".into());
                Action::None
            }
            Key::Char('[') | Key::Char(']') => self.move_song(key == Key::Char(']')),
            Key::Char('+') | Key::Char('=') => self.add_sound(),
            Key::Enter => self.replace_sound(),
            Key::Char('x') => self.remove_sound(),
            Key::Char('{') | Key::Char('}') => self.move_sound(key == Key::Char('}')),
            Key::Char(',') => self.adjust_sound_volume(-VOLUME_STEP_DB),
            Key::Char('.') => self.adjust_sound_volume(VOLUME_STEP_DB),
            // `S` ÉCLIPSE la touche commune (ADR 0009) : en édition de set, on enregistre le set
            Key::Char('S') => self.save_set(),
            // la touche du pied ne se bloque **jamais** (la leçon de #249) : on édite en écoutant,
            // donc on doit pouvoir changer de son sans sortir de l'édition
            Key::PageDown | Key::PageUp => self.step_set(key == Key::PageDown),
            // **l'édition ne laisse rien tomber.** Tout le reste lui appartient et reste inerte :
            // sans ça, les touches de la vue scène agissaient derrière un écran qui ne les montre
            // pas — `L` changeait de set en abandonnant les modifications sans rien demander,
            // `i` dépliait un détail invisible, `P` ouvrait la banque (même famille que #249).
            _ => {
                // celles qui ont un sens ailleurs dans cet espace le disent, plutôt que de ne
                // rien faire du tout (ADR 0009 : une touche possédée mais inerte dit pourquoi)
                if matches!(key, Key::Char('i') | Key::Char('L') | Key::Char('n') | Key::Char('P')) {
                    self.message = Some("en édition du set : Échap ou e pour en sortir".into());
                }
                Action::None
            }
        })
    }

    fn edit_mut(&mut self) -> &mut Edit {
        self.set_edit.as_mut().expect("en édition")
    }

    /// `↑` / `↓` : viser un morceau. **Rien ne change à l'oreille** (§ 8).
    fn aim_song(&mut self, by: isize) -> Action {
        let n = self.set.as_ref().map(|s| s.spec.songs.len()).unwrap_or(0);
        if n == 0 {
            self.message = Some("set vide : a ajoute un morceau".into());
            return Action::None;
        }
        let i = self.edit_mut().aim.song();
        let next = (i as isize + by).clamp(0, n as isize - 1) as usize;
        self.edit_mut().aim = Aim::Song(next);
        Action::None
    }

    /// `←` / `→` : viser un son du morceau. Depuis la ligne du morceau, `→` entre dans ses sons.
    fn aim_sound(&mut self, by: isize) -> Action {
        let song = self.edit_mut().aim.song();
        let n = self.set.as_ref().and_then(|s| s.spec.songs.get(song)).map(|s| s.sounds.len()).unwrap_or(0);
        if n == 0 {
            self.message = Some("ce morceau n'a aucun son : + en ajoute un".into());
            return Action::None;
        }
        let aim = match (self.edit_mut().aim.sound(), by) {
            (None, 1) => Aim::Sound((song, 0)),
            (None, _) => Aim::Song(song),
            (Some(0), -1) => Aim::Song(song),
            (Some(k), _) => Aim::Sound((song, (k as isize + by).clamp(0, n as isize - 1) as usize)),
        };
        self.edit_mut().aim = aim;
        Action::None
    }
}

impl State {
    /// **`L` › « ＋ nouveau set… »** : un set vide, nommé tout de suite, et l'édition ouverte
    /// dessus — c'est la fin du geste du § 7.
    ///
    /// Rien n'est écrit sur le disque avant `S` : le set n'existe que dans `browse`, et son `●`
    /// le dit. C'est aussi pour ça qu'on ne le retient pas encore dans `[set] default` (la boucle
    /// le fera à l'enregistrement, quand le fichier existera).
    pub(in crate::browse) fn new_set(&mut self) -> Action {
        self.set =
            Some(ActiveSet::new(String::new(), String::new(), library::set::SetSpec::default(), &|_| None));
        self.set_edit = Some(Edit::new(Aim::Song(0)));
        self.edit_mut().typing =
            Some(Typing { new_set: true, ..Typing::of(Field::Name, String::new(), false) });
        self.message = Some("nouveau set : tape son nom, puis Entrée".into());
        // la banque préchargée était celle de l'ancien set (ou la bibliothèque entière) : à refaire
        Action::SetChanged
    }

    /// Ouvrir une saisie sur le morceau visé, **pré-remplie** par sa valeur actuelle.
    fn type_field(&mut self, field: Field) -> Action {
        // le nom du set n'appartient à aucun morceau : il se tape même sur un set encore vide
        if field == Field::Name {
            let text = self.set.as_ref().map(|x| x.spec.name.clone()).unwrap_or_default();
            self.edit_mut().typing = Some(Typing::of(field, text, true));
            self.message = Some(format!("{} : tape puis Entrée (Échap annule)", field.label()));
            return Action::None;
        }
        let song = self.edit_mut().aim.song();
        let Some(s) = self.set.as_ref().and_then(|x| x.spec.songs.get(song)) else {
            self.message = Some("set vide : a ajoute un morceau".into());
            return Action::None;
        };
        let text = match field {
            // traité plus haut : le nom du set n'est pas un champ du morceau
            Field::Name => String::new(),
            Field::Title => s.title.clone().unwrap_or_default(),
            Field::Tempo => s.tempo_bpm.map(|t| format!("{t:.0}")).unwrap_or_default(),
            Field::Pc => s.pc.map(|p| p.to_string()).unwrap_or_default(),
        };
        self.edit_mut().typing = Some(Typing::of(field, text, true));
        self.message = Some(format!("{} : tape puis Entrée (Échap annule)", field.label()));
        Action::None
    }

    /// `a` : un morceau de plus **après** celui qu'on vise, avec le premier PC libre, et on tape
    /// son titre tout de suite — un morceau sans nom ne se reconnaît pas dans une set list.
    fn add_song(&mut self) -> Action {
        let at = self.edit_mut().aim.song();
        let (pc, where_) = {
            let Some(set) = self.set.as_mut() else { return Action::None };
            let pc = set.free_pc();
            let where_ = if set.spec.songs.is_empty() { 0 } else { at + 1 };
            set.spec.songs.insert(where_, Song { pc: Some(pc), ..Default::default() });
            (pc, where_)
        };
        self.resync_set();
        let e = self.edit_mut();
        e.aim = Aim::Song(where_);
        e.typing = Some(Typing { new_song: true, ..Typing::of(Field::Title, String::new(), false) });
        self.message = Some(format!("morceau {} (PC {pc}) : tape son titre", where_ + 1));
        Action::None
    }

    /// Les touches d'une saisie. Tout y est une lettre (#175), sauf Entrée, Échap et Retour.
    fn set_typing_key(&mut self, key: Key) -> Action {
        let Some(t) = self.set_edit.as_mut().and_then(|e| e.typing.as_mut()) else {
            return Action::None;
        };
        match key {
            Key::Char(c) => {
                if t.fresh {
                    t.text.clear();
                    t.fresh = false;
                }
                t.text.push(c);
                Action::None
            }
            Key::Backspace => {
                t.fresh = false;
                t.text.pop();
                Action::None
            }
            Key::Esc => {
                let (was_new, was_new_set) = (t.new_song, t.new_set);
                let song = self.edit_mut().aim.song();
                self.edit_mut().typing = None;
                if was_new_set {
                    // « nouveau set » puis Échap : on ne laisse pas un set sans nom, donc sans
                    // fichier. On revient là où on était, sans set actif.
                    self.set = None;
                    self.set_edit = None;
                    self.message = Some("nouveau set abandonné".into());
                    return Action::SetChanged;
                }
                if was_new {
                    // `a` puis Échap : on ne laisse pas un morceau sans titre derrière soi
                    if let Some(set) = self.set.as_mut() {
                        set.spec.songs.remove(song);
                    }
                    self.resync_set();
                    self.edit_mut().aim = Aim::Song(song.saturating_sub(1));
                    self.message = Some("morceau abandonné".into());
                } else {
                    self.message = Some("saisie annulée".into());
                }
                Action::None
            }
            Key::Enter => self.commit_typing(),
            _ => Action::None,
        }
    }

    /// Valider une saisie : c'est ici que les valeurs sont **contrôlées**, et refusées avec la
    /// raison — un message qui dit « non » sans dire pourquoi fait perdre plus de temps que l'erreur.
    fn commit_typing(&mut self) -> Action {
        let Some(t) = self.set_edit.as_ref().and_then(|e| e.typing.clone()) else {
            return Action::None;
        };
        let song = self.edit_mut().aim.song();
        let text = t.text.trim().to_string();
        match t.field {
            Field::Name => {
                if text.is_empty() {
                    self.message = Some("un set a besoin d'un nom : c'est aussi celui du fichier".into());
                    return Action::None;
                }
                if let Some(set) = self.set.as_mut() {
                    set.spec.name = text.clone();
                    // le nom **est** le nom du fichier, comme pour un preset (`S` dans la Chaîne) :
                    // `library::Library::set_path` n'en remplace que les séparateurs de chemin
                    set.stem = text.clone();
                }
                self.message = Some(format!("set « {text} » : a ajoute son premier morceau"));
            }
            Field::Title => {
                if text.is_empty() && t.new_song {
                    self.message = Some("un morceau a besoin d'un titre".into());
                    return Action::None;
                }
                if let Some(set) = self.set.as_mut() {
                    if let Some(s) = set.spec.songs.get_mut(song) {
                        s.title = (!text.is_empty()).then_some(text.clone());
                    }
                }
                self.message = Some(if text.is_empty() {
                    "titre retiré".into()
                } else {
                    format!("morceau « {text} »")
                });
            }
            Field::Tempo => {
                // vide = pas de tempo propre : le morceau suit celui de ses presets (§ 8)
                let bpm = if text.is_empty() {
                    None
                } else {
                    match text.parse::<f32>() {
                        Ok(v) if (dsp::tempo::MIN_BPM..=dsp::tempo::MAX_BPM).contains(&v) => Some(v),
                        _ => {
                            self.message = Some(format!(
                                "tempo « {text} » refusé : un nombre entre {:.0} et {:.0}, ou vide pour suivre les presets",
                                dsp::tempo::MIN_BPM,
                                dsp::tempo::MAX_BPM
                            ));
                            return Action::None;
                        }
                    }
                };
                if let Some(s) = self.set.as_mut().and_then(|x| x.spec.songs.get_mut(song)) {
                    s.tempo_bpm = bpm;
                }
                self.message = Some(match bpm {
                    Some(v) => format!("tempo du morceau : {v:.0} BPM"),
                    None => "tempo du morceau : celui de ses presets".into(),
                });
            }
            Field::Pc => {
                let pc = match text.parse::<u8>() {
                    Ok(v) if (1..=128).contains(&v) => v,
                    _ => {
                        self.message = Some(format!("PC « {text} » refusé : un numéro entre 1 et 128"));
                        return Action::None;
                    }
                };
                // un PC en double rendrait le set injouable en MIDI : on refuse **en nommant**
                // le morceau qui l'a déjà, sinon il faut le chercher à la main
                if let Some(owner) = self.set.as_ref().and_then(|s| s.pc_owner(pc, song)) {
                    self.message =
                        Some(format!("PC {pc} déjà pris par « {} » : choisis-en un autre", owner.label()));
                    return Action::None;
                }
                if let Some(s) = self.set.as_mut().and_then(|x| x.spec.songs.get_mut(song)) {
                    s.pc = Some(pc);
                }
                self.message = Some(format!("PC {pc}"));
            }
        }
        self.edit_mut().typing = None;
        Action::None
    }

    /// `X` confirmé : retirer le morceau visé.
    fn set_remove_key(&mut self, key: Key) -> Action {
        self.edit_mut().confirm_remove = false;
        if key != Key::Char('X') {
            self.message = Some("morceau gardé".into());
            return Action::None;
        }
        let song = self.edit_mut().aim.song();
        let (gone, n) = {
            let Some(set) = self.set.as_mut() else { return Action::None };
            if song >= set.spec.songs.len() {
                return Action::None;
            }
            let gone = set.spec.songs.remove(song).label().to_string();
            let n = set.spec.songs.len();
            // le son joué peut avoir disparu : on ne laisse pas une position qui ne veut plus
            // rien dire
            if set.at.is_some_and(|(i, _)| i >= n || i == song) {
                set.at = None;
            }
            (gone, n)
        };
        self.resync_set();
        self.edit_mut().aim = Aim::Song(song.min(n.saturating_sub(1)));
        self.message = Some(format!("morceau « {gone} » retiré"));
        Action::None
    }

    /// `[` / `]` : déplacer le morceau dans le set.
    fn move_song(&mut self, down: bool) -> Action {
        let song = self.edit_mut().aim.song();
        let to = if down { song + 1 } else { song.wrapping_sub(1) };
        {
            let Some(set) = self.set.as_mut() else { return Action::None };
            if to >= set.spec.songs.len() {
                self.message = Some(if down { "déjà le dernier morceau" } else { "déjà le premier" }.into());
                return Action::None;
            }
            set.spec.songs.swap(song, to);
            // le son joué suit son morceau : sinon on écouterait soudain un autre morceau
            if let Some((i, k)) = set.at {
                if i == song {
                    set.at = Some((to, k));
                } else if i == to {
                    set.at = Some((song, k));
                }
            }
        }
        self.resync_set();
        self.edit_mut().aim = Aim::Song(to);
        self.message = Some(format!("morceau déplacé en {}", to + 1));
        Action::None
    }
}

impl State {
    /// `+` : un son de plus **après** celui qu'on vise, et la liste des presets s'ouvre dessus.
    /// Un son sans preset n'aurait aucun sens, donc Échap le retire (comme un bloc tout juste posé
    /// par le catalogue, #170).
    fn add_sound(&mut self) -> Action {
        let aim = self.edit_mut().aim;
        let song = aim.song();
        if self.presets.is_empty() {
            self.message = Some("aucun preset enregistré : S dans la Chaîne en enregistre un".into());
            return Action::None;
        }
        let k = aim.sound().map(|k| k + 1).unwrap_or(0);
        let played = {
            let Some(set) = self.set.as_mut() else { return Action::None };
            let Some(s) = set.spec.songs.get_mut(song) else {
                self.message = Some("set vide : a ajoute un morceau".into());
                return Action::None;
            };
            s.sounds.insert(k, Sound { preset: String::new(), volume_db: 0.0 });
            set.at
        };
        self.resync_set();
        let e = self.edit_mut();
        e.aim = Aim::Sound((song, k));
        e.picking = Some(Picking { at: (song, k), is_new: true, was: String::new(), cursor: 0, played });
        self.try_picked()
    }

    /// `Entrée` sur un son : la même liste, pour **remplacer** son preset.
    fn replace_sound(&mut self) -> Action {
        let Some(k) = self.edit_mut().aim.sound() else {
            self.message = Some("→ pour viser un son, puis Entrée pour changer son preset".into());
            return Action::None;
        };
        let song = self.edit_mut().aim.song();
        if self.presets.is_empty() {
            self.message = Some("aucun preset enregistré : S dans la Chaîne en enregistre un".into());
            return Action::None;
        }
        // on ouvre la liste **sur le preset actuel** : c'est de là qu'on compare
        let was = self
            .set
            .as_ref()
            .and_then(|s| s.spec.songs.get(song))
            .and_then(|s| s.sounds.get(k))
            .map(|s| s.preset.clone())
            .unwrap_or_default();
        let cursor = self.presets.iter().position(|p| p.name == was || p.id == was).unwrap_or(0);
        let played = self.set.as_ref().and_then(|s| s.at);
        self.edit_mut().picking = Some(Picking { at: (song, k), is_new: false, was, cursor, played });
        self.try_picked()
    }

    /// Les touches de la liste des presets. Elle prend **tout** : une modale ne retombe jamais
    /// (ADR 0009).
    fn set_picking_key(&mut self, key: Key) -> Action {
        let n = self.presets.len();
        match key {
            Key::Up | Key::PageUp => {
                if let Some(p) = self.set_edit.as_mut().and_then(|e| e.picking.as_mut()) {
                    p.cursor = p.cursor.saturating_sub(1);
                }
                self.try_picked()
            }
            Key::Down | Key::PageDown => {
                if let Some(p) = self.set_edit.as_mut().and_then(|e| e.picking.as_mut()) {
                    p.cursor = (p.cursor + 1).min(n.saturating_sub(1));
                }
                self.try_picked()
            }
            Key::Enter => {
                let Some(p) = self.set_edit.as_mut().and_then(|e| e.picking.take()) else {
                    return Action::None;
                };
                let name = self.presets.get(p.cursor).map(|x| x.name.clone()).unwrap_or_default();
                self.message = Some(format!("son {} : « {name} »", p.at.1 + 1));
                Action::None
            }
            Key::Esc => {
                let Some(p) = self.set_edit.as_mut().and_then(|e| e.picking.take()) else {
                    return Action::None;
                };
                if p.is_new {
                    // rien n'est ajouté : le son n'existait que pour être rempli
                    if let Some(s) = self.set.as_mut().and_then(|x| x.spec.songs.get_mut(p.at.0)) {
                        if p.at.1 < s.sounds.len() {
                            s.sounds.remove(p.at.1);
                        }
                    }
                    self.resync_set();
                    self.edit_mut().aim = Aim::Song(p.at.0);
                    self.message = Some("aucun son ajouté".into());
                } else {
                    // les `↑↓` ont écrit leurs essais dans le son : on remet celui d'avant,
                    // sinon « preset inchangé » serait un mensonge (et le set resterait modifié)
                    if let Some(sound) = self
                        .set
                        .as_mut()
                        .and_then(|x| x.spec.songs.get_mut(p.at.0))
                        .and_then(|x| x.sounds.get_mut(p.at.1))
                    {
                        sound.preset = p.was.clone();
                    }
                    self.resync_set();
                    self.message = Some("preset inchangé".into());
                }
                // on rend ce qu'on jouait (§ 8)
                match p.played {
                    Some(at) => self.play_set_sound(at),
                    None => Action::None,
                }
            }
            _ => Action::None,
        }
    }

    /// Écrire le preset visé dans le son, et le **jouer** : c'est ce qui fait qu'on choisit à
    /// l'oreille et non sur un nom.
    fn try_picked(&mut self) -> Action {
        let Some(p) = self.set_edit.as_ref().and_then(|e| e.picking.clone()) else {
            return Action::None;
        };
        let Some(name) = self.presets.get(p.cursor).map(|x| x.name.clone()) else {
            return Action::None;
        };
        if let Some(s) = self.set.as_mut().and_then(|x| x.spec.songs.get_mut(p.at.0)) {
            if let Some(sound) = s.sounds.get_mut(p.at.1) {
                sound.preset = name.clone();
            }
        }
        self.resync_set();
        self.play_set_sound(p.at)
    }

    /// `x` : retirer le son visé. Sans confirmation — le preset reste dans la banque, on ne perd
    /// rien (§ 8).
    fn remove_sound(&mut self) -> Action {
        let Some(k) = self.edit_mut().aim.sound() else {
            self.message = Some("→ pour viser un son, puis x pour le retirer (X retire le morceau)".into());
            return Action::None;
        };
        let song = self.edit_mut().aim.song();
        let (gone, left) = {
            let Some(set) = self.set.as_mut() else { return Action::None };
            let Some(s) = set.spec.songs.get_mut(song) else { return Action::None };
            if k >= s.sounds.len() {
                return Action::None;
            }
            let gone = s.sounds.remove(k).preset;
            let left = s.sounds.len();
            if set.at == Some((song, k)) {
                set.at = None;
            }
            (gone, left)
        };
        self.resync_set();
        self.edit_mut().aim = if left == 0 { Aim::Song(song) } else { Aim::Sound((song, k.min(left - 1))) };
        self.message = Some(format!("son « {gone} » retiré"));
        Action::None
    }

    /// `{` / `}` : déplacer le son dans son morceau. **Sa valeur de CC 69 change** : c'est sa
    /// place qui le désigne en MIDI (§ 5).
    fn move_sound(&mut self, down: bool) -> Action {
        let Some(k) = self.edit_mut().aim.sound() else {
            self.message = Some("→ pour viser un son, puis { } pour le déplacer".into());
            return Action::None;
        };
        let song = self.edit_mut().aim.song();
        let to = if down { k + 1 } else { k.wrapping_sub(1) };
        {
            let Some(set) = self.set.as_mut() else { return Action::None };
            let Some(s) = set.spec.songs.get_mut(song) else { return Action::None };
            if to >= s.sounds.len() {
                self.message = Some(if down { "déjà le dernier son" } else { "déjà le premier" }.into());
                return Action::None;
            }
            s.sounds.swap(k, to);
            if let Some((i, j)) = set.at {
                if i == song && j == k {
                    set.at = Some((i, to));
                } else if i == song && j == to {
                    set.at = Some((i, k));
                }
            }
        }
        self.resync_set();
        self.edit_mut().aim = Aim::Sound((song, to));
        self.message = Some(format!("son déplacé en {} (CC 69 = {to})", to + 1));
        Action::None
    }

    /// `,` / `.` : le volume **propre au morceau**, qui s'ajoute à celui du preset (§ 4).
    fn adjust_sound_volume(&mut self, by: f32) -> Action {
        let Some(k) = self.edit_mut().aim.sound() else {
            self.message = Some("→ pour viser un son, puis , et . pour son volume".into());
            return Action::None;
        };
        let song = self.edit_mut().aim.song();
        let Some(s) = self.set.as_mut().and_then(|x| x.spec.songs.get_mut(song)) else {
            return Action::None;
        };
        let Some(sound) = s.sounds.get_mut(k) else { return Action::None };
        sound.volume_db = (sound.volume_db + by).clamp(-24.0, 12.0);
        let db = sound.volume_db;
        // si c'est le son joué, on l'entend tout de suite : c'est un paramètre vivant (ADR 0007)
        let played = self.set.as_ref().and_then(|x| x.at) == Some((song, k));
        let action = if played { self.play_set_sound((song, k)) } else { Action::None };
        // **après** avoir rejoué : `play_set_sound` écrit sa propre ligne (« 1/2 · A · son 1/2 »),
        // et c'est la valeur du volume qu'on veut lire quand on vient de la changer
        self.message = Some(format!("volume du son : {db:+.1} dB"));
        action
    }

    /// **`S` : enregistrer le set.** L'écriture est à la boucle (l'état est pur), comme pour un
    /// preset (#110).
    fn save_set(&mut self) -> Action {
        let Some(set) = self.set.as_ref() else { return Action::None };
        if set.stem.trim().is_empty() {
            self.message = Some("ce set n'a pas de nom : impossible de le nommer en fichier".into());
            return Action::None;
        }
        if set.spec.songs.is_empty() {
            self.message = Some("set vide : rien à enregistrer".into());
            return Action::None;
        }
        if let Some(bad) = set.spec.songs.iter().position(|s| s.sounds.is_empty()) {
            self.message = Some(format!(
                "morceau {} (« {} ») n'a aucun son : + en ajoute un",
                bad + 1,
                set.spec.songs[bad].label()
            ));
            return Action::None;
        }
        Action::SaveSet(set.stem.clone())
    }
}
