//! Profil sonore et grain des modèles NAM, mesurés avec **ta basse** au niveau qu'ils reçoivent.
//!
//! Un modèle est non linéaire : son profil dépend du signal et du niveau. On lui fait jouer
//! l'enregistrement de calibration (les 4 s les plus jouées), multiplié par le trim et l'entrée
//! propre du modèle, soit exactement ce qu'il reçoit en jeu. Sans calibration : des notes de basse
//! synthétiques à -30 dBFS de crête.
//!
//! Mesures (hors temps réel) :
//! - courbe tonale : spectre de sortie / spectre d'entrée par tiers d'octave (`IrProfile::from_transfer`) ;
//!   pour un modèle sans baffle, l'IR préférée y est ajoutée à l'affichage (ce qu'on entend) ;
//! - grain : distorsion harmonique d'un la (110 Hz) joué à ta crête, compression (baisse du facteur
//!   de crête sur ton jeu) et marge avant saturation (gain à partir duquel la distorsion dépasse
//!   3 %). Une mesure par cohérence entrée / sortie sur ton jeu a été essayée : faussée par les
//!   captures avec baffle ou pièce (réponse longue), abandonnée.
//!
//! Le résultat (sans l'IR) est gardé dans `library/.cache/model-profiles.json`, indexé par modèle,
//! entrée totale et signal : refaire `calibrate` ou changer l'entrée d'un modèle le recalcule.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use dsp::chain::{AmpSpec, Level};
use dsp::profile::{crest_db, harmonic_distortion, IrProfile, RATE};

use crate::profile::{describe_all, Described, Profiles};
use crate::recommend::{needs_ir, recommend, NeedsIr};
use crate::{bytes_hash, content_hash, Entry, Kind, Library};
use model::LoadedConfig;

const VERSION: u32 = 3;
const WINDOW_S: f32 = 4.0;
/// Fondamentale du test de saturation : la (corde de la à vide).
const F0: f32 = 110.0;
/// Seuil de « casse » (distorsion harmonique, %).
const BREAK_PCT: f32 = 3.0;

/// Mesure d'un modèle (sans IR).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelProfile {
    /// Courbe tonale du modèle seul.
    pub voicing: IrProfile,
    /// Distorsion harmonique (%) d'un la 110 Hz joué à ta crête : le grain.
    pub thd_pct: f32,
    /// Baisse du facteur de crête sur ton jeu (dB) : compression.
    pub compression_db: f32,
    /// Gain supplémentaire à partir duquel la distorsion dépasse 3 % ; `None` : pas dans ±18 dB.
    pub break_db: Option<f32>,
    /// Mesuré sur le signal de synthèse (pas de calibration).
    pub synthetic: bool,
}

impl ModelProfile {
    /// « clair », « crunch 6 % », « saturé 18 % · compressé », « clair · casse à +6 dB ».
    pub fn grain(&self) -> String {
        let t = self.thd_pct;
        let mut s = if t < 2.0 {
            "clair".to_string()
        } else if t < 8.0 {
            format!("crunch {t:.0} %")
        } else if t < 20.0 {
            format!("saturé {t:.0} %")
        } else {
            format!("très saturé {t:.0} %")
        };
        if self.compression_db >= 5.0 {
            s.push_str(" · compressé");
        }
        if t < BREAK_PCT {
            match self.break_db {
                Some(b) if b > 0.0 => s.push_str(&format!(" · casse à +{b:.0} dB")),
                None => s.push_str(" · ne casse pas"),
                _ => {}
            }
        }
        s
    }
}

// ---------------------------------------------------------------------------------------------
// Le signal de mesure : fenêtre la plus forte, basse synthétique, crête robuste

/// Signal de mesure, au niveau de la carte (avant trim).
pub struct Signal {
    pub samples: Vec<f32>,
    /// Gain (dB) appliqué avant le modèle en plus de l'entrée propre : le trim.
    pub trim_db: f32,
    pub synthetic: bool,
    hash: String,
}

/// Les `WINDOW_S` secondes les plus jouées (RMS max par blocs de 0,5 s).
fn loudest_window(x: &[f32], sr: u32) -> &[f32] {
    let block = (sr / 2) as usize;
    let len = (WINDOW_S * sr as f32) as usize;
    if x.len() <= len {
        return x;
    }
    let energy: Vec<f64> = x.chunks(block).map(|c| c.iter().map(|v| (*v as f64).powi(2)).sum()).collect();
    let n = len / block;
    let best = (0..=energy.len().saturating_sub(n))
        .max_by(|&a, &b| {
            energy[a..a + n].iter().sum::<f64>().total_cmp(&energy[b..b + n].iter().sum::<f64>())
        })
        .unwrap_or(0);
    &x[best * block..(best * block + len).min(x.len())]
}

/// Notes de basse synthétiques (Karplus-Strong), crête robuste à -30 dBFS.
pub fn synthetic_bass() -> Vec<f32> {
    let mut out = Vec::new();
    let mut seed = 0x1234_5678u32;
    let notes = [41.2f32, 55.0, 73.42, 98.0, 61.74, 82.41, 49.0, 65.41];
    while out.len() < (WINDOW_S * RATE as f32) as usize {
        for (k, f) in notes.iter().enumerate() {
            let period = (RATE as f32 / f) as usize;
            let mut line: Vec<f32> = (0..period)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
                })
                .collect();
            let vel = if k % 2 == 0 { 1.0 } else { 0.6 };
            for i in 0..(0.75 * RATE as f32) as usize {
                let j = i % period;
                let next = 0.996 * 0.5 * (line[j] + line[(j + 1) % period]);
                out.push(line[j] * vel);
                line[j] = next;
            }
        }
    }
    let g = dsp::db_to_gain(-30.0) / robust_peak(&out).max(1e-9);
    out.iter_mut().for_each(|v| *v *= g);
    out
}

/// Crête robuste (99,9e centile), comme `calibrate`.
fn robust_peak(x: &[f32]) -> f32 {
    let mut a: Vec<f32> = x.iter().map(|v| v.abs()).collect();
    a.sort_by(|p, q| p.total_cmp(q));
    a.get(((a.len() as f64) * 0.999) as usize).copied().unwrap_or(0.0)
}

impl Signal {
    /// Enregistrement de calibration de la config, sinon signal de synthèse.
    pub fn from_config(conf: &LoadedConfig) -> Self {
        let path = conf.config.calibration_path(&conf.path);
        match dsp::wav::read_mono(&path) {
            Ok((x, sr)) if !conf.ignored && !x.is_empty() => {
                let x = if sr == RATE { x } else { dsp::ir::resample(&x, sr, RATE) };
                Self::new(loudest_window(&x, RATE).to_vec(), conf.config.trim_db, false)
            }
            _ => Self::new(synthetic_bass(), 0.0, true),
        }
    }

    pub fn new(samples: Vec<f32>, trim_db: f32, synthetic: bool) -> Self {
        let bytes: Vec<u8> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        let hash = bytes_hash(&bytes);
        Self { samples, trim_db, synthetic, hash }
    }
}

// ---------------------------------------------------------------------------------------------
// Mesurer un modèle, et composer son profil avec celui d'une IR

/// Mesure un modèle : `input_db` = entrée propre du modèle (en plus du trim du signal).
pub fn measure(model: &Path, signal: &Signal, input_db: f32) -> Result<ModelProfile> {
    let spec = AmpSpec { model: Some(model.to_path_buf()), level: Level::Raw, ..Default::default() };
    let mut chain = spec.build(RATE, 4096)?;
    let gain = dsp::db_to_gain(signal.trim_db + input_db);
    let x: Vec<f32> = signal.samples.iter().map(|v| v * gain).collect();
    let mut y = x.clone();
    for c in y.chunks_mut(4096) {
        chain.process(c);
    }
    let voicing = IrProfile::from_transfer(&x, &y, RATE);
    let compression_db = crest_db(&x) - crest_db(&y);
    // marge avant saturation : sinus en paliers de 3 dB (-18 → +18), d'un seul tenant (une remise
    // à zéro du modèle coûte cher), distorsion mesurée sur la fin de chaque palier
    let peak = robust_peak(&x);
    let step = (0.2 * RATE as f32) as usize;
    let gains: Vec<f32> = (-6..=6).map(|k| k as f32 * 3.0).collect();
    let mut s: Vec<f32> = Vec::with_capacity(step * gains.len());
    for (k, g) in gains.iter().enumerate() {
        let amp = peak * dsp::db_to_gain(*g);
        for i in 0..step {
            let n = k * step + i;
            s.push(amp * (2.0 * std::f32::consts::PI * F0 * n as f32 / RATE as f32).sin());
        }
    }
    chain.reset();
    for c in s.chunks_mut(4096) {
        chain.process(c);
    }
    let thd = |k: usize| harmonic_distortion(&s[k * step + step / 3..(k + 1) * step], RATE, F0);
    let thd_pct = thd(6);
    let break_db = (0..gains.len()).find(|&k| thd(k) > BREAK_PCT).map(|k| gains[k]);
    Ok(ModelProfile { voicing, thd_pct, compression_db, break_db, synthetic: signal.synthetic })
}

/// Courbe entendue : la courbe du modèle + la réponse de l'IR, ramenée à la même référence.
pub fn with_ir(voicing: &IrProfile, ir: &IrProfile) -> IrProfile {
    let centers = dsp::profile::band_centers();
    let sum: Vec<f32> = voicing.bands_db.iter().zip(&ir.bands_db).map(|(a, b)| a + b).collect();
    let refs: Vec<f32> =
        centers.iter().zip(&sum).filter(|(c, _)| (80.0..=5000.0).contains(*c)).map(|(_, d)| *d).collect();
    // moyenne en puissance, comme les profils d'IR
    let mean_pow = refs.iter().map(|d| 10f64.powf(*d as f64 / 10.0)).sum::<f64>() / refs.len().max(1) as f64;
    let offset = (10.0 * mean_pow.max(1e-30).log10()) as f32;
    let bands_db: Vec<f32> = sum.iter().map(|d| d - offset).collect();
    IrProfile {
        high_cut_hz: voicing.high_cut_hz.min(ir.high_cut_hz),
        low_cut_hz: voicing.low_cut_hz.max(ir.low_cut_hz),
        bands_db,
    }
}

// ---------------------------------------------------------------------------------------------
// Le cache sur disque, et l'accès par la bibliothèque

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    profiles: HashMap<String, ModelProfile>,
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(".cache").join("model-profiles.json")
}

fn key(model_hash: &str, input_db: f32, signal: &Signal) -> String {
    format!("{model_hash}|{:.1}|{}", signal.trim_db + input_db, signal.hash)
}

/// Profils des modèles de la bibliothèque, par chemin relatif.
pub struct ModelProfiles {
    pub by_rel: HashMap<String, Described>,
    pub measures: HashMap<String, ModelProfile>,
    /// IR prise en compte dans la courbe affichée (modèles sans baffle).
    pub ir_used: HashMap<String, String>,
    pub computed: usize,
    pub synthetic: bool,
}

impl ModelProfiles {
    /// Lit le cache, mesure les modèles manquants (en parallèle), puis étiquette chaque modèle par
    /// rapport à son pack. `irs` fournit la réponse des IR pour la courbe entendue.
    pub fn load(lib: &Library, conf: &LoadedConfig, irs: &Profiles, progress: impl FnOnce(usize)) -> Self {
        let signal = Signal::from_config(conf);
        let mut cache: Cache = std::fs::read(cache_path(&lib.root))
            .ok()
            .and_then(|b| serde_json::from_slice::<Cache>(&b).ok())
            .filter(|c| c.version == VERSION)
            .unwrap_or(Cache { version: VERSION, ..Default::default() });
        let models: Vec<&Entry> = lib.of_kind(Kind::Model).collect();
        let metas: Vec<_> = models.iter().map(|e| lib.meta(&e.path).ok()).collect();
        let keys: Vec<Option<String>> = models
            .iter()
            .zip(&metas)
            .map(|(e, m)| {
                let h = content_hash(&e.path).ok()?;
                Some(key(&h, m.as_ref().and_then(|m| m.input_db).unwrap_or(0.0), &signal))
            })
            .collect();
        let missing: Vec<usize> = (0..models.len())
            .filter(|&i| keys[i].as_ref().is_some_and(|k| !cache.profiles.contains_key(k)))
            .collect();
        if !missing.is_empty() {
            progress(missing.len());
        }
        // mesure en parallèle : une instance NAM par thread
        let level = log::max_level();
        log::set_max_level(log::LevelFilter::Warn.min(level)); // pas de « NAM … » par modèle
        let next = AtomicUsize::new(0);
        let results = Mutex::new(Vec::new());
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(missing.len().max(1));
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&i) = missing.get(k) else { break };
                    let input = metas[i].as_ref().and_then(|m| m.input_db).unwrap_or(0.0);
                    match measure(&models[i].path, &signal, input) {
                        Ok(p) => results.lock().expect("verrou").push((i, p)),
                        Err(e) => log::debug!("profil impossible pour {} : {e:#}", models[i].rel),
                    }
                });
            }
        });
        log::set_max_level(level);
        if std::env::var_os("NAMFOOP_PROFILE_DEBUG").is_some() {
            for (i, p) in results.lock().expect("verrou").iter() {
                println!(
                    "grain\t{:.1}\t{:.1}\t{:?}\t{}",
                    p.thd_pct, p.compression_db, p.break_db, models[*i].rel
                );
            }
        }
        let results = results.into_inner().expect("verrou");
        let computed = results.len();
        for (i, p) in results {
            cache.profiles.insert(keys[i].clone().expect("filtré"), p);
        }
        if computed > 0 {
            let p = cache_path(&lib.root);
            let saved = std::fs::create_dir_all(p.parent().expect("parent"))
                .and_then(|_| {
                    std::fs::write(
                        p.with_extension("json.tmp"),
                        serde_json::to_vec(&cache).unwrap_or_default(),
                    )
                })
                .and_then(|_| std::fs::rename(p.with_extension("json.tmp"), &p));
            if let Err(e) = saved {
                log::warn!("cache des profils de modèles non écrit : {e}");
            }
        }
        // courbe entendue (avec l'IR préférée ou la mieux conseillée pour un modèle sans baffle)
        let mut items = Vec::new();
        let mut measures = HashMap::new();
        let mut ir_used = HashMap::new();
        for (i, e) in models.iter().enumerate() {
            let Some(p) = keys[i].as_ref().and_then(|k| cache.profiles.get(k)) else { continue };
            let mut curve = p.voicing.clone();
            if let Some(meta) = &metas[i] {
                if needs_ir(&meta.gear_type) == NeedsIr::Yes {
                    if let Some(s) = recommend(lib, e, meta, conf.config.default_ir.as_deref()).first() {
                        if let Some(d) = irs.by_rel.get(&s.entry.rel) {
                            curve = with_ir(&p.voicing, &d.profile);
                            ir_used.insert(e.rel.clone(), s.entry.stem.clone());
                        }
                    }
                }
            }
            items.push((e.rel.clone(), e.pack.clone(), curve));
            measures.insert(e.rel.clone(), p.clone());
        }
        let mut by_rel = describe_all(items);
        for (rel, d) in by_rel.iter_mut() {
            d.grain = measures.get(rel).map(ModelProfile::grain);
        }
        Self { by_rel, measures, ir_used, computed, synthetic: signal.synthetic }
    }

    /// Avec message de progression (commandes en ligne).
    pub fn load_verbose(lib: &Library, conf: &LoadedConfig, irs: &Profiles) -> Self {
        let p = Self::load(lib, conf, irs, |n| {
            if n >= 3 {
                eprintln!("profilage de {n} modèles avec ton enregistrement de calibration (une seule fois)…")
            }
        });
        if p.synthetic && !p.by_rel.is_empty() {
            eprintln!("profils des modèles mesurés sur un signal de synthèse : lance `namfoop calibrate` pour les mesurer avec ta basse");
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(thd: f32, comp: f32, brk: Option<f32>) -> ModelProfile {
        ModelProfile {
            voicing: IrProfile { bands_db: vec![0.0; 28], low_cut_hz: 31.0, high_cut_hz: 16000.0 },
            thd_pct: thd,
            compression_db: comp,
            break_db: brk,
            synthetic: false,
        }
    }

    #[test]
    fn grain_labels() {
        assert_eq!(measured(0.5, 1.0, Some(6.0)).grain(), "clair · casse à +6 dB");
        assert_eq!(measured(0.5, 1.0, None).grain(), "clair · ne casse pas");
        assert_eq!(measured(6.2, 2.0, Some(-3.0)).grain(), "crunch 6 %");
        assert_eq!(measured(18.0, 7.0, Some(-9.0)).grain(), "saturé 18 % · compressé");
        assert_eq!(measured(30.0, 4.0, Some(-18.0)).grain(), "très saturé 30 %");
    }

    #[test]
    fn loudest_window_and_synthetic_signal() {
        let mut x = vec![0.001f32; 48000 * 10];
        for v in x[48000 * 3..48000 * 9].iter_mut() {
            *v = 0.5;
        }
        let w = loudest_window(&x, 48000);
        assert_eq!(w.len(), 48000 * 4);
        assert!(w.iter().all(|v| *v == 0.5));
        let s = synthetic_bass();
        assert!(s.len() >= 48000 * 4);
        assert!((dsp::gain_to_db(robust_peak(&s)) + 30.0).abs() < 0.5);
    }

    #[test]
    fn with_ir_adds_the_cabinet_response() {
        let v = IrProfile { bands_db: vec![0.0; 28], low_cut_hz: 31.0, high_cut_hz: 16000.0 };
        let mut ir = v.clone();
        ir.bands_db[27] = -30.0;
        ir.high_cut_hz = 5000.0;
        let w = with_ir(&v, &ir);
        assert!(w.bands_db[27] < -29.0 && w.bands_db[10].abs() < 0.1);
        assert_eq!(w.high_cut_hz, 5000.0);
    }

    #[test]
    fn real_model_is_measured_and_level_dependent() {
        let model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models/wavenet.nam");
        let signal = Signal::new(synthetic_bass(), 0.0, true);
        let soft = measure(&model, &signal, -12.0).unwrap();
        let hot = measure(&model, &signal, 12.0).unwrap();
        assert!(soft.thd_pct.is_finite() && hot.thd_pct.is_finite());
        assert!(
            hot.thd_pct > soft.thd_pct,
            "plus d'entrée, plus de distorsion : {} vs {}",
            hot.thd_pct,
            soft.thd_pct
        );
        assert_eq!(soft.voicing.bands_db.len(), 28);
        // la clé de cache dépend de l'entrée et du signal
        let other = Signal::new(synthetic_bass(), -6.0, true);
        assert_ne!(key("h", 0.0, &signal), key("h", 3.0, &signal));
        assert_ne!(key("h", 0.0, &signal), key("h", 0.0, &other));
    }
}
