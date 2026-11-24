//! **Ce qu'un message MIDI demande à un set** : le morceau d'un Program Change, le son d'un CC.
//! Règles pures, partagées par `browse` (qui joue) et l'écran MIDI (qui dit ce qui serait joué) :
//! une seule source pour « ce morceau n'a que deux sons », sinon les deux finiraient par diverger.
//!
//! Une valeur qui ne correspond à rien est **refusée avec sa raison**, jamais remplacée par un
//! autre son (ADR 0011 : Fractal écrête, namfoop le dit).

use library::set::SetSpec;

/// Pourquoi un message ne correspond à rien dans le set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Aucun morceau ne porte ce numéro de Program Change.
    NoSongWithPc(u8),
    /// Un CC de son, alors qu'aucun morceau n'est joué.
    NoSongPlayed,
    /// Le morceau n'a pas ce son (`count` : combien il en a).
    TooFewSounds { title: String, count: usize },
}

impl Refusal {
    /// La raison, telle que l'écran et la ligne d'état la disent.
    pub fn reason(&self) -> String {
        match self {
            Self::NoSongWithPc(n) => format!("aucun morceau du set n'a le numéro {n}"),
            Self::NoSongPlayed => "aucun morceau n'est joué".into(),
            Self::TooFewSounds { count: 0, .. } => "ce morceau n'a aucun son".into(),
            Self::TooFewSounds { title, count } => {
                format!("« {title} » n'a que {count} son{}", if *count > 1 { "s" } else { "" })
            }
        }
    }
}

/// Le morceau dont le `pc` vaut `n`.
pub fn song_for_pc(spec: &SetSpec, n: u8) -> Result<usize, Refusal> {
    spec.songs.iter().position(|s| s.pc == Some(n)).ok_or(Refusal::NoSongWithPc(n))
}

/// Le son `k` (valeur du CC, 0 = le premier) du morceau `song`.
pub fn sound_of(spec: &SetSpec, song: Option<usize>, k: u8) -> Result<(usize, usize), Refusal> {
    let song = song.ok_or(Refusal::NoSongPlayed)?;
    let count = spec.songs.get(song).map_or(0, |s| s.sounds.len());
    if (k as usize) < count {
        Ok((song, k as usize))
    } else {
        Err(Refusal::TooFewSounds { title: title(spec, song), count })
    }
}

/// Le titre d'un morceau, ou son rang s'il n'en a pas.
pub fn title(spec: &SetSpec, song: usize) -> String {
    spec.songs.get(song).and_then(|s| s.title.clone()).unwrap_or_else(|| format!("morceau {}", song + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use library::set::{Song, Sound};

    fn set() -> SetSpec {
        let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
        SetSpec {
            name: "Live".into(),
            songs: vec![
                Song {
                    title: Some("Le Phare".into()),
                    pc: Some(2),
                    sounds: vec![sound("fuzz"), sound("clean")],
                    ..Default::default()
                },
                Song { title: Some("Rappel".into()), pc: None, sounds: vec![], ..Default::default() },
            ],
            base: Default::default(),
        }
    }

    #[test]
    fn un_pc_appelle_le_morceau_qui_porte_son_numero() {
        assert_eq!(song_for_pc(&set(), 2), Ok(0));
        assert_eq!(song_for_pc(&set(), 9).unwrap_err().reason(), "aucun morceau du set n'a le numéro 9");
    }

    #[test]
    fn un_son_hors_du_morceau_est_refuse_avec_sa_raison() {
        assert_eq!(sound_of(&set(), Some(0), 1), Ok((0, 1)));
        assert_eq!(sound_of(&set(), Some(0), 5).unwrap_err().reason(), "« Le Phare » n'a que 2 sons");
        assert_eq!(sound_of(&set(), Some(1), 0).unwrap_err().reason(), "ce morceau n'a aucun son");
        assert_eq!(sound_of(&set(), None, 0).unwrap_err().reason(), "aucun morceau n'est joué");
    }
}
