//! **La vue scène** : ce que l'espace 3 fait des touches (§ 6 et § 7 de `docs/design/set.md`,
//! #219). Le dessin est dans `ui/set.rs`.
//!
//! La règle qui tient tout : **viser n'est pas jouer**. `↑` / `↓` déplacent un curseur dans la set
//! list sans rien changer à l'oreille ; c'est `Entrée` qui joue le morceau visé, sur son premier
//! son. Sur scène, parcourir sa set list pendant un morceau ne doit produire aucun son.

use super::super::*;
use super::At;

/// La fenêtre **`L` : choisir le set** (§ 7).
#[derive(Clone, Debug, Default)]
pub struct Picker {
    pub cursor: usize,
    /// Les sets de la bibliothèque, dans l'ordre d'affichage.
    pub sets: Vec<Entry>,
}

/// Une ligne de la fenêtre `L`.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// Un set de la bibliothèque : identifiant, nom, morceaux, sons, fichier.
    Set { id: String, stem: String, name: String, songs: usize, sounds: usize, rel: String },
    /// `aucun set` : la banque entière, comme sans mode Set.
    None,
    /// `＋ nouveau set…` : un set vide, à remplir en édition (la suite est l'US 4).
    New,
}

impl Entry {
    /// Ce que la fenêtre affiche à gauche, et à droite.
    pub fn columns(&self) -> (String, String) {
        match self {
            Entry::Set { name, stem, songs, sounds, rel, .. } => {
                let title = if name.trim().is_empty() { stem.clone() } else { name.clone() };
                let n = if *songs <= 1 && *sounds > 0 {
                    // le show à sons : pas de morceaux à compter
                    format!("— morceaux · {sounds} son{}", plural(*sounds))
                } else {
                    format!(
                        "{songs} morceau{} · {sounds} son{}",
                        if *songs > 1 { "x" } else { "" },
                        plural(*sounds)
                    )
                };
                (title, format!("{n} · {rel}"))
            }
            Entry::None => ("aucun set".into(), "la banque entière, comme aujourd'hui".into()),
            Entry::New => ("＋ nouveau set…".into(), "un set vide, à remplir en édition".into()),
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n > 1 {
        "s"
    } else {
        ""
    }
}

impl State {
    /// Les touches de l'espace **Set**. Rend `None` quand la touche ne lui appartient pas, pour
    /// qu'elle retombe sur les touches communes (patron de `fx_key`).
    pub(in crate::browse::state) fn set_key(&mut self, key: Key) -> Option<Action> {
        if self.set_picker.is_some() {
            return self.set_picker_key(key);
        }
        if self.space != space::Space::Set {
            return None;
        }
        let songs = self.set.as_ref().map(|s| s.spec.songs.len()).unwrap_or(0);
        match key {
            // `L` : choisir le set, depuis l'espace Set comme ailleurs
            Key::Char('L') => Some(self.open_set_picker()),
            // **son suivant / précédent** : la touche du pied. Elle ne passait PAS par ici au
            // premier jet (#219) — `step_preset` n'est atteint que par `fx_key`, qui ne tourne que
            // dans l'espace Chaîne — si bien que dans l'espace Set, celui où elle compte le plus,
            // PgDn/PgUp tombaient sur le curseur de liste de la Bibliothèque et ne jouaient rien.
            Key::PageDown => Some(self.step_preset(true)),
            Key::PageUp => Some(self.step_preset(false)),
            // les touches de morceau, pour la même raison : elles n'étaient liées nulle part dans
            // l'espace Set (#218 les avait laissées en attente de cet écran)
            k if k == self.song_keys.0 && songs > 0 => Some(self.step_set_song(true)),
            k if k == self.song_keys.1 && songs > 0 => Some(self.step_set_song(false)),
            _ if songs == 0 => match key {
                Key::Up | Key::Down | Key::Enter | Key::Char('i') => {
                    self.message = Some("aucun set : L pour en choisir un".into());
                    Some(Action::None)
                }
                _ => None,
            },
            Key::Up => {
                let s = self.set.as_mut()?;
                s.cursor = s.cursor.saturating_sub(1);
                Some(Action::None)
            }
            Key::Down => {
                let s = self.set.as_mut()?;
                s.cursor = (s.cursor + 1).min(songs - 1);
                Some(Action::None)
            }
            // `Entrée` : jouer le morceau visé, sur son premier son
            Key::Enter => {
                let s = self.set.as_ref()?;
                let at = s.first_sound(s.cursor);
                Some(match at {
                    Some(at) => self.play_set_sound(at),
                    None => {
                        self.message = Some("ce morceau n'a aucun son jouable".into());
                        Action::None
                    }
                })
            }
            // `i` : déplier ou replier le détail du morceau visé
            Key::Char('i') => {
                let s = self.set.as_mut()?;
                let cur = s.cursor;
                match s.expanded.iter().position(|i| *i == cur) {
                    Some(k) => {
                        s.expanded.remove(k);
                    }
                    None => s.expanded.push(cur),
                }
                Some(Action::None)
            }
            _ => None,
        }
    }

    /// `L` : ouvre la fenêtre de choix du set (§ 7). La liste est demandée à la boucle, qui seule
    /// voit la bibliothèque.
    pub(in crate::browse::state) fn open_set_picker(&mut self) -> Action {
        self.set_picker = Some(Picker::default());
        self.message = None;
        Action::ListSets
    }

    /// Les sets trouvés par la boucle, rangés dans la fenêtre.
    pub fn adopt_sets(&mut self, sets: Vec<Entry>) {
        let Some(p) = self.set_picker.as_mut() else { return };
        // le set actif d'abord visé : on rouvre la fenêtre sur ce qu'on joue
        let here = self.set.as_ref().map(|s| s.id.clone());
        p.sets = sets;
        p.sets.push(Entry::None);
        p.sets.push(Entry::New);
        p.cursor = here
            .and_then(|id| p.sets.iter().position(|e| matches!(e, Entry::Set { id: x, .. } if *x == id)))
            .unwrap_or(0);
    }

    fn set_picker_key(&mut self, key: Key) -> Option<Action> {
        let n = self.set_picker.as_ref()?.sets.len();
        match key {
            Key::Esc | Key::Char('L') => {
                self.set_picker = None;
                self.message = None;
                Some(Action::None)
            }
            Key::Up | Key::PageUp => {
                let p = self.set_picker.as_mut()?;
                p.cursor = p.cursor.saturating_sub(1);
                Some(Action::None)
            }
            Key::Down | Key::PageDown => {
                let p = self.set_picker.as_mut()?;
                p.cursor = (p.cursor + 1).min(n.saturating_sub(1));
                Some(Action::None)
            }
            Key::Enter => {
                let p = self.set_picker.as_ref()?;
                let choice = p.sets.get(p.cursor)?.clone();
                self.set_picker = None;
                Some(match choice {
                    Entry::Set { id, .. } => Action::OpenSet(id),
                    Entry::None => {
                        self.set = None;
                        self.message = Some("aucun set : la banque entière, en boucle".into());
                        // la banque redevient celle de la bibliothèque : il faut la rebâtir
                        Action::SetChanged
                    }
                    Entry::New => {
                        self.message = Some("nouveau set : l'édition vient avec la suite (#220)".into());
                        Action::None
                    }
                })
            }
            _ => Some(Action::None),
        }
    }
}

impl super::ActiveSet {
    /// Le premier son **jouable** d'un morceau (ce que joue `Entrée`).
    pub fn first_sound(&self, song: usize) -> Option<At> {
        self.flat().into_iter().find(|(i, _)| *i == song)
    }

    /// Le morceau est-il déplié (`i`) ?
    pub fn is_expanded(&self, song: usize) -> bool {
        self.expanded.contains(&song)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::state::set::ActiveSet;
    use crate::browse::state::space::Space;
    use crate::browse::state::tests::with_chain;
    use library::set::{SetSpec, Song, Sound};

    fn staged() -> State {
        let mut s = with_chain(vec![]);
        let spec = SetSpec {
            name: "Groupe 1".into(),
            songs: vec![
                Song {
                    title: Some("A".into()),
                    sounds: vec![Sound { preset: "p".into(), volume_db: 0.0 }],
                    ..Default::default()
                },
                Song { title: Some("B (vide)".into()), ..Default::default() },
            ],
            ..Default::default()
        };
        s.set = Some(ActiveSet::new("s1".into(), "g1".into(), spec, &|q| (q == "p").then_some(0)));
        s
    }

    /// `3` mène à la vue scène depuis n'importe où, comme `1` et `2` (§ 6).
    #[test]
    fn the_third_space_is_reachable_from_anywhere() {
        let mut s = staged();
        for from in [Key::Char('1'), Key::Char('2')] {
            s.on_key(from);
            s.on_key(Key::Char('3'));
            assert_eq!(s.space, Space::Set, "depuis {from:?}");
        }
        // et le dire quand on y est déjà
        s.on_key(Key::Char('3'));
        assert!(s.message.as_deref().is_some_and(|m| m.contains("déjà")), "{:?}", s.message);
    }

    /// **Viser n'est pas jouer** : `↑` / `↓` déplacent le curseur sans rien changer à l'oreille,
    /// c'est `Entrée` qui joue (§ 5). Sur scène, parcourir sa set list pendant un morceau ne doit
    /// produire aucun son.
    #[test]
    fn aiming_changes_nothing_until_enter() {
        let mut s = staged();
        s.on_key(Key::Char('3'));
        assert_eq!(s.set.as_ref().unwrap().at, None, "rien n'est joué au départ");
        s.on_key(Key::Down);
        assert_eq!(s.set.as_ref().unwrap().cursor, 1);
        assert_eq!(s.set.as_ref().unwrap().at, None, "viser n'a rien joué");
        // le second morceau n'a aucun son : Entrée le dit au lieu de ne rien faire
        s.on_key(Key::Enter);
        assert_eq!(s.set.as_ref().unwrap().at, None);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("aucun son")), "{:?}", s.message);
        // le premier, lui, se joue
        s.on_key(Key::Up);
        s.on_key(Key::Enter);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)));
        // le curseur ne sort pas de la liste
        for _ in 0..5 {
            s.on_key(Key::Down);
        }
        assert_eq!(s.set.as_ref().unwrap().cursor, 1);
        for _ in 0..5 {
            s.on_key(Key::Up);
        }
        assert_eq!(s.set.as_ref().unwrap().cursor, 0);
    }

    /// `i` déplie et replie, et **plusieurs morceaux peuvent rester dépliés** (§ 6).
    #[test]
    fn the_detail_toggles_and_several_can_stay_open() {
        let mut s = staged();
        s.on_key(Key::Char('3'));
        s.on_key(Key::Char('i'));
        assert!(s.set.as_ref().unwrap().is_expanded(0));
        s.on_key(Key::Down);
        s.on_key(Key::Char('i'));
        let set = s.set.as_ref().unwrap();
        assert!(set.is_expanded(0) && set.is_expanded(1), "les deux restent ouverts");
        s.on_key(Key::Char('i'));
        assert!(!s.set.as_ref().unwrap().is_expanded(1));
    }

    /// `L` : la fenêtre rend `OpenSet` pour un set, `SetChanged` pour « aucun set » — et se ferme
    /// dans les deux cas (§ 7).
    #[test]
    fn the_picker_returns_what_the_loop_must_do() {
        let mut s = staged();
        s.on_key(Key::Char('3'));
        assert!(matches!(s.on_key(Key::Char('L')), Action::ListSets));
        assert!(s.set_picker.is_some());
        s.adopt_sets(vec![Entry::Set {
            id: "s2".into(),
            stem: "trio".into(),
            name: "Trio".into(),
            songs: 1,
            sounds: 3,
            rel: "sets/trio.set.toml".into(),
        }]);
        // la fenêtre s'ouvre sur ce qu'on joue quand il y est ; ici le set actif n'est pas listé
        assert_eq!(s.set_picker.as_ref().unwrap().cursor, 0);
        assert!(matches!(s.on_key(Key::Enter), Action::OpenSet(id) if id == "s2"));
        assert!(s.set_picker.is_none(), "la fenêtre se ferme");

        // « aucun set » rend la banque entière
        s.on_key(Key::Char('L'));
        s.adopt_sets(Vec::new());
        assert_eq!(s.set_picker.as_ref().unwrap().sets, vec![Entry::None, Entry::New]);
        assert!(matches!(s.on_key(Key::Enter), Action::SetChanged));
        assert!(s.set.is_none(), "plus de set actif");

        // Échap referme sans rien changer
        s.on_key(Key::Char('L'));
        s.adopt_sets(Vec::new());
        s.on_key(Key::Esc);
        assert!(s.set_picker.is_none());
    }

    /// Sans set, l'espace 3 répond quand même : il dit où trouver un set (§ 6).
    #[test]
    fn without_a_set_the_keys_point_to_l() {
        let mut s = with_chain(vec![]);
        s.on_key(Key::Char('3'));
        for key in [Key::Down, Key::Enter, Key::Char('i')] {
            s.message = None;
            s.on_key(key);
            assert!(s.message.as_deref().is_some_and(|m| m.contains("L pour")), "{key:?}");
        }
    }
}

#[cfg(test)]
mod foot_tests {
    use super::*;
    use crate::browse::state::set::ActiveSet;
    use crate::browse::state::space::Space;
    use crate::browse::state::tests::with_chain;
    use library::set::{SetSpec, Song, Sound};

    /// Deux morceaux, trois sons, trois presets distincts.
    fn staged() -> State {
        let mut s = with_chain(vec![]);
        let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
        let spec = SetSpec {
            name: "One More Time".into(),
            songs: vec![
                Song {
                    title: Some("A".into()),
                    sounds: vec![sound("p0"), sound("p1")],
                    ..Default::default()
                },
                Song { title: Some("B".into()), sounds: vec![sound("p2")], ..Default::default() },
            ],
            ..Default::default()
        };
        let find = |q: &str| match q {
            "p0" => Some(0),
            "p1" => Some(1),
            "p2" => Some(2),
            _ => None,
        };
        s.set = Some(ActiveSet::new("s".into(), "omt".into(), spec, &find));
        s.on_key(Key::Char('3'));
        assert_eq!(s.space, Space::Set);
        s
    }

    /// **La régression du pied** : dans l'espace Set, `PgDn` / `PgUp` doivent suivre le set. Au
    /// premier jet de #219 elles n'y étaient liées nulle part — `step_preset` n'est atteint que par
    /// `fx_key`, qui ne tourne que dans l'espace Chaîne —, si bien que dans l'espace Set, celui où
    /// la touche du pied compte le plus, elles tombaient sur le curseur de liste de la Bibliothèque
    /// et ne jouaient rien. Ça marchait dans `run --set` (liste blanche explicite), pas dans
    /// `browse` : signalé par l'utilisateur, set « One More Time ».
    #[test]
    fn page_keys_follow_the_set_in_the_set_space() {
        let mut s = staged();
        s.play_set_sound((0, 0));
        for (key, want) in
            [(Key::PageDown, (0, 1)), (Key::PageDown, (1, 0)), (Key::PageUp, (0, 1)), (Key::PageUp, (0, 0))]
        {
            s.on_key(key);
            assert_eq!(s.set.as_ref().unwrap().at, Some(want), "{key:?}");
        }
        // les bords ne bouclent pas (§ 5)
        s.on_key(Key::PageUp);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)));
        assert!(s.message.as_deref().is_some_and(|m| m.contains("début du set")), "{:?}", s.message);
    }

    /// Les touches de **morceau** suivant / précédent, pour la même raison : elles n'agissaient
    /// nulle part dans l'espace Set.
    #[test]
    fn song_keys_work_in_the_set_space() {
        let mut s = staged();
        s.play_set_sound((0, 0));
        s.on_key(s.song_keys.0);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)), "morceau suivant");
        s.on_key(s.song_keys.1);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)), "morceau précédent");
    }

    /// **Et les autres espaces ne changent pas** : dans la Bibliothèque, `PgDn` reste le défilement
    /// de la liste par dix, et `→` / `←` les onglets (§ 5 : la vue scène ne vole rien ailleurs).
    #[test]
    fn the_other_spaces_keep_their_page_keys() {
        let mut s = staged();
        s.on_key(Key::Char('1'));
        let before = s.set.as_ref().unwrap().at;
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, before, "la Bibliothèque ne joue pas le set");
        let tab = s.tab;
        s.on_key(Key::Right);
        assert_ne!(s.tab, tab, "→ change encore d'onglet");
    }
}
