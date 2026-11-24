//! **Les sets de concert** (`library/sets/<nom>.set.toml`) : la liste ordonnée des morceaux d'un
//! concert, chacun avec ses sons — des **presets de la bibliothèque, référencés et non copiés**
//! (§ 3 de [`docs/design/set.md`](../../../docs/design/set.md)).
//!
//! ```toml
//! name = "Groupe 1 · Live Mairie"
//!
//! [[song]]
//! title = "Le Phare"
//! pc = 2                            # facultatif : le Program Change qui appelle CE morceau
//! tempo_bpm = 124                   # facultatif : prime sur le tempo des presets du morceau
//! time = "4/4"                      # facultatif : mesure par défaut
//! prompt = "prompts/le-phare.cho"   # facultatif : ChordPro, relatif à library/sets/
//!   [[song.sound]]                  # son 1 : CC 69 valeur 0
//!   preset = "fuzz-face"
//!   [[song.sound]]                  # son 2 : CC 69 valeur 1
//!   preset = "clean-svt"
//!   volume_db = -1.5                # facultatif : s'ajoute au volume du preset, dans ce morceau
//! ```
//!
//! Un **son n'a pas de n° de PC** : il est désigné par sa place dans le morceau (CC 69 valeur
//! `place - 1`), comme les snapshots Helix ou les scènes du Quad Cortex. Réordonner les sons d'un
//! morceau change donc leur valeur de CC.
//!
//! Deux sortes de contrôles, et la différence compte sur scène :
//! - **erreur** : le set ne s'ouvre pas (`pc` en double, tempo ou PC hors bornes, mesure
//!   illisible) — ce sont des fautes qui rendraient la navigation imprévisible ;
//! - **avertissement** ([`SetSpec::warnings`]) : le set s'ouvre et **le reste joue**, seul le son
//!   fautif est marqué (un preset introuvable, un prompteur absent). Un concert ne s'arrête pas
//!   parce qu'un son a été renommé.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{bail, Error, Result};

/// Bornes du n° de Program Change (le protocole MIDI les fixe).
pub const PC_MIN: u8 = 1;
pub const PC_MAX: u8 = 128;

/// Un set : des morceaux dans l'ordre.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SetSpec {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "song", skip_serializing_if = "Vec::is_empty")]
    pub songs: Vec<Song>,
    /// Dossier où les chemins de prompteur sont résolus (`library/sets/`) ; non sérialisé.
    #[serde(skip)]
    pub base: PathBuf,
}

/// Un morceau : ses sons, et ce qui lui est propre (tempo, mesure, prompteur).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Song {
    /// Absent : un set **sans titres** (le show à trois sons est un set à un seul morceau).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Program Change qui appelle ce morceau, **unique dans le set**. Absent : le morceau ne se
    /// joue qu'au pied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pc: Option<u8>,
    /// Tempo du morceau (à la noire) : il prime sur celui des presets qu'il joue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo_bpm: Option<f32>,
    /// Mesure par défaut (`4/4`, `6/8`). Gardée **telle qu'écrite** pour que l'aller-retour ne
    /// change pas le fichier ; [`Song::time_sig`] la lit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Prompteur ChordPro, **relatif à `library/sets/`** (§ 10 ; ici, seul le chemin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<PathBuf>,
    #[serde(default, rename = "sound", skip_serializing_if = "Vec::is_empty")]
    pub sounds: Vec<Sound>,
}

/// Un son d'un morceau : un preset de la bibliothèque, et ce que ce morceau en fait.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    /// Identifiant du preset, celui de `namfoop presets`.
    pub preset: String,
    /// S'**ajoute** au volume du preset, dans ce morceau seulement.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub volume_db: f32,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// Une mesure : `4/4`, `6/8`. L'unité est une puissance de deux, comme en solfège.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeSig {
    pub beats: u32,
    pub unit: u32,
}

impl TimeSig {
    pub const COMMON: TimeSig = TimeSig { beats: 4, unit: 4 };
}

impl std::fmt::Display for TimeSig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.beats, self.unit)
    }
}

impl std::str::FromStr for TimeSig {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let (a, b) = s.trim().split_once('/').ok_or_else(|| {
            Error::Msg(format!("mesure « {s} » illisible : attendu « temps/unité », par exemple 4/4"))
        })?;
        let beats: u32 = a
            .trim()
            .parse()
            .map_err(|_| Error::Msg(format!("mesure « {s} » : « {a} » n'est pas un nombre de temps")))?;
        let unit: u32 = b
            .trim()
            .parse()
            .map_err(|_| Error::Msg(format!("mesure « {s} » : « {b} » n'est pas une unité")))?;
        if !(1..=32).contains(&beats) {
            bail!("mesure « {s} » : {beats} temps par mesure, attendu 1 à 32");
        }
        if ![1, 2, 4, 8, 16, 32].contains(&unit) {
            bail!("mesure « {s} » : unité {unit}, attendu une puissance de deux (1, 2, 4, 8, 16, 32)");
        }
        Ok(TimeSig { beats, unit })
    }
}

/// Le fichier est-il un set (`*.set.toml`) ?
pub fn is_set_file(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().ends_with(".set.toml"))
}

/// Nom affichable : nom de fichier sans `.set.toml`.
pub fn display_name(path: &Path) -> String {
    let n = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match n.to_lowercase().strip_suffix(".set.toml") {
        Some(_) => n[..n.len() - ".set.toml".len()].to_string(),
        None => path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(n),
    }
}

/// Un contrôle **non bloquant** : le set s'ouvre, mais ce son ou ce morceau est marqué (§ 3).
#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    /// Index du morceau dans le set.
    pub song: usize,
    /// Index du son dans le morceau ; `None` : le morceau lui-même.
    pub sound: Option<usize>,
    /// Ce que l'écran et `namfoop sets` affichent, au mot près.
    pub text: String,
}

impl Song {
    /// La mesure du morceau, ou `4/4` quand il n'en donne pas. Déjà vérifiée par
    /// [`SetSpec::load`] : une mesure illisible empêche le set de s'ouvrir.
    pub fn time_sig(&self) -> TimeSig {
        self.time.as_deref().and_then(|t| t.parse().ok()).unwrap_or(TimeSig::COMMON)
    }

    /// Ce que l'écran affiche d'un morceau sans titre.
    pub fn label(&self) -> &str {
        self.title.as_deref().filter(|t| !t.trim().is_empty()).unwrap_or("(sans titre)")
    }
}

impl SetSpec {
    /// Lit un set et applique les contrôles **bloquants** (§ 3). `base` devient le dossier du
    /// fichier, où les chemins de prompteur sont résolus.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).map_err(|e| Error::Msg(format!("set {} : {e}", path.display())))?;
        let mut spec: SetSpec =
            toml::from_str(&text).map_err(|e| Error::Msg(format!("set {} : {e}", path.display())))?;
        spec.base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        spec.validate(path)?;
        Ok(spec)
    }

    /// Le morceau qui porte déjà ce numéro de Program Change, s'il y en a un (hors `except`) : un
    /// numéro est **unique dans le set**, et c'est cette règle que l'édition du set et l'écran MIDI
    /// appliquent avant d'écrire.
    pub fn pc_owner(&self, pc: u8, except: usize) -> Option<usize> {
        self.songs.iter().enumerate().find(|(i, s)| *i != except && s.pc == Some(pc)).map(|(i, _)| i)
    }

    /// Écrit le set (son édition, dans `browse` ou l'écran MIDI). L'aller-retour ne change pas le
    /// fichier.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Msg(format!("set {} : {e}", dir.display())))?;
        }
        let text =
            toml::to_string_pretty(self).map_err(|e| Error::Msg(format!("set {} : {e}", path.display())))?;
        std::fs::write(path, text).map_err(|e| Error::Msg(format!("set {} : {e}", path.display())))
    }

    /// Les contrôles qui **empêchent** le set de s'ouvrir : ils rendraient la navigation
    /// imprévisible sur scène.
    fn validate(&self, path: &Path) -> Result<()> {
        let who = |i: usize| format!("morceau {} « {} »", i + 1, self.songs[i].label());
        for (i, song) in self.songs.iter().enumerate() {
            if let Some(pc) = song.pc {
                if !(PC_MIN..=PC_MAX).contains(&pc) {
                    bail!("set {} : {} : pc = {pc}, attendu {PC_MIN} à {PC_MAX}", path.display(), who(i));
                }
                // le même PC sur deux morceaux : lequel jouer ? Le message nomme les deux.
                if let Some(j) = self.songs[..i].iter().position(|s| s.pc == Some(pc)) {
                    bail!(
                        "set {} : pc = {pc} sur deux morceaux, {} et {} : il doit être unique",
                        path.display(),
                        who(j),
                        who(i)
                    );
                }
            }
            if let Some(bpm) = song.tempo_bpm {
                let (lo, hi) = (dsp::tempo::MIN_BPM, dsp::tempo::MAX_BPM);
                if !bpm.is_finite() || bpm < lo || bpm > hi {
                    bail!("set {} : {} : tempo_bpm = {bpm}, attendu {lo} à {hi}", path.display(), who(i));
                }
            }
            if let Some(t) = song.time.as_deref() {
                t.parse::<TimeSig>()
                    .map_err(|e| Error::Msg(format!("set {} : {} : {e}", path.display(), who(i))))?;
            }
        }
        Ok(())
    }

    /// Les contrôles **non bloquants** : `known` dit si un identifiant de preset existe dans la
    /// bibliothèque, `prompt_exists` si un fichier de prompteur est là. Le set joue quand même.
    pub fn warnings(
        &self,
        known: &dyn Fn(&str) -> bool,
        prompt_exists: &dyn Fn(&Path) -> bool,
    ) -> Vec<Warning> {
        let mut out = Vec::new();
        for (i, song) in self.songs.iter().enumerate() {
            if let Some(p) = song.prompt.as_deref() {
                if !prompt_exists(&self.base.join(p)) {
                    out.push(Warning {
                        song: i,
                        sound: None,
                        text: format!("⚠ prompteur {} introuvable", p.display()),
                    });
                }
            }
            for (k, sound) in song.sounds.iter().enumerate() {
                if !known(&sound.preset) {
                    out.push(Warning {
                        song: i,
                        sound: Some(k),
                        text: format!("⚠ {} introuvable", sound.preset),
                    });
                }
            }
        }
        out
    }

    /// Nombre de sons du set, tous morceaux confondus (ce que `namfoop sets` annonce).
    pub fn sound_count(&self) -> usize {
        self.songs.iter().map(|s| s.sounds.len()).sum()
    }

    /// Le morceau que le Program Change `pc` appelle (§ 5). Unique, garanti par [`SetSpec::load`].
    pub fn song_of_pc(&self, pc: u8) -> Option<usize> {
        self.songs.iter().position(|s| s.pc == Some(pc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fake_library;

    /// Les deux exemples du § 3 de la spécification, mot pour mot.
    const GROUPE_1: &str = r#"name = "Groupe 1 · Live Mairie"

[[song]]
title = "Le Phare"
pc = 2
tempo_bpm = 124
time = "4/4"
prompt = "prompts/le-phare.cho"
  [[song.sound]]
  preset = "fuzz-face"
  [[song.sound]]
  preset = "clean-svt"
  volume_db = -1.5

[[song]]
title = "Marée basse"
pc = 4
tempo_bpm = 82
time = "6/8"
  [[song.sound]]
  preset = "clean-svt"
  [[song.sound]]
  preset = "drive-b7k"
"#;

    const TRIO: &str = r#"name = "Trio acoustique"
[[song]]
pc = 1
  [[song.sound]]
  preset = "acou-di"
  [[song.sound]]
  preset = "fretless"
  [[song.sound]]
  preset = "dub-sub"
"#;

    fn written(tag: &str, toml: &str) -> PathBuf {
        fake_library(tag, &[("sets/essai.set.toml", toml)]).join("sets/essai.set.toml")
    }

    /// Le set du § 3 se lit comme il est écrit, y compris ce qu'un son doit **au morceau** et non
    /// au preset (son volume).
    #[test]
    fn the_spec_example_reads_as_written() {
        let spec = SetSpec::load(&written("ex1", GROUPE_1)).expect("set lu");
        assert_eq!(spec.name, "Groupe 1 · Live Mairie");
        assert_eq!(spec.songs.len(), 2);
        assert_eq!(spec.sound_count(), 4);

        let phare = &spec.songs[0];
        assert_eq!(phare.title.as_deref(), Some("Le Phare"));
        assert_eq!(phare.pc, Some(2));
        assert_eq!(phare.tempo_bpm, Some(124.0));
        assert_eq!(phare.time_sig(), TimeSig { beats: 4, unit: 4 });
        assert_eq!(phare.prompt.as_deref(), Some(Path::new("prompts/le-phare.cho")));
        assert_eq!(phare.sounds.len(), 2);
        assert_eq!(phare.sounds[0].preset, "fuzz-face");
        assert_eq!(phare.sounds[0].volume_db, 0.0, "un son sans volume ne change rien");
        assert_eq!(phare.sounds[1].volume_db, -1.5);

        assert_eq!(spec.songs[1].time_sig(), TimeSig { beats: 6, unit: 8 });
        // le PC désigne le morceau, et lui seul (§ 5)
        assert_eq!(spec.song_of_pc(2), Some(0));
        assert_eq!(spec.song_of_pc(4), Some(1));
        assert_eq!(spec.song_of_pc(3), None, "un PC qu'aucun morceau ne porte");
    }

    /// Le show à trois sons : **un seul morceau sans titre** (§ 2).
    #[test]
    fn a_three_sound_show_is_one_untitled_song() {
        let spec = SetSpec::load(&written("ex2", TRIO)).expect("set lu");
        assert_eq!(spec.songs.len(), 1);
        assert_eq!(spec.sound_count(), 3);
        assert_eq!(spec.songs[0].title, None);
        assert_eq!(spec.songs[0].label(), "(sans titre)");
        // sans mesure donnée, c'est 4/4
        assert_eq!(spec.songs[0].time_sig(), TimeSig::COMMON);
    }

    /// Aller-retour : ce qui est lu puis réécrit se relit **à l'identique** (l'édition de l'US 4
    /// ne doit rien perdre).
    #[test]
    fn a_set_survives_a_round_trip() {
        for (tag, toml) in [("rt1", GROUPE_1), ("rt2", TRIO)] {
            let spec = SetSpec::load(&written(tag, toml)).expect("set lu");
            let out = written(&format!("{tag}-out"), "");
            spec.save(&out).expect("set écrit");
            let again = SetSpec::load(&out).expect("set relu");
            assert_eq!(again.songs, spec.songs, "{tag}");
            assert_eq!(again.name, spec.name, "{tag}");
            // et ce qui vaut le défaut ne s'écrit pas : le fichier reste lisible à la main
            let text = std::fs::read_to_string(&out).unwrap();
            assert!(!text.contains("volume_db = 0"), "{tag} : un volume nul ne s'écrit pas :\n{text}");
        }
    }

    /// Un **preset introuvable** est un avertissement : le son reste, marqué, et le reste du set
    /// joue (§ 3). Un concert ne s'arrête pas parce qu'un son a été renommé.
    #[test]
    fn a_missing_preset_is_only_a_warning() {
        let spec = SetSpec::load(&written("warn", GROUPE_1)).expect("set lu");
        let known = |id: &str| id != "clean-svt";
        let w = spec.warnings(&known, &|_| true);
        assert_eq!(w.len(), 2, "le preset manquant sert dans deux morceaux : {w:?}");
        assert_eq!(w[0], Warning { song: 0, sound: Some(1), text: "⚠ clean-svt introuvable".into() });
        assert_eq!(w[1].song, 1);
        assert_eq!(w[1].sound, Some(0));
        // tout va bien : aucun avertissement
        assert!(spec.warnings(&|_| true, &|_| true).is_empty());
        // un prompteur absent est aussi un avertissement, porté par le morceau
        let w = spec.warnings(&|_| true, &|_| false);
        assert_eq!(w.len(), 1);
        assert_eq!((w[0].song, w[0].sound), (0, None));
        assert!(w[0].text.contains("le-phare.cho"), "{}", w[0].text);
    }

    /// Le **même `pc` sur deux morceaux** empêche le set de s'ouvrir, et le message **nomme les
    /// deux** : lequel jouerait-on, sinon ?
    #[test]
    fn the_same_pc_twice_names_both_songs() {
        let toml = r#"name = "Doublon"
[[song]]
title = "Premier"
pc = 7
[[song]]
title = "Second"
pc = 7
"#;
        let err = SetSpec::load(&written("pc2", toml)).expect_err("refusé").to_string();
        assert!(err.contains("pc = 7"), "{err}");
        assert!(err.contains("Premier") && err.contains("Second"), "{err}");
        assert!(err.contains("unique"), "{err}");
    }

    /// Les bornes et la mesure : des erreurs, parce qu'elles rendraient la navigation ou la
    /// battue imprévisibles.
    #[test]
    fn out_of_range_values_and_an_unreadable_time_are_errors() {
        let case = |tag: &str, body: &str| {
            SetSpec::load(&written(tag, &format!("name = \"E\"\n{body}"))).expect_err("refusé").to_string()
        };
        let err = case("pc0", "[[song]]\ntitle = \"A\"\npc = 0\n");
        assert!(err.contains("pc = 0") && err.contains("1 à 128"), "{err}");
        let err = case("pc200", "[[song]]\ntitle = \"A\"\npc = 129\n");
        assert!(err.contains("129"), "{err}");
        let err = case("bpm", "[[song]]\ntitle = \"A\"\ntempo_bpm = 12\n");
        assert!(err.contains("tempo_bpm = 12") && err.contains("30") && err.contains("300"), "{err}");
        let err = case("bpm2", "[[song]]\ntitle = \"A\"\ntempo_bpm = 400\n");
        assert!(err.contains("400"), "{err}");
        for bad in ["4", "4/3", "0/4", "quatre/quatre", "4/"] {
            let err = case("t", &format!("[[song]]\ntitle = \"A\"\ntime = \"{bad}\"\n"));
            assert!(err.contains("mesure"), "« {bad} » : {err}");
            assert!(err.contains('A'), "« {bad} » : le message doit nommer le morceau — {err}");
        }
        // et les mesures légitimes passent
        for good in ["4/4", "6/8", "3/4", "7/8", "12/8", "2/2"] {
            let p = written("tg", &format!("name = \"E\"\n[[song]]\ntime = \"{good}\"\n"));
            assert!(SetSpec::load(&p).is_ok(), "« {good} » devrait passer");
        }
    }

    /// Un morceau **sans son** ou **sans `pc`** est permis : on écrit un set par étapes, et un
    /// morceau peut ne se jouer qu'au pied (§ 3).
    #[test]
    fn a_song_without_sounds_or_pc_is_allowed() {
        let toml = r#"name = "En cours"
[[song]]
title = "Rien encore"
[[song]]
title = "Au pied seulement"
  [[song.sound]]
  preset = "acou-di"
"#;
        let spec = SetSpec::load(&written("vide", toml)).expect("set lu");
        assert_eq!(spec.songs.len(), 2);
        assert_eq!(spec.songs[0].sounds.len(), 0);
        assert_eq!(spec.songs[0].pc, None);
        assert_eq!(spec.sound_count(), 1);
        assert!(spec.warnings(&|_| true, &|_| true).is_empty(), "rien à signaler");
    }

    /// Le nom de fichier : `<nom>.set.toml`, et ce qui n'en est pas un n'est pas un set.
    #[test]
    fn only_set_toml_files_are_sets() {
        assert!(is_set_file(Path::new("sets/Groupe 1.set.toml")));
        assert!(is_set_file(Path::new("sets/GROUPE.SET.TOML")));
        assert!(!is_set_file(Path::new("presets/chill.preset.toml")));
        assert!(!is_set_file(Path::new("cabs/mix.cab.toml")));
        assert_eq!(display_name(Path::new("sets/Groupe 1.set.toml")), "Groupe 1");
        assert_eq!(display_name(Path::new("sets/x.toml")), "x");
    }
}
