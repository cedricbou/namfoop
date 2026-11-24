//! **Les gabarits MIDI** : les numéros d'un pédalier connu (son, tap, accordeur), à poser d'un coup
//! sur un canal. Gabarits livrés : `profiles/midi/*.toml` du dépôt (inclus dans le binaire) ; ceux
//! de l'utilisateur, dans `midi-templates/` à côté de la config, priment à identifiant égal (nom du
//! fichier sans `.toml`). Un canal n'enregistre pas son gabarit : « personnalisé » se déduit en
//! comparant ses numéros.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Un gabarit : son nom et ses trois numéros de CC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiTemplate {
    pub name: String,
    pub cc_sound: u8,
    pub cc_tap: u8,
    pub cc_tuner: u8,
    /// D'où viennent les numéros : un manuel, un ADR, ou ce qui reste à vérifier.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

const BUILTIN: &[(&str, &str)] = &[
    ("fractal", include_str!("../../../profiles/midi/fractal.toml")),
    ("helix", include_str!("../../../profiles/midi/helix.toml")),
    ("quad-cortex", include_str!("../../../profiles/midi/quad-cortex.toml")),
];

/// Le dossier des gabarits de l'utilisateur : `midi-templates/` à côté du fichier de config.
pub fn user_dir(config_path: &Path) -> PathBuf {
    config_path.parent().unwrap_or(Path::new(".")).join("midi-templates")
}

fn parse(id: &str, text: &str, path: &Path) -> Result<MidiTemplate> {
    let t: MidiTemplate =
        toml::from_str(text).map_err(|source| Error::Parse { path: path.into(), source })?;
    for (what, v) in [("cc_sound", t.cc_sound), ("cc_tap", t.cc_tap), ("cc_tuner", t.cc_tuner)] {
        if !(1..=127).contains(&v) {
            return Err(Error::Msg(format!("gabarit MIDI « {id} » : {what} = {v}, attendu 1 à 127")));
        }
    }
    if t.cc_sound == t.cc_tap || t.cc_sound == t.cc_tuner || t.cc_tap == t.cc_tuner {
        return Err(Error::Msg(format!("gabarit MIDI « {id} » : deux gestes sur le même CC")));
    }
    Ok(t)
}

/// Tous les gabarits, livrés puis ceux de `user` (qui priment), dans l'ordre de leur nom.
pub fn all(user: Option<&Path>) -> Result<Vec<(String, MidiTemplate)>> {
    let mut out: std::collections::BTreeMap<String, MidiTemplate> = BUILTIN
        .iter()
        .map(|(id, text)| Ok((id.to_string(), parse(id, text, &PathBuf::from(format!("{id}.toml")))?)))
        .collect::<Result<_>>()?;
    if let Some(dir) = user.filter(|d| d.is_dir()) {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        files.sort();
        for p in files {
            let id = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let text =
                std::fs::read_to_string(&p).map_err(|source| Error::Read { path: p.clone(), source })?;
            out.insert(id.clone(), parse(&id, &text, &p)?);
        }
    }
    let mut v: Vec<(String, MidiTemplate)> = out.into_iter().collect();
    v.sort_by_key(|a| a.1.name.to_lowercase());
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les gabarits livrés se lisent, et le Fractal est exactement le défaut de namfoop (ADR 0011).
    #[test]
    fn les_gabarits_livres_se_lisent_et_fractal_est_le_defaut() {
        let all = all(None).expect("gabarits livrés");
        let names: Vec<&str> = all.iter().map(|(_, t)| t.name.as_str()).collect();
        assert_eq!(names, ["Fractal", "Helix", "Quad Cortex"]);
        let d = crate::Midi::default();
        let fractal = &all.iter().find(|(id, _)| id == "fractal").expect("fractal").1;
        assert_eq!((fractal.cc_sound, fractal.cc_tap, fractal.cc_tuner), (d.cc_sound, d.cc_tap, d.cc_tuner));
        assert!(
            all.iter().all(|(_, t)| !t.source.is_empty()),
            "chaque gabarit dit d'où viennent ses numéros"
        );
    }

    #[test]
    fn un_gabarit_de_l_utilisateur_prime_et_un_gabarit_faux_est_refuse() {
        let dir = std::env::temp_dir().join(format!("namfoop-gabarits-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("helix.toml"),
            "name = \"Helix\"\ncc_sound = 69\ncc_tap = 64\ncc_tuner = 70\n",
        )
        .unwrap();
        let all = all(Some(&dir)).unwrap();
        assert_eq!(all.iter().find(|(id, _)| id == "helix").unwrap().1.cc_tuner, 70, "le mien prime");
        std::fs::write(dir.join("faux.toml"), "name = \"Faux\"\ncc_sound = 14\ncc_tap = 14\ncc_tuner = 15\n")
            .unwrap();
        assert!(super::all(Some(&dir)).is_err(), "deux gestes sur le même CC");
        std::fs::remove_dir_all(&dir).ok();
    }
}
