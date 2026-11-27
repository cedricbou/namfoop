//! **La calibration jouée**, résolue une fois par la préparation (`prepare.rs`) avant toute commande
//! (`docs/design/instruments.md` § 4) : quel instrument, sur quelle carte, donc quel trim, quel
//! enregistrement, quelle cible. Ceux qui lisaient la calibration dans la config (le cadre de la
//! chaîne, le repère du vumètre, le grain des modèles) la reçoivent d'ici.
//!
//! **On ne réécrit jamais `conf.config` en mémoire** : plusieurs chemins clonent la config chargée
//! puis l'enregistrent (le set retenu de `browse`, l'écran MIDI, `setup`), et ils écriraient la
//! calibration de l'instrument du jour dans `config.toml`.
//!
//! Priorité : `--instrument` > l'instrument par défaut de la bibliothèque > la calibration de la
//! config (bibliothèque sans instrument). `--no-config` ne prend aucun instrument qu'on ne nomme
//! pas : les résultats restent reproductibles.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use library::instrument::{self as lib_instrument, Instrument};

use crate::config::{LevelMode, LoadedConfig};

/// La carte pour laquelle on résout.
#[derive(Clone, Debug, PartialEq)]
pub enum Card {
    /// Une carte réelle, par l'identifiant de son profil.
    Real(String),
    /// La carte simulée.
    Sim,
    /// Aucune : `render`, `models`, `meta`, `import`.
    Offline,
}

/// L'état de la calibration jouée, tel que l'écran l'annonce (§ 7).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum State {
    /// La calibration de la config : aucun instrument dans la bibliothèque, ou `--no-config`.
    #[default]
    Config,
    /// Mesurée sur la carte branchée.
    Calibrated,
    /// Jamais mesurée sur cette carte : joue sans, comme aujourd'hui.
    Uncalibrated,
    /// « Jouée sans », réponse retenue.
    Skipped,
    /// Hors carte (carte simulée, rendu) : la mesure de la carte préférée, sinon la plus récente.
    Borrowed { card: String },
    /// Hors carte, et aucune mesure.
    Never,
}

/// Ce que reçoivent les commandes. `Default` : aucune calibration (trim nul, pas
/// d'enregistrement) — celle de `--no-config`.
#[derive(Clone, Debug, Default)]
pub struct Calibration {
    pub trim_db: f32,
    /// Crête robuste visée lors de la mesure (dBFS) ; `None` : jamais mesurée.
    pub target_dbfs: Option<f32>,
    /// L'enregistrement de calibration, référence du niveau « match » et du grain des modèles.
    pub wav: Option<PathBuf>,
    /// L'instrument joué (« 1 BB 735A ») ; `None` sans instrument.
    pub instrument: Option<String>,
    /// La carte, par l'identifiant de son profil ; `None` hors carte.
    pub card: Option<String>,
    pub state: State,
}

impl Calibration {
    /// Crête de l'instrument attendue à l'entrée de la carte quand on joue fort (cible − trim) :
    /// le repère du vumètre d'entrée. `None` sans calibration.
    pub fn input_peak_dbfs(&self) -> Option<f32> {
        if self.trim_db == 0.0 && self.target_dbfs.is_none() {
            return None;
        }
        Some(self.target_dbfs.unwrap_or(crate::calibrate::DEFAULT_TARGET_DBFS) - self.trim_db)
    }

    /// Le jeu de l'instrument, qui mesure le grain des modèles ; un signal de synthèse sans
    /// enregistrement.
    pub fn signal(&self) -> library::model_profile::Signal {
        library::model_profile::Signal::from_recording(self.wav.as_deref(), self.trim_db)
    }

    /// Mesurée pour ce qu'on joue (sur la carte, ou empruntée hors carte).
    pub fn measured(&self) -> bool {
        matches!(self.state, State::Calibrated | State::Borrowed { .. })
            || (self.state == State::Config && self.input_peak_dbfs().is_some())
    }

    /// Le badge de l'en-tête (§ 7) : `1 BB 735A · itwo · calibrée`. `None` sans instrument.
    pub fn badge(&self) -> Option<String> {
        let who = self.instrument.as_deref()?;
        Some(match (&self.state, &self.card) {
            (State::Calibrated, Some(c)) => format!("{who} · {c} · calibrée"),
            (State::Uncalibrated | State::Skipped, Some(c)) => format!("{who} · {c} · non calibrée"),
            (_, None) if self.measured() => format!("{who} · carte simulée"),
            (_, None) => format!("{who} · carte simulée · non calibrée"),
            (_, Some(c)) => format!("{who} · {c}"),
        })
    }

    /// Le niveau de sortie, faute de `--level` : « match » dès que l'instrument joué a une mesure
    /// avec son enregistrement — sur la carte, ou empruntée hors carte —, sinon celui de la
    /// config. La config ne suffit pas : `setup` y écrit `normalize` en toutes lettres, et une
    /// calibration arrivée avec la bibliothèque doit jouer comme sur la machine qui l'a mesurée.
    /// Sans enregistrement, « match » n'a pas de référence : la config qui le demande joue en
    /// `normalize`, et la ligne de démarrage le dit.
    pub fn level(&self, config: LevelMode) -> LevelMode {
        match (self.instrument.is_some() && self.measured(), self.wav.is_some(), config) {
            (true, true, _) => LevelMode::Match,
            (_, false, LevelMode::Match) => LevelMode::Normalize,
            _ => config,
        }
    }

    /// La ligne de démarrage (§ 4), avec le niveau retenu :
    /// `instrument  1 BB 735A · calibrée sur itwo (trim -17,5 dB) · niveau match`.
    pub fn startup_line(&self, level: LevelMode) -> Option<String> {
        let who = self.instrument.as_deref()?;
        let trim = format!("{:+.1}", self.trim_db).replace('.', ",");
        let what = match (&self.state, &self.card) {
            (State::Calibrated, Some(c)) => format!("calibrée sur {c} (trim {trim} dB)"),
            (State::Skipped, Some(c)) => format!("jouée sans calibration sur {c}"),
            (State::Uncalibrated, Some(c)) => format!(
                "pas calibrée sur {c} : joue sans calibration (`namfoop calibrate --instrument \"{}\"`)",
                who.split_once(' ').map_or(who, |x| x.1)
            ),
            (State::Borrowed { card }, _) => format!("calibration de {card} (trim {trim} dB)"),
            _ => "aucune calibration".into(),
        };
        let level = match level {
            LevelMode::Match => "match",
            LevelMode::Normalize => "normalize",
            LevelMode::Raw => "raw",
        };
        Some(format!("instrument  {who} · {what} · niveau {level}"))
    }
}

/// La calibration de la config, telle que les commandes la lisaient.
fn from_config(conf: &LoadedConfig) -> Calibration {
    if conf.ignored {
        return Calibration::default();
    }
    let c = &conf.config;
    Calibration {
        trim_db: c.trim_db,
        target_dbfs: c.calibration_target_dbfs,
        wav: Some(c.calibration_path(&conf.path)),
        ..Default::default()
    }
}

/// La carte préférée de la config (`[audio] profile`), hors `--no-config`.
fn preferred_card(conf: &LoadedConfig) -> Option<&str> {
    conf.config.audio.profile.as_deref().filter(|_| !conf.ignored)
}

/// Résout la calibration jouée. Pure : la bibliothèque est lue par l'appelant.
pub fn resolve(
    conf: &LoadedConfig,
    instruments: &[Instrument],
    query: Option<&str>,
    card: &Card,
    library: &Path,
) -> Result<Calibration> {
    let chosen = match query {
        Some(q) => lib_instrument::find(instruments, q)?,
        None if conf.ignored => return Ok(Calibration::default()),
        None => match lib_instrument::default_of(instruments) {
            Some(i) => i,
            None => return Ok(from_config(conf)),
        },
    };
    let measure = |key: &str, c: &lib_instrument::Calibration| Calibration {
        trim_db: c.trim_db.unwrap_or(0.0),
        target_dbfs: c.target_dbfs,
        wav: c.wav.as_deref().map(|w| lib_instrument::wav_path(library, w)),
        instrument: Some(chosen.label()),
        card: Some(key.into()),
        state: State::Calibrated,
    };
    let bare = |card: Option<String>, state| Calibration {
        instrument: Some(chosen.label()),
        card,
        state,
        ..Default::default()
    };
    Ok(match card {
        Card::Real(id) => match chosen.calibration.get(id) {
            Some(c) if c.measured() => measure(id, c),
            Some(c) if c.skipped => bare(Some(id.clone()), State::Skipped),
            _ => bare(Some(id.clone()), State::Uncalibrated),
        },
        Card::Sim | Card::Offline => {
            let pref = preferred_card(conf).and_then(|p| chosen.on_card(p).map(|c| (p, c)));
            match pref.or_else(|| chosen.latest()) {
                Some((key, c)) => {
                    Calibration { card: None, state: State::Borrowed { card: key.into() }, ..measure(key, c) }
                }
                None => bare(None, State::Never),
            }
        }
    })
}

/// La carte d'une commande qui ouvre la carte : l'identifiant du profil retenu, sinon le nom du
/// device ALSA réduit à un identifiant.
pub fn card_of(conf: &LoadedConfig, audio: &audio_io::Settings) -> Result<Card> {
    if audio.backend == audio_io::Kind::Sim {
        return Ok(Card::Sim);
    }
    if let Some((id, _)) = crate::cli_audio::card_profile(conf, audio.backend)? {
        return Ok(Card::Real(id));
    }
    let device = audio.device.as_deref().unwrap_or("default");
    Ok(Card::Real(lib_instrument::slug(device.trim_start_matches("hw:"))))
}

/// La bibliothèque peut manquer (`render` s'en passe) : sans elle, aucun instrument.
pub fn load(library: &Path) -> Result<Vec<Instrument>> {
    if !library.is_dir() {
        return Ok(vec![]);
    }
    lib_instrument::load_all(library).map_err(|e| anyhow::anyhow!("{e:#}"))
}

/// La date du jour (UTC), au format TOML : `AAAA-MM-JJ`.
pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Jours depuis le 1970-01-01 -> (année, mois, jour), calendrier grégorien (algorithme de
/// Howard Hinnant).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// L'instrument que `calibrate` mesure : celui qu'on nomme — créé s'il n'existe pas —, sinon le
/// défaut. `None` : aucun instrument et aucun nom, la calibration va dans la config.
pub fn to_calibrate(list: &[Instrument], query: Option<&str>) -> Result<Option<(Instrument, bool)>> {
    match query {
        Some(q) => match lib_instrument::find_opt(list, q).map_err(|e| anyhow::anyhow!("{e:#}"))? {
            Some(i) => Ok(Some((i.clone(), false))),
            None => {
                if q.trim().is_empty() {
                    bail!("--instrument : donne un nom");
                }
                Ok(Some((lib_instrument::create(list, q), true)))
            }
        },
        None => Ok(lib_instrument::default_of(list).map(|i| (i.clone(), false))),
    }
}

/// Écrit l'instrument et son enregistrement après une mesure.
pub fn save_measure(
    library: &Path,
    mut i: Instrument,
    card: &str,
    trim_db: f32,
    target: f32,
    rec: &[f32],
    rate: u32,
) -> Result<(PathBuf, PathBuf)> {
    i.set_calibration(card, trim_db, target, &today()).map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let wav = lib_instrument::wav_path(library, &i.wav_name(card));
    std::fs::create_dir_all(lib_instrument::dir(library))
        .with_context(|| format!("création de {}", lib_instrument::dir(library).display()))?;
    dsp::wav::write_mono(&wav, rec, rate)?;
    let file = lib_instrument::save(library, &i).map_err(|e| anyhow::anyhow!("{e:#}"))?;
    Ok((file, wav))
}

#[cfg(test)]
mod tests {
    use super::*;
    use library::instrument::create;

    fn conf_in(dir: &Path, text: &str) -> LoadedConfig {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join("config.toml");
        std::fs::write(&p, text).unwrap();
        LoadedConfig::load(Some(p), false).unwrap()
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("namfoop-instr-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// BB calibrée sur l'iTwo et sur la MTK (plus récente), Fretless sur rien.
    fn two() -> Vec<Instrument> {
        let mut bb = create(&[], "BB 735A");
        bb.set_calibration("itwo", -17.5, -30.0, "2026-11-20").unwrap();
        bb.set_calibration("mtk", -12.0, -30.0, "2026-11-24").unwrap();
        let mut fl = create(std::slice::from_ref(&bb), "Fretless");
        fl.calibration
            .insert("mtk".into(), library::instrument::Calibration { skipped: true, ..Default::default() });
        vec![bb, fl]
    }

    #[test]
    fn instrument_option_wins_over_default_which_wins_over_config() {
        let d = tmp("prio");
        let conf = conf_in(&d, "trim_db = -3.0\n");
        let lib = d.join("library");
        let itwo = Card::Real("itwo".into());
        // aucun instrument : la config
        let c = resolve(&conf, &[], None, &itwo, &lib).unwrap();
        assert_eq!((c.trim_db, &c.state, c.instrument.as_deref()), (-3.0, &State::Config, None));
        // le défaut
        let c = resolve(&conf, &two(), None, &itwo, &lib).unwrap();
        assert_eq!((c.trim_db, &c.state), (-17.5, &State::Calibrated));
        assert_eq!(c.wav.clone().unwrap(), lib.join("instruments/bb-735a.itwo.wav"));
        assert_eq!(c.badge().unwrap(), "1 BB 735A · itwo · calibrée");
        // --instrument
        let c = resolve(&conf, &two(), Some("fret"), &itwo, &lib).unwrap();
        assert_eq!((c.trim_db, &c.state, c.wav.is_none()), (0.0, &State::Uncalibrated, true));
        assert_eq!(c.badge().unwrap(), "2 Fretless · itwo · non calibrée");
        assert_eq!(c.input_peak_dbfs(), None, "sans calibration, pas de repère");
        let c = resolve(&conf, &two(), Some("2"), &Card::Real("mtk".into()), &lib).unwrap();
        assert_eq!(c.state, State::Skipped);
    }

    #[test]
    fn unknown_instrument_lists_the_others() {
        let d = tmp("unknown");
        let conf = conf_in(&d, "");
        let e = resolve(&conf, &two(), Some("guitare"), &Card::Sim, &d).unwrap_err().to_string();
        assert!(e.contains("1 BB 735A") && e.contains("2 Fretless"), "{e}");
    }

    /// Hors carte : la carte préférée, sinon la plus récente.
    #[test]
    fn offline_takes_the_preferred_card_else_the_latest() {
        let d = tmp("offline");
        let lib = d.join("library");
        let c = resolve(&conf_in(&d, ""), &two(), None, &Card::Offline, &lib).unwrap();
        assert_eq!((c.trim_db, &c.state), (-12.0, &State::Borrowed { card: "mtk".into() }));
        let c =
            resolve(&conf_in(&d, "[audio]\nprofile = \"itwo\"\n"), &two(), None, &Card::Sim, &lib).unwrap();
        assert_eq!(c.trim_db, -17.5);
        assert_eq!(c.badge().unwrap(), "1 BB 735A · carte simulée");
        let c = resolve(&conf_in(&d, ""), &two(), Some("fretless"), &Card::Sim, &lib).unwrap();
        assert_eq!(c.state, State::Never);
    }

    /// `--no-config` : aucun instrument qu'on ne nomme pas.
    #[test]
    fn no_config_takes_no_default_instrument() {
        let conf = LoadedConfig::load(None, true).unwrap();
        let c = resolve(&conf, &two(), None, &Card::Offline, Path::new("/nulle-part")).unwrap();
        assert_eq!((c.trim_db, c.wav.is_none(), c.instrument), (0.0, true, None));
        let c = resolve(&conf, &two(), Some("bb"), &Card::Offline, Path::new("/x")).unwrap();
        assert_eq!(c.trim_db, -12.0, "nommé, il est pris");
    }

    #[test]
    fn startup_lines_say_what_plays() {
        let d = tmp("lines");
        let conf = conf_in(&d, "");
        let c = resolve(&conf, &two(), None, &Card::Real("itwo".into()), &d).unwrap();
        assert_eq!(
            c.startup_line(LevelMode::Match).unwrap(),
            "instrument  1 BB 735A · calibrée sur itwo (trim -17,5 dB) · niveau match"
        );
        let c = resolve(&conf, &two(), Some("2"), &Card::Real("mtk".into()), &d).unwrap();
        assert_eq!(
            c.startup_line(LevelMode::Normalize).unwrap(),
            "instrument  2 Fretless · jouée sans calibration sur mtk · niveau normalize"
        );
        assert_eq!(resolve(&conf, &[], None, &Card::Sim, &d).unwrap().startup_line(LevelMode::Raw), None);
    }

    /// Le niveau suit la calibration de l'instrument, pas la config de la machine : celle qui
    /// arrive avec la bibliothèque joue en « match » comme sur la machine qui l'a mesurée.
    #[test]
    fn a_measured_instrument_plays_matched_whatever_the_config_says() {
        let d = tmp("level");
        let conf = conf_in(&d, "level = \"normalize\"\n");
        let n = LevelMode::Normalize;
        let on = |q, card: &str| resolve(&conf, &two(), q, &Card::Real(card.into()), &d).unwrap();
        assert_eq!(on(None, "itwo").level(n), LevelMode::Match, "mesurée sur la carte");
        assert_eq!(on(Some("2"), "mtk").level(n), n, "« jouée sans » : la config");
        assert_eq!(on(None, "umc").level(LevelMode::Raw), LevelMode::Raw, "jamais calibrée : la config");
        // sans enregistrement, « match » de la config n'a pas de référence
        let m = LevelMode::Match;
        assert_eq!(on(None, "umc").level(m), n, "jamais calibrée, config en match");
        assert_eq!(on(Some("2"), "mtk").level(m), n, "« jouée sans », config en match");
        assert_eq!(on(None, "itwo").level(m), m);
        let c = resolve(&conf, &two(), None, &Card::Offline, &d).unwrap();
        assert_eq!(c.level(n), LevelMode::Match, "hors carte, la mesure empruntée");
        assert_eq!(resolve(&conf, &[], None, &Card::Offline, &d).unwrap().level(n), n, "sans instrument");
    }

    #[test]
    fn calibrate_creates_a_named_instrument_or_takes_the_default() {
        let v = two();
        let (i, new) = to_calibrate(&v, Some("Guitare")).unwrap().unwrap();
        assert!(new && i.number == 3 && i.id == "guitare");
        let (i, new) = to_calibrate(&v, Some("fretless")).unwrap().unwrap();
        assert!(!new && i.number == 2);
        assert_eq!(to_calibrate(&v, None).unwrap().unwrap().0.number, 1);
        assert!(to_calibrate(&[], None).unwrap().is_none());
    }

    #[test]
    fn today_is_a_toml_date() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_782), (2026, 11, 25));
        assert!(today().parse::<toml::value::Datetime>().is_ok());
    }
}
