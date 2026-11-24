//! **Le prompteur à l'écran** (§ 10 de `docs/design/set.md`) : où l'on en est dans le texte
//! du morceau joué, et comment on s'y déplace.
//!
//! La lecture du ChordPro est dans `library::prompt` (pure, testée). Ici, rien que la **position**
//! et ce qui la déplace :
//!
//! - **le son.** À l'arrêt, changer de son amène à la prochaine section liée à ce son
//!   (`{x_namfoop_sound}`). **En marche, la position suit le temps** et un changement de son ne
//!   déplace rien : un son joué un temps trop tôt décalerait tout le reste. Ce qui fait avancer la
//!   position au tempo est la battue, dans [`super::beat`] ;
//! - **`Fin` / `Début`** (réglables, `[set] prompt_next` / `prompt_prev`) : la section suivante ou
//!   précédente, **sans toucher au son**, et recalées sur son 1. C'est le geste du pied quand le
//!   texte et le son ne suivent pas le même découpage.
//!
//! Les prompteurs eux-mêmes sont tenus par `State::prompts`, **indexés par leur chemin** et non par
//! le numéro du morceau : l'édition d'un set ajoute, retire et déplace des morceaux, et une
//! liste parallèle se serait désalignée au premier `[`.

use std::path::PathBuf;

use library::prompt::Prompt;

use super::At;
use crate::browse::state::{Action, State};

/// Le prompteur d'un morceau, tel que `browse` le tient. Un fichier absent ou illisible **n'est
/// pas une erreur** : le cadre le dit et le concert continue (§ 10).
#[derive(Clone, Debug)]
pub enum Loaded {
    Ok(Prompt),
    Failed(String),
}

impl Loaded {
    pub fn prompt(&self) -> Option<&Prompt> {
        match self {
            Loaded::Ok(p) => Some(p),
            Loaded::Failed(_) => None,
        }
    }
}

impl State {
    /// Les prompteurs lus par la boucle (elle seule voit la bibliothèque), à l'ouverture d'un set
    /// et au retour de `$EDITOR`.
    pub fn adopt_prompts(&mut self, found: Vec<(PathBuf, Loaded)>) {
        for (path, loaded) in found {
            self.prompts.insert(path, loaded);
        }
    }

    /// Le prompteur d'un morceau, s'il en a un et s'il a pu être lu.
    pub fn prompt_of(&self, song: usize) -> Option<&Loaded> {
        let set = self.set.as_ref()?;
        let rel = set.spec.songs.get(song)?.prompt.as_deref()?;
        self.prompts.get(&set.spec.base.join(rel))
    }

    /// `Fin` / `Début` : la section suivante / précédente du morceau joué. **Rien ne change à
    /// l'oreille** — le texte et le son n'ont pas forcément le même découpage.
    pub(in crate::browse::state) fn step_prompt(&mut self, next: bool) -> Action {
        let Some(set) = self.set.as_ref() else {
            self.message = Some("aucun set : L pour en choisir un".into());
            return Action::None;
        };
        let Some(song) = set.at.map(|(i, _)| i).or(Some(set.cursor)) else { return Action::None };
        let n = match self.prompt_of(song).and_then(|l| l.prompt()) {
            Some(p) => p.sections.len(),
            None => {
                // sur scène, `t` n'existe pas : ne pas annoncer une touche qui sera refusée
                self.message = Some(if self.locked {
                    "pas de prompteur pour ce morceau".into()
                } else {
                    "pas de prompteur pour ce morceau : t l'écrit".to_string()
                });
                return Action::None;
            }
        };
        if n == 0 {
            self.message = Some("prompteur vide : t pour l'écrire".into());
            return Action::None;
        }
        let here = self.set.as_ref().map(|s| s.prompt_section).unwrap_or(0);
        let to = if next { here + 1 } else { here.wrapping_sub(1) };
        if to >= n {
            self.message = Some(if next { "dernière section" } else { "première section" }.into());
            return Action::None;
        }
        let label = self
            .prompt_of(song)
            .and_then(|l| l.prompt())
            .and_then(|p| p.sections.get(to))
            .map(|s| s.label.clone())
            .unwrap_or_default();
        let now = self.now;
        let ticks = self.clock_elapsed_ticks().unwrap_or(0);
        if let Some(set) = self.set.as_mut() {
            set.prompt_section = to;
            // **`Fin` / `Début` recalent sur le 1 de la section** (§ 11), en marche comme à
            // l'arrêt : c'est le geste du pied quand le groupe n'est pas là où le texte le dit.
            set.beat.restart(now, ticks);
        }
        self.place_prompt(song);
        self.message = Some(if label.is_empty() {
            format!("section {}/{n}", to + 1)
        } else {
            format!("{} ({}/{n})", label, to + 1)
        });
        Action::None
    }

    /// **La position suit le son** (§ 10, à l'arrêt) : le son `at` vient d'être joué, on va à la
    /// prochaine section qui lui est liée. Aucune section liée : on ne bouge pas — sauter au début
    /// du texte parce qu'un son a changé serait pire que de ne rien faire.
    pub(in crate::browse::state) fn follow_prompt(&mut self, at: At) {
        let now = self.now;
        let ticks = self.clock_elapsed_ticks().unwrap_or(0);
        let Some(set) = self.set.as_ref() else { return };
        let same_song = set.at.map(|(i, _)| i) == Some(at.0);
        // **en marche, la position suit le temps** (§ 10, § 14) : changer de son ne la déplace
        // pas, sinon un son joué un temps trop tôt décalerait tout le texte. À l'arrêt seulement,
        // elle suit le son. Changer de **morceau**, lui, remet au début et arrête la battue : c'est
        // un autre morceau, avec sa mesure et son tempo.
        if same_song && set.beat.running {
            return;
        }
        let sound = set
            .spec
            .songs
            .get(at.0)
            .and_then(|s| s.sounds.get(at.1))
            .map(|s| s.preset.clone())
            .unwrap_or_default();
        let from = if same_song { set.prompt_section } else { 0 };
        let found =
            self.prompt_of(at.0).and_then(|l| l.prompt()).and_then(|p| p.section_of_sound(&sound, from));
        if let Some(set) = self.set.as_mut() {
            // on a changé de morceau : le texte repart du début, c'est un autre texte
            if !same_song {
                set.prompt_section = 0;
                set.beat.running = false;
                set.beat.restart(now, ticks);
            }
            if let Some(i) = found {
                set.prompt_section = i;
                set.beat.restart(now, ticks);
            }
        }
        self.place_prompt(at.0);
    }

    /// **`t`** : écrire le prompteur du morceau visé dans `$EDITOR`. L'état ne touche à aucun
    /// fichier (c'est la boucle qui rend la main au terminal) : il dit **quel** morceau, et crée
    /// au besoin le `prompt =` qui manquait dans le set.
    pub(in crate::browse::state) fn edit_prompt(&mut self) -> Action {
        let Some(set) = self.set.as_ref() else {
            self.message = Some("aucun set : L pour en choisir un".into());
            return Action::None;
        };
        let song = set.at.map(|(i, _)| i).unwrap_or(set.cursor);
        let Some(s) = set.spec.songs.get(song) else {
            self.message = Some("set vide : e puis a pour ajouter un morceau".into());
            return Action::None;
        };
        // pas de `prompt =` : on le renseigne (le set devient « non enregistré », donc visible)
        if s.prompt.is_none() {
            let rel = PathBuf::from("prompts").join(prompt_file_name(s.label()));
            if let Some(song_mut) = self.set.as_mut().and_then(|x| x.spec.songs.get_mut(song)) {
                song_mut.prompt = Some(rel.clone());
            }
            // `S` ici enregistre le **preset** : c'est l'édition (`e`) qui enregistre le set, et
            // le dire précisément évite de chercher pourquoi « S » n'a rien écrit
            self.message =
                Some(format!("prompteur {} : à enregistrer dans le set (e puis S)", rel.display()));
        }
        Action::EditPrompt(song)
    }
}

/// **Deux sections voisines de ce nom n'en font qu'une** à l'écran (§ 4 de `prompteur.md`) : le
/// pont en trois grilles, dont une en 2/4, a un seul titre et une seule durée. La comparaison
/// ignore la casse et les espaces de bord ; une section sans nom n'en rejoint aucune. Le
/// prompteur et la bande de battue regroupent par cette seule règle.
pub fn same_name(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Le nom du fichier d'un prompteur créé par `t` : le titre du morceau, en minuscules, les
/// séparateurs remplacés par des tirets. Les accents restent — c'est un nom de fichier pour un
/// musicien, pas un identifiant.
pub fn prompt_file_name(title: &str) -> String {
    let mut out = String::new();
    for c in title.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let name = out.trim_matches('-');
    format!("{}.cho", if name.is_empty() { "morceau" } else { name })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::state::set::ActiveSet;
    use crate::browse::state::tests::with_chain;
    use crate::browse::state::{chain, Action};
    use crate::tui::Key;
    use library::set::{SetSpec, Song, Sound};
    use std::path::Path;

    const CHO: &str = "{title: Le Phare}\n\
        {start_of_verse: Couplet 1}\n{x_namfoop_sound: clean}\n[Am]le phare\n{end_of_verse}\n\
        {start_of_chorus: Refrain}\n{x_namfoop_sound: fuzz}\n{comment: monte}\ntout casse\n{end_of_chorus}\n\
        {start_of_verse: Couplet 2}\n{x_namfoop_sound: clean}\nencore\n{end_of_verse}\n";

    /// Un set d'un morceau à deux sons, son prompteur lu, dans l'espace Set.
    fn staged() -> State {
        let mut s = with_chain(vec![chain::Block::Leaf(chain::Leaf::Nam { model: Some(0), on: true })]);
        let spec = |name: &str| {
            let t = with_chain(vec![chain::Block::Leaf(chain::Leaf::Nam { model: Some(0), on: true })]);
            t.to_preset(name)
        };
        s.adopt_presets(vec![
            ("p1".into(), "clean".into(), "/x/clean.preset.toml".into(), spec("clean")),
            ("p2".into(), "fuzz".into(), "/x/fuzz.preset.toml".into(), spec("fuzz")),
        ]);
        let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
        let set = SetSpec {
            name: "Concert".into(),
            base: PathBuf::from("/x/sets"),
            songs: vec![
                Song {
                    title: Some("Le Phare".into()),
                    prompt: Some(PathBuf::from("prompts/le-phare.cho")),
                    sounds: vec![sound("clean"), sound("fuzz")],
                    ..Default::default()
                },
                Song { title: Some("Néon".into()), sounds: vec![sound("clean")], ..Default::default() },
            ],
        };
        let names: Vec<String> = s.presets.iter().map(|p| p.name.clone()).collect();
        let find = |q: &str| names.iter().position(|n| n == q);
        s.set = Some(ActiveSet::new("s".into(), "concert".into(), set, &find));
        s.adopt_prompts(vec![(
            PathBuf::from("/x/sets/prompts/le-phare.cho"),
            Loaded::Ok(Prompt::parse(CHO)),
        )]);
        s.mark_start();
        s.on_key(Key::Char('3'));
        s
    }

    fn section(s: &State) -> usize {
        s.set.as_ref().expect("un set").prompt_section
    }

    /// **La position suit le son** (§ 10) : changer de son amène à la **prochaine** section liée à
    /// ce son — et pas à la première, sinon jouer le refrain deux fois ramènerait au début.
    #[test]
    fn changing_sound_goes_to_the_next_section_of_that_sound() {
        let mut s = staged();
        s.play_set_sound((0, 0));
        assert_eq!(section(&s), 0, "« clean » : le couplet 1");
        s.play_set_sound((0, 1));
        assert_eq!(section(&s), 1, "« fuzz » : le refrain");
        s.play_set_sound((0, 0));
        assert_eq!(section(&s), 2, "« clean » de nouveau : le COUPLET 2");
        // un morceau sans prompteur ne déplace rien et ne casse rien
        s.play_set_sound((1, 0));
        assert_eq!(section(&s), 0, "autre morceau : son texte repart du début");
    }

    /// **`Fin` / `Début`** : la section suivante / précédente, **sans toucher au son**. C'est le
    /// geste du pied quand le texte et le son ne suivent pas le même découpage.
    #[test]
    fn the_foot_keys_step_the_sections_without_touching_the_sound() {
        let mut s = staged();
        s.play_set_sound((0, 0));
        let heard = s.set.as_ref().unwrap().at;
        s.on_key(Key::End);
        assert_eq!(section(&s), 1);
        assert_eq!(s.set.as_ref().unwrap().at, heard, "le son n'a pas changé");
        assert!(s.message.as_deref().is_some_and(|m| m.contains("Refrain")), "{:?}", s.message);
        s.on_key(Key::End);
        assert_eq!(section(&s), 2);
        // au bout, on le dit au lieu de ne rien faire
        s.on_key(Key::End);
        assert_eq!(section(&s), 2);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("dernière")), "{:?}", s.message);
        s.on_key(Key::Home);
        assert_eq!(section(&s), 1);
        // et les deux touches sont **réglables** (§ 10)
        s.prompt_keys = (Key::Char('>'), Key::Char('<'));
        s.on_key(Key::Char('>'));
        assert_eq!(section(&s), 2, "la touche réglée agit");
        s.on_key(Key::End);
        assert_eq!(section(&s), 2, "et l'ancienne ne fait plus rien");
    }

    /// Un morceau **sans prompteur** le dit, et dit comment en avoir un — mais jamais sur scène,
    /// où `t` est refusée.
    #[test]
    fn a_song_without_a_prompt_says_how_to_get_one() {
        let mut s = staged();
        s.play_set_sound((1, 0));
        s.on_key(Key::End);
        let m = s.message.clone().unwrap_or_default();
        assert!(m.contains("pas de prompteur") && m.contains('t'), "{m}");
        s.locked = true;
        s.message = None;
        s.on_key(Key::End);
        let m = s.message.clone().unwrap_or_default();
        assert_eq!(m, "pas de prompteur pour ce morceau", "sur scène, `t` n'existe pas");
    }

    /// **`t`** : l'état ne touche à aucun fichier — il dit quel morceau, et **renseigne le
    /// `prompt =`** qui manquait, ce qui rend le set « non enregistré ».
    #[test]
    fn t_names_the_file_and_fills_the_set() {
        let mut s = staged();
        s.play_set_sound((1, 0)); // « Néon », sans prompteur
        assert!(matches!(s.on_key(Key::Char('t')), Action::EditPrompt(1)));
        let song = &s.set.as_ref().unwrap().spec.songs[1];
        assert_eq!(song.prompt.as_deref(), Some(Path::new("prompts/néon.cho")));
        assert!(s.unsaved().iter().any(|x| x.contains("set")), "le set est à enregistrer");
        // sur un morceau qui en a déjà un, rien n'est inventé
        s.play_set_sound((0, 0));
        assert!(matches!(s.on_key(Key::Char('t')), Action::EditPrompt(0)));
        assert_eq!(
            s.set.as_ref().unwrap().spec.songs[0].prompt.as_deref(),
            Some(Path::new("prompts/le-phare.cho"))
        );
    }

    /// Le nom de fichier proposé par `t` : lisible par un musicien, et sûr comme nom de fichier.
    #[test]
    fn the_file_name_comes_from_the_title() {
        assert_eq!(prompt_file_name("Le Phare"), "le-phare.cho");
        assert_eq!(prompt_file_name("Marée basse / reprise"), "marée-basse-reprise.cho");
        assert_eq!(prompt_file_name("  "), "morceau.cho");
    }

    /// Un fichier **illisible** n'est pas une erreur bloquante : il est retenu comme tel, et c'est
    /// l'écran qui le dit (§ 10).
    #[test]
    fn an_unreadable_prompt_is_not_an_error() {
        let mut s = staged();
        s.adopt_prompts(vec![(
            PathBuf::from("/x/sets/prompts/le-phare.cho"),
            Loaded::Failed("fichier introuvable".into()),
        )]);
        assert!(s.prompt_of(0).is_some(), "il est connu");
        assert!(s.prompt_of(0).and_then(|l| l.prompt()).is_none(), "mais illisible");
        s.play_set_sound((0, 0));
        s.on_key(Key::End);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("pas de prompteur")), "{:?}", s.message);
    }
}
