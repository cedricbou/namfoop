//! **Le set actif** : la set list de concert chargée dans `browse` (§ 4 et § 5 de
//! `docs/design/set.md`).
//!
//! Ici, rien que de la navigation : où l'on en est dans le set, quel son vient au prochain appui,
//! et ce que ce son doit au morceau (son volume, son tempo). L'écran est l'US 3, le MIDI l'US 7.
//!
//! Deux règles qui se voient dans le code :
//! - **la liste à plat** (§ 5) : `PgDn` passe au son suivant, puis au **premier son du morceau
//!   suivant** ; `PgUp` fait l'inverse. **Pas de bouclage** — sur scène, dépasser la fin d'un set
//!   doit ne rien faire et le dire, pas rejouer le premier morceau ;
//! - **un son dont le preset est introuvable est sauté**, jamais joué : le set a été contrôlé à
//!   l'ouverture (`library::set`), le son est marqué, et le concert continue.

use library::set::SetSpec;

pub mod screen;

/// **La battue** : la position dans le temps du morceau, et la bande qui la montre (§ 11).
pub mod beat;

/// Le prompteur à l'écran : la position dans le texte du morceau joué.
pub mod prompt;

/// Où l'on en est dans le set : un morceau et un son.
pub type At = (usize, usize);

/// L'édition du set à l'écran (`e`, § 8).
pub mod edit;

/// Le set chargé, et la place de chacun de ses sons dans la bibliothèque.
#[derive(Clone, Debug)]
pub struct ActiveSet {
    /// Identifiant court du set (`namfoop sets`). Affiché par l'écran de scène (US 3).
    #[allow(dead_code)]
    pub id: String,
    /// Nom affichable (celui du fichier, sans `.set.toml`).
    pub stem: String,
    pub spec: SetSpec,
    /// Pour chaque morceau, pour chaque son : sa place dans `State::presets`. `None` : preset
    /// introuvable, le son est sauté par la navigation.
    slots: Vec<Vec<Option<usize>>>,
    /// Le son joué. `None` : on n'a encore rien joué du set.
    pub at: Option<At>,
    /// Morceau **visé** par `↑` / `↓` dans la vue scène : viser ne change pas le son, c'est
    /// `Entrée` qui joue (§ 6). Rien ne bouge à l'oreille en parcourant la set list.
    pub cursor: usize,
    /// Morceaux dont le **détail** est déplié (`i`) : PC, valeurs de CC 69, volumes, prompteur.
    /// Plusieurs peuvent rester ouverts (§ 6).
    pub expanded: Vec<usize>,
    /// **Où l'on en est dans le texte** du morceau joué (§ 10) : la section, puis la ligne. La
    /// ligne ne bougera d'elle-même qu'avec la battue (US 6b) ; ici, elle repart à zéro à chaque
    /// section.
    pub prompt_section: usize,
    pub prompt_line: usize,
    /// Dans une ligne de grille, **la mesure** où l'on est (0 = la première de la ligne) : celle
    /// que le prompteur allume. `None` : une ligne de texte.
    pub prompt_bar: Option<u32>,
    /// **La battue** (§ 11) : la mesure et le temps dans la section courante, et si elle avance.
    /// La section, elle, est `prompt_section` : le texte et la battue avancent ensemble.
    pub beat: beat::Beat,
    /// Le set **tel qu'il est sur le disque**. C'est la référence du `●` « non enregistré » : la
    /// même idée que `start_fx` pour la chaîne, et c'est ce qui fait que `unsaved()` voit
    /// l'édition sans code en plus.
    pub saved: SetSpec,
}

impl ActiveSet {
    /// Charge un set : `find` donne la place d'un preset dans `State::presets` d'après son
    /// identifiant (ou ses mots-clés), `None` s'il est introuvable.
    pub fn new(id: String, stem: String, spec: SetSpec, find: &dyn Fn(&str) -> Option<usize>) -> Self {
        let slots =
            spec.songs.iter().map(|song| song.sounds.iter().map(|s| find(&s.preset)).collect()).collect();
        Self {
            saved: spec.clone(),
            id,
            stem,
            spec,
            slots,
            at: None,
            cursor: 0,
            expanded: Vec::new(),
            prompt_section: 0,
            prompt_line: 0,
            prompt_bar: None,
            beat: beat::Beat::default(),
        }
    }

    /// **Après une édition**, recalculer la place de chaque son dans la banque : ajouter, retirer
    /// ou déplacer un son change toute la correspondance, et `flat()` s'en sert pour savoir ce qui
    /// est jouable.
    pub fn resync(&mut self, find: impl Fn(&str) -> Option<usize>) {
        self.slots = self
            .spec
            .songs
            .iter()
            .map(|song| song.sounds.iter().map(|s| find(&s.preset)).collect())
            .collect();
    }

    /// Tous les sons **jouables**, dans l'ordre du set : c'est la liste à plat du § 5. Un son dont
    /// le preset manque n'y est pas, un morceau sans son non plus.
    pub fn flat(&self) -> Vec<At> {
        self.slots
            .iter()
            .enumerate()
            .flat_map(|(i, sounds)| {
                sounds.iter().enumerate().filter_map(move |(k, slot)| slot.map(|_| (i, k)))
            })
            .collect()
    }

    /// La place de `at` dans la liste à plat.
    fn flat_index(&self, at: At) -> Option<usize> {
        self.flat().iter().position(|x| *x == at)
    }

    /// Le son suivant (`PgDn`) ou précédent (`PgUp`) dans la liste à plat. `None` : on est au bout
    /// — **pas de bouclage** (§ 5).
    ///
    /// Rien n'a encore été joué : on entre dans le set par le bout qui va dans le sens.
    pub fn step(&self, down: bool) -> Option<At> {
        let flat = self.flat();
        if flat.is_empty() {
            return None;
        }
        let Some(at) = self.at else {
            return Some(if down { flat[0] } else { flat[flat.len() - 1] });
        };
        match self.flat_index(at) {
            Some(i) if down => flat.get(i + 1).copied(),
            Some(i) => i.checked_sub(1).and_then(|k| flat.get(k).copied()),
            // le son joué n'est plus jouable (preset disparu entre-temps) : on repart du bord
            None => Some(if down { flat[0] } else { flat[flat.len() - 1] }),
        }
    }

    /// Le **premier son jouable** du morceau suivant ou précédent (touches réglables, défaut
    /// — branché aux touches par l'US 3, qui crée l'espace Set où elles agissent.
    #[allow(dead_code)]
    /// `→` / `←`). Sert quand un morceau finit sur son deuxième son, et aux pédaliers à trois
    /// boutons. Pas de bouclage non plus.
    pub fn step_song(&self, down: bool) -> Option<At> {
        let here = self.at.map(|(i, _)| i);
        let songs: Vec<usize> = self.flat().iter().map(|(i, _)| *i).collect();
        let first_of = |song: usize| self.flat().into_iter().find(|(i, _)| *i == song);
        match here {
            None => first_of(if down { *songs.first()? } else { *songs.last()? }),
            Some(cur) if down => songs.iter().find(|i| **i > cur).copied().and_then(first_of),
            Some(cur) => songs.iter().rev().find(|i| **i < cur).copied().and_then(first_of),
        }
    }

    /// La place dans `State::presets` du son `at`.
    pub fn slot(&self, at: At) -> Option<usize> {
        *self.slots.get(at.0)?.get(at.1)?
    }

    /// Ce que **ce morceau** ajoute au volume du preset de ce son.
    pub fn volume_db(&self, at: At) -> f32 {
        self.spec.songs.get(at.0).and_then(|s| s.sounds.get(at.1)).map(|s| s.volume_db).unwrap_or(0.0)
    }

    /// Le tempo du morceau, qui **prime** sur celui du preset (§ 4). `None` : garder celui du
    /// preset.
    pub fn tempo_bpm(&self, at: At) -> Option<f32> {
        self.spec.songs.get(at.0).and_then(|s| s.tempo_bpm)
    }

    /// Les presets **distincts** du set, dans l'ordre de première apparition : c'est la banque à
    /// précharger (§ 4). Le volume et le tempo étant des paramètres vivants (ADR 0007, 0008), un
    /// même preset joué dans dix morceaux n'occupe qu'**une** place.
    pub fn distinct_slots(&self) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for slot in self.slots.iter().flatten().flatten() {
            if !out.contains(slot) {
                out.push(*slot);
            }
        }
        out
    }

    /// Combien de **morceaux** du set jouent ce preset (sa place dans `State::presets`). Sert au
    /// titre de la Chaîne quand on y arrive depuis l'espace Set (`2`, § 8) : modifier un son,
    /// c'est modifier son preset, donc **tous** les morceaux qui s'en servent — il faut le savoir
    /// avant d'enregistrer.
    ///
    /// On compte des **places** et non des noms : un son peut désigner son preset par son
    /// identifiant ou par des mots-clés, et c'est `slots` qui a déjà fait la résolution. Et on
    /// compte des morceaux, pas des sons : deux sons du même morceau sur le même preset, c'est le
    /// même endroit du concert.
    pub fn songs_using(&self, slot: usize) -> usize {
        self.slots.iter().filter(|sounds| sounds.contains(&Some(slot))).count()
    }

    /// Ce que l'écran et les messages disent du son `at` : « 2/4 · Le Phare · son 2/2 ».
    pub fn label(&self, at: At) -> String {
        let songs = self.spec.songs.len();
        let sounds = self.spec.songs.get(at.0).map(|s| s.sounds.len()).unwrap_or(0);
        let title = self.spec.songs.get(at.0).map(|s| s.label()).unwrap_or("?");
        if songs == 1 && self.spec.songs[0].title.is_none() {
            // le show à sons : pas de titre à afficher, seulement la place du son
            format!("son {}/{sounds}", at.1 + 1)
        } else {
            format!("{}/{songs} · {title} · son {}/{sounds}", at.0 + 1, at.1 + 1)
        }
    }

    /// Y a-t-il au moins un son jouable ? Sinon le set ne sert à rien et il faut le dire.
    pub fn is_playable(&self) -> bool {
        !self.flat().is_empty()
    }
}

use super::{chain, Action, State};

impl State {
    /// **Son suivant / précédent du set** (`PgDn` / `PgUp`) : la liste à plat, **sans bouclage**
    /// (§ 5). Au bout, on ne fait rien et on le dit — sur scène, dépasser la fin d'un set ne doit
    /// pas rejouer le premier morceau.
    pub(super) fn step_set(&mut self, down: bool) -> Action {
        let Some(s) = self.set.as_ref() else {
            // possédée mais sans objet : on le dit plutôt que de ne rien faire (ADR 0009)
            self.message = Some("aucun set : L pour en choisir un".into());
            return Action::None;
        };
        if !s.is_playable() {
            self.message =
                Some(format!("set « {} » : aucun son jouable (presets introuvables : namfoop sets)", s.stem));
            return Action::None;
        }
        match s.step(down) {
            Some(at) => self.play_set_sound(at),
            None => {
                self.message = Some(if down { "fin du set" } else { "début du set" }.to_string());
                Action::None
            }
        }
    }

    /// **Morceau suivant / précédent** : le premier son du morceau, pour les pédaliers à trois
    /// boutons et les morceaux qui finissent sur leur deuxième son (§ 5).
    ///
    /// Branché aux touches par l'US 3, qui crée l'espace Set où elles agissent.
    #[allow(dead_code)]
    pub fn step_set_song(&mut self, down: bool) -> Action {
        let Some(s) = self.set.as_ref() else { return Action::None };
        match s.step_song(down) {
            Some(at) => self.play_set_sound(at),
            None => {
                self.message = Some(if down { "dernier morceau" } else { "premier morceau" }.to_string());
                Action::None
            }
        }
    }

    /// Joue un son du set : la chaîne de son preset, **plus** ce que le morceau lui ajoute — son
    /// volume et son tempo, deux paramètres vivants, donc posés sans rebâtir la chaîne (§ 4).
    /// Ce que le titre de la Chaîne ajoute quand elle vient d'un son du set (`2` depuis
    /// l'espace Set, § 8). `None` : la chaîne n'est pas celle d'un preset du set.
    pub fn set_preset_note(&self) -> Option<String> {
        let set = self.set.as_ref()?;
        let path = self.preset.as_ref().map(|p| p.path.clone())?;
        let slot = self.presets.iter().position(|p| p.path == path)?;
        let n = set.songs_using(slot);
        (n > 0).then(|| format!("preset utilisé par {n} morceau{} de ce set", if n > 1 { "x" } else { "" }))
    }

    pub fn play_set_sound(&mut self, at: At) -> Action {
        let Some(s) = self.set.as_ref() else { return Action::None };
        let Some(slot) = s.slot(at) else {
            self.message = Some("ce son n'est pas jouable (preset introuvable)".into());
            return Action::None;
        };
        let (volume, tempo, label) = (s.volume_db(at), s.tempo_bpm(at), s.label(at));
        let was_cut = self.bass_cut();
        // **le prompteur suit le son** (§ 10) : avant de bouger `at`, parce que la règle dépend de
        // si l'on change de morceau ou seulement de son
        self.follow_prompt(at);
        let action = self.switch_to(slot);
        // `switch_to` peut demander confirmation : la position ne bouge qu'une fois le son installé
        if let Some(s) = self.set.as_mut() {
            s.at = Some(at);
            // le curseur suit : viser est indépendant de jouer (§ 5), mais après avoir joué on
            // est **là**, et `i` comme `Entrée` doivent partir de là
            s.cursor = at.0;
        }
        self.apply_set_sound(volume, tempo);
        // le son a pu être rendu par le chargement (`apply_pending`) : le message le redit
        let back = was_cut && !self.bass_cut();
        self.message = Some(if back { format!("{label} · son rendu") } else { label });
        action
    }

    /// Ce que le morceau ajoute au preset : son volume s'**ajoute** à celui du preset, son
    /// tempo **prime** sur le sien et entre dans l'arbitre comme le ferait celui d'un preset
    ///, un tap le recouvrant ensuite.
    fn apply_set_sound(&mut self, volume_db: f32, tempo_bpm: Option<f32>) {
        if volume_db != 0.0 {
            let v = self.fx.chain.volume_db + volume_db;
            self.fx.chain.volume_db = v.clamp(chain::VOLUME_MIN_DB, chain::VOLUME_MAX_DB);
        }
        if let Some(bpm) = tempo_bpm {
            self.fx.chain.tempo_bpm = dsp::tempo::clamp_bpm(bpm);
            // comme le tempo d'un preset : l'arbitre le prend, un tap le recouvre ensuite
            self.tempo.preset(self.fx.chain.tempo_bpm);
        }
        // le son du set est la **référence** : il ne doit pas s'afficher « non enregistré » alors
        // qu'on n'a rien touché (le volume du morceau n'appartient pas au preset)
        self.mark_start();
    }

    /// **Le tempo d'une section** (§ 11) : il entre dans l'arbitre comme celui d'un preset — un
    /// tap ou une horloge le recouvrent ensuite. Il ne touche **pas** à `fx.chain.tempo_bpm` : un
    /// morceau qui module n'a pas à rendre son preset « non enregistré ».
    pub(super) fn set_section_tempo(&mut self, bpm: f32) {
        self.tempo.preset(dsp::tempo::clamp_bpm(bpm));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::state::tests::with_chain;
    use crate::tui::Key;
    use library::set::{SetSpec, Song, Sound};

    /// Le set d'exemple du § 3 : deux morceaux, quatre sons, et le second morceau partage son
    /// premier son avec le premier (ce qui fait trois presets **distincts**).
    fn spec() -> SetSpec {
        let sound = |preset: &str, volume_db: f32| Sound { preset: preset.into(), volume_db };
        SetSpec {
            name: "Groupe 1".into(),
            songs: vec![
                Song {
                    title: Some("Le Phare".into()),
                    pc: Some(2),
                    tempo_bpm: Some(124.0),
                    sounds: vec![sound("fuzz", 0.0), sound("clean", -1.5)],
                    ..Default::default()
                },
                Song {
                    title: Some("Marée basse".into()),
                    pc: Some(4),
                    tempo_bpm: Some(82.0),
                    sounds: vec![sound("clean", 0.0), sound("drive", 2.0)],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    /// Les presets de la bibliothèque, par nom : `fuzz` = 0, `clean` = 1, `drive` = 2.
    fn find(q: &str) -> Option<usize> {
        ["fuzz", "clean", "drive"].iter().position(|x| *x == q)
    }

    fn set() -> ActiveSet {
        ActiveSet::new("s1".into(), "Groupe 1".into(), spec(), &find)
    }

    /// **La liste à plat** (§ 5) : les sons dans l'ordre du set, morceau après morceau.
    #[test]
    fn the_flat_list_follows_the_set_order() {
        let s = set();
        assert_eq!(s.flat(), vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert!(s.is_playable());
    }

    /// `PgDn` / `PgUp` parcourent la liste à plat et **ne bouclent pas** : sur scène, dépasser la
    /// fin d'un set ne doit pas rejouer le premier morceau.
    #[test]
    fn stepping_never_wraps_around() {
        let mut s = set();
        // rien joué : on entre par le bout qui va dans le sens
        assert_eq!(s.step(true), Some((0, 0)));
        assert_eq!(s.step(false), Some((1, 1)));

        s.at = Some((0, 0));
        assert_eq!(s.step(true), Some((0, 1)));
        assert_eq!(s.step(false), None, "début du set : rien, pas de bouclage");
        s.at = Some((0, 1));
        assert_eq!(s.step(true), Some((1, 0)), "après le dernier son, le morceau suivant");
        s.at = Some((1, 1));
        assert_eq!(s.step(true), None, "fin du set : rien, pas de bouclage");
        assert_eq!(s.step(false), Some((1, 0)));
    }

    /// **Morceau suivant / précédent** : le **premier** son du morceau, pas le son de même rang.
    /// C'est ce qui sert quand un morceau finit sur son deuxième son.
    #[test]
    fn stepping_songs_goes_to_the_first_sound() {
        let mut s = set();
        s.at = Some((0, 1)); // deuxième son du premier morceau
        assert_eq!(s.step_song(true), Some((1, 0)), "le premier son du morceau suivant");
        s.at = Some((1, 1));
        assert_eq!(s.step_song(false), Some((0, 0)), "et du précédent");
        assert_eq!(s.step_song(true), None, "dernier morceau");
        s.at = Some((0, 0));
        assert_eq!(s.step_song(false), None, "premier morceau");
    }

    /// Un son dont le **preset est introuvable** est sauté, jamais joué : le set a été contrôlé,
    /// le son est marqué, et le concert continue (§ 3).
    #[test]
    fn a_sound_whose_preset_is_missing_is_skipped() {
        let s = ActiveSet::new("s1".into(), "G".into(), spec(), &|q| find(q).filter(|_| q != "clean"));
        // les deux sons « clean » disparaissent de la liste à plat
        assert_eq!(s.flat(), vec![(0, 0), (1, 1)]);
        let mut s = s;
        s.at = Some((0, 0));
        assert_eq!(s.step(true), Some((1, 1)), "le son manquant est sauté");
        assert_eq!(s.slot((0, 1)), None);
        assert!(s.is_playable());

        // plus aucun preset : le set n'est pas jouable, et il faut le dire
        let none = ActiveSet::new("s1".into(), "G".into(), spec(), &|_| None);
        assert!(none.flat().is_empty());
        assert!(!none.is_playable());
        assert_eq!(none.step(true), None);
    }

    /// **Une place par preset distinct** (§ 4) : `clean` sert dans les deux morceaux et n'occupe
    /// qu'une place, parce que le volume et le tempo sont des paramètres vivants.
    #[test]
    fn the_bank_holds_one_slot_per_distinct_preset() {
        assert_eq!(set().distinct_slots(), vec![0, 1, 2], "fuzz, clean, drive");
        // un set qui ne joue qu'un preset, dans dix morceaux : une seule place
        let mono = SetSpec {
            songs: (0..10)
                .map(|_| Song {
                    sounds: vec![Sound { preset: "clean".into(), volume_db: 0.0 }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let s = ActiveSet::new("s".into(), "M".into(), mono, &find);
        assert_eq!(s.distinct_slots(), vec![1], "dix morceaux, une place");
        assert_eq!(s.flat().len(), 10, "mais dix sons à jouer");
    }

    /// Ce que le morceau ajoute au son : son volume s'**ajoute** à celui du preset, son tempo
    /// **prime** sur le sien (§ 4).
    #[test]
    fn a_song_gives_its_volume_and_its_tempo_to_its_sounds() {
        let s = set();
        assert_eq!(s.volume_db((0, 0)), 0.0);
        assert_eq!(s.volume_db((0, 1)), -1.5);
        assert_eq!(s.volume_db((1, 1)), 2.0);
        assert_eq!(s.tempo_bpm((0, 0)), Some(124.0));
        assert_eq!(s.tempo_bpm((1, 0)), Some(82.0));
        // un morceau sans tempo garde celui du preset
        let no_tempo = SetSpec {
            songs: vec![Song {
                sounds: vec![Sound { preset: "fuzz".into(), volume_db: 0.0 }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(ActiveSet::new("s".into(), "N".into(), no_tempo, &find).tempo_bpm((0, 0)), None);
    }

    /// Ce que l'écran et les messages disent : la place dans le set, et seulement le son quand le
    /// set est un **show à sons** (un morceau sans titre, § 2).
    #[test]
    fn the_label_says_where_we_are() {
        assert_eq!(set().label((0, 1)), "1/2 · Le Phare · son 2/2");
        assert_eq!(set().label((1, 0)), "2/2 · Marée basse · son 1/2");
        let show = SetSpec {
            songs: vec![Song {
                sounds: (0..3).map(|_| Sound { preset: "fuzz".into(), volume_db: 0.0 }).collect(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(ActiveSet::new("s".into(), "T".into(), show, &find).label((0, 2)), "son 3/3");
    }

    /// **Sans set, rien ne change** (§ 13) : `PgDn` reste la banque entière, en boucle.
    #[test]
    fn without_a_set_nothing_changes() {
        let mut s = with_chain(vec![]);
        assert!(s.set.is_none());
        // la banque est vide dans ce harnais : le message est celui d'avant le mode Set
        s.on_key(Key::PageDown);
        assert_eq!(s.message.as_deref(), Some("aucun preset enregistré : S enregistre la chaîne"));
    }

    /// **De bout en bout dans l'état** : `PgDn` avec un set actif joue le son suivant, pose le
    /// volume du morceau **par-dessus** celui du preset et son tempo, et s'arrête à la fin du set.
    #[test]
    fn page_down_plays_the_set_and_applies_what_the_song_adds() {
        use dsp::preset::PresetSpec;
        let mut s = with_chain(vec![]);
        // trois presets dans la bibliothèque, dont un à volume non neutre
        let preset = |name: &str, volume_db: f32| {
            let spec = PresetSpec { name: name.into(), volume_db, tempo_bpm: 100.0, ..Default::default() };
            (name.to_string(), name.to_string(), format!("/x/{name}.preset.toml").into(), spec)
        };
        s.adopt_presets(vec![preset("fuzz", 0.0), preset("clean", -3.0), preset("drive", 0.0)]);
        let by_name = |q: &str| s.presets.iter().position(|p| p.name == q);
        s.set = Some(ActiveSet::new("s1".into(), "Groupe 1".into(), spec(), &|q| by_name(q)));
        s.mark_start();
        // **dans l'espace Set** : la Chaîne garde sa banque, les espaces sont indépendants
        s.on_key(Key::Char('3'));

        // son 1 du morceau 1 : volume du preset (0), tempo du morceau (124, qui prime sur 100)
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)));
        assert_eq!(s.fx.chain.volume_db, 0.0);
        assert_eq!(s.heard_tempo_bpm(), 124.0, "le tempo du morceau prime sur celui du preset");
        assert!(s.message.as_deref().is_some_and(|m| m.contains("Le Phare")), "{:?}", s.message);

        // son 2 : volume du preset (-3) PLUS celui du morceau (-1,5)
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 1)));
        assert_eq!(s.fx.chain.volume_db, -4.5, "le morceau s'ajoute au preset, il ne le remplace pas");
        // et rien n'est marqué « non enregistré » : on n'a touché à aucun preset
        assert!(s.unsaved().is_empty(), "{:?}", s.unsaved());

        // morceau 2 : son tempo
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)));
        assert_eq!(s.heard_tempo_bpm(), 82.0);
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 1)));

        // la fin du set : rien ne bouge, et le message le dit (§ 5)
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 1)), "on reste sur le dernier son");
        assert_eq!(s.message.as_deref(), Some("fin du set"));
        // et dans l'autre sens, le début
        for _ in 0..3 {
            s.on_key(Key::PageUp);
        }
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)));
        s.on_key(Key::PageUp);
        assert_eq!(s.message.as_deref(), Some("début du set"));
    }

    /// Les touches réglables de la config se lisent par leur nom (§ 9).
    #[test]
    fn configurable_keys_are_read_by_name() {
        assert_eq!(Key::from_name("Right"), Some(Key::Right));
        assert_eq!(Key::from_name("left"), Some(Key::Left));
        assert_eq!(Key::from_name("Space"), Some(Key::Char(' ')));
        assert_eq!(Key::from_name("PageDown"), Some(Key::PageDown));
        assert_eq!(Key::from_name("k"), Some(Key::Char('k')));
        assert_eq!(Key::from_name("*"), Some(Key::Char('*')));
        assert_eq!(Key::from_name("n'importe quoi"), None);
        assert_eq!(Key::from_name(""), None);
    }
}

#[cfg(test)]
mod independence_tests {
    use super::*;
    use crate::browse::state::space::Space;
    use crate::browse::state::tests::with_chain;
    use crate::tui::Key;
    use dsp::preset::PresetSpec;
    use library::set::{SetSpec, Song, Sound};

    /// Un set de deux morceaux sur deux presets, une banque de trois.
    fn staged() -> State {
        let mut s = with_chain(vec![]);
        let preset = |name: &str| {
            let spec = PresetSpec { name: name.into(), tempo_bpm: 100.0, ..Default::default() };
            (name.to_string(), name.to_string(), format!("/x/{name}.preset.toml").into(), spec)
        };
        s.adopt_presets(vec![preset("alpha"), preset("beta"), preset("gamma")]);
        let names: Vec<String> = s.presets.iter().map(|p| p.name.clone()).collect();
        let find = |q: &str| names.iter().position(|n| n == q);
        let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
        let spec = SetSpec {
            name: "Concert".into(),
            songs: vec![
                Song { title: Some("A".into()), sounds: vec![sound("alpha")], ..Default::default() },
                Song { title: Some("B".into()), sounds: vec![sound("beta")], ..Default::default() },
            ],
            ..Default::default()
        };
        s.set = Some(ActiveSet::new("s".into(), "concert".into(), spec, &find));
        s.mark_start();
        s
    }

    /// **Les espaces sont indépendants**. Choisir un espace, c'est travailler sur ce qu'il
    /// représente : dans la **Chaîne** on travaille son preset, donc `PgDn` parcourt la **banque**
    /// même quand un set est actif. Le set ne se joue que dans **son** espace.
    ///
    /// Détourner `step_preset` vers le set dès qu'il y en a un enlèverait le travail à la maille
    /// preset de l'espace fait pour ça.
    #[test]
    fn the_chain_space_steps_the_bank_even_with_a_set() {
        let mut s = staged();
        s.on_key(Key::Char('2'));
        assert_eq!(s.space, Space::Chain);
        let set_before = s.set.as_ref().unwrap().at;

        // trois presets, en boucle : c'est la banque, pas les deux sons du set
        let mut seen = Vec::new();
        for _ in 0..4 {
            s.on_key(Key::PageDown);
            seen.push(s.preset.as_ref().map(|p| p.name.clone()).unwrap_or_default());
        }
        assert_eq!(seen, ["alpha", "beta", "gamma", "alpha"], "la banque entière, en boucle");
        assert_eq!(s.set.as_ref().unwrap().at, set_before, "le set n'a pas bougé");
    }

    /// Et dans l'autre sens : l'espace **Set** joue le set, et travailler un preset dans la Chaîne
    /// ne déplace pas le morceau joué.
    #[test]
    fn the_set_space_steps_the_set_and_the_two_do_not_interfere() {
        let mut s = staged();
        s.on_key(Key::Char('3'));
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)), "premier son du set");
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)), "son suivant du set");
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)), "pas de bouclage : fin du set");
        assert!(s.message.as_deref().is_some_and(|m| m.contains("fin du set")));

        // on part travailler dans la Chaîne : la banque défile, le set reste où il est
        s.on_key(Key::Char('2'));
        s.on_key(Key::PageDown);
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)), "le set n'a pas bougé");

        // et on revient : le set reprend là où on l'avait laissé
        s.on_key(Key::Char('3'));
        s.on_key(Key::PageUp);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)), "le son précédent du set");
    }

    /// Sans set, l'espace Set le **dit** au lieu de ne rien faire : la touche lui appartient
    /// (ADR 0009), elle n'a juste rien à jouer.
    #[test]
    fn without_a_set_the_page_keys_say_so() {
        let mut s = with_chain(vec![]);
        s.on_key(Key::Char('3'));
        s.on_key(Key::PageDown);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("L pour")), "{:?}", s.message);
    }
}

impl State {
    /// Recalculer les places des presets du set après une édition. La vérité, c'est
    /// `State::presets` : un son désigne son preset par **nom** (ou identifiant), comme dans le
    /// fichier, et c'est là qu'on le retrouve.
    pub(in crate::browse::state) fn resync_set(&mut self) {
        let by: Vec<(String, String)> = self.presets.iter().map(|p| (p.name.clone(), p.id.clone())).collect();
        if let Some(set) = self.set.as_mut() {
            set.resync(|q| by.iter().position(|(n, i)| n == q || i == q));
        }
    }
}
