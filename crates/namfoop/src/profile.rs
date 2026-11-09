//! Profil sonore des IR lisible par un humain : mini-spectre et étiquettes relatives au pack
//! (« grave +3 », « sombre », « coupe 4,2 kHz »), avec un cache par empreinte du contenu.
//!
//! Le calcul (tiers d'octave) est dans `dsp::profile`. Les étiquettes comparent chaque zone à la
//! médiane du pack (ou de la bibliothèque si le pack a moins de 3 IR) : dans un pack de 11 « Mix »
//! numérotés, elles disent ce qui distingue chacun des autres.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dsp::profile::IrProfile;

use crate::library::{content_hash, Kind, Library};

/// Zones lues par les étiquettes : (nom, de, à) en Hz.
pub const ZONES: [(&str, f32, f32); 7] = [
    ("sub", 40.0, 80.0),
    ("grave", 80.0, 200.0),
    ("bas-médium", 200.0, 500.0),
    ("médium", 500.0, 1500.0),
    ("haut-médium", 1500.0, 3500.0),
    ("présence", 3500.0, 7000.0),
    ("air", 7000.0, 14000.0),
];

/// Niveau de chaque zone (dB), puis l'étendue aiguë en octaves (log2 de `high_cut_hz`).
pub type Zones = [f32; 8];
const EXT: usize = 7;

pub fn zones(p: &IrProfile) -> Zones {
    let mut z = [0.0; 8];
    for (v, (_, lo, hi)) in z.iter_mut().zip(ZONES) {
        *v = p.zone_db(lo, hi);
    }
    z[EXT] = p.high_cut_hz.max(1.0).log2();
    z
}

const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Mini-spectre de 12 caractères (40 Hz – 13 kHz, deux tiers d'octave par caractère), de -18 à +6 dB.
pub fn spark(p: &IrProfile) -> String {
    p.bands_db[2..26]
        .chunks(2)
        .map(|c| {
            let db = c.iter().sum::<f32>() / c.len() as f32;
            let t = ((db + 18.0) / 24.0).clamp(0.0, 0.999);
            SPARK[(t * SPARK.len() as f32) as usize]
        })
        .collect()
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn median_zones(all: &[&Zones]) -> Zones {
    let mut m = [0.0; 8];
    for (k, v) in m.iter_mut().enumerate() {
        *v = median(all.iter().map(|z| z[k]).collect());
    }
    m
}

fn fmt_db(d: f32) -> String {
    format!("{}{:.0}", if d >= 0.0 { "+" } else { "−" }, d.abs())
}

fn fmt_khz(f: f32) -> String {
    format!("{:.1} kHz", f / 1000.0).replace('.', ",")
}

/// Seuil d'une étiquette de zone (dB d'écart à la médiane du pack).
const TAG_DB: f32 = 1.5;

/// Étiquettes d'une IR : écarts à `reference` (médiane du pack) — caractère d'ensemble, puis les
/// zones les plus différentes, puis l'étendue aiguë si elle diffère d'un tiers d'octave ou plus.
pub fn tags(z: &Zones, reference: &Zones, p: &IrProfile) -> Vec<String> {
    let d: Vec<f32> = z.iter().zip(reference).map(|(a, b)| a - b).collect();
    let mut out = Vec::new();
    let mut used = [false; 7];
    let top = (d[5] + d[6]) / 2.0;
    if top >= 2.5 {
        out.push("brillant".to_string());
        used[5] = true;
        used[6] = true;
    } else if top <= -2.5 {
        out.push("sombre".to_string());
        used[5] = true;
        used[6] = true;
    }
    if d[3] >= 2.5 {
        out.push("médium en avant".to_string());
        used[3] = true;
    } else if d[3] <= -2.5 {
        out.push("médium creusé".to_string());
        used[3] = true;
    }
    let mut rest: Vec<usize> = (0..7).filter(|&k| !used[k] && d[k].abs() >= TAG_DB).collect();
    rest.sort_by(|&a, &b| d[b].abs().total_cmp(&d[a].abs()));
    for k in rest.into_iter().take(3usize.saturating_sub(out.len()).min(2)) {
        out.push(format!("{} {}", ZONES[k].0, fmt_db(d[k])));
    }
    if d[EXT] >= 1.0 / 3.0 {
        out.push(format!("aigus jusqu'à {}", fmt_khz(p.high_cut_hz)));
    } else if d[EXT] <= -1.0 / 3.0 {
        out.push(format!("aigus coupés à {}", fmt_khz(p.high_cut_hz)));
    }
    if out.is_empty() {
        out.push("dans la moyenne".to_string());
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Cache

const CACHE_VERSION: u32 = 1;

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    /// Empreinte du contenu -> profil.
    profiles: HashMap<String, IrProfile>,
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(".cache").join("ir-profiles.json")
}

fn load_cache(root: &Path) -> Cache {
    std::fs::read(cache_path(root))
        .ok()
        .and_then(|b| serde_json::from_slice::<Cache>(&b).ok())
        .filter(|c| c.version == CACHE_VERSION)
        .unwrap_or(Cache { version: CACHE_VERSION, ..Default::default() })
}

fn save_cache(root: &Path, c: &Cache) -> Result<()> {
    let p = cache_path(root);
    std::fs::create_dir_all(p.parent().expect("parent"))?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(c)?)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// Description d'une IR pour l'affichage.
#[derive(Clone, Debug)]
pub struct Described {
    /// Courbe complète (panneau spectre du navigateur).
    pub profile: Arc<IrProfile>,
    pub spark: String,
    pub tags: Vec<String>,
}

/// Profils de toutes les IR et mélanges de la bibliothèque, indexés par chemin relatif.
pub struct Profiles {
    pub by_rel: HashMap<String, Described>,
    /// Profils calculés lors de cet appel (absents du cache).
    pub computed: usize,
}

impl Profiles {
    /// Lit le cache, calcule les profils manquants (et les y ajoute), puis étiquette chaque IR.
    /// `progress` est appelé avec le nombre d'IR à calculer, s'il y en a.
    pub fn load(lib: &Library, progress: impl FnOnce(usize)) -> Self {
        let mut cache = load_cache(&lib.root);
        let entries: Vec<_> = lib.entries.iter().filter(|e| matches!(e.kind, Kind::Ir | Kind::Cab)).collect();
        let hashes: Vec<Option<String>> = entries.iter().map(|e| content_hash(&e.path).ok()).collect();
        let missing: Vec<usize> = (0..entries.len())
            .filter(|&i| hashes[i].as_ref().is_some_and(|h| !cache.profiles.contains_key(h)))
            .collect();
        if !missing.is_empty() {
            progress(missing.len());
        }
        let mut computed = 0;
        for &i in &missing {
            match IrProfile::of_file(&entries[i].path) {
                Ok(p) => {
                    cache.profiles.insert(hashes[i].clone().expect("filtré"), p);
                    computed += 1;
                }
                Err(e) => log::debug!("profil impossible pour {} : {e}", entries[i].rel),
            }
        }
        if computed > 0 {
            if let Err(e) = save_cache(&lib.root, &cache) {
                log::warn!("cache des profils non écrit : {e:#}");
            }
        }
        // zones, puis médianes par pack et de la bibliothèque
        let found: Vec<(usize, IrProfile, Zones)> = (0..entries.len())
            .filter_map(|i| {
                let p = cache.profiles.get(hashes[i].as_ref()?)?.clone();
                let z = zones(&p);
                Some((i, p, z))
            })
            .collect();
        let all: Vec<&Zones> = found.iter().map(|(_, _, z)| z).collect();
        let lib_median = median_zones(&all);
        let mut packs: HashMap<&str, Vec<&Zones>> = HashMap::new();
        for (i, _, z) in &found {
            packs.entry(entries[*i].pack.as_str()).or_default().push(z);
        }
        let pack_median: HashMap<&str, Zones> = packs
            .iter()
            .map(|(k, v)| (*k, if v.len() >= 3 { median_zones(v) } else { lib_median }))
            .collect();
        let by_rel = found
            .iter()
            .map(|(i, p, z)| {
                let e = entries[*i];
                let reference = pack_median.get(e.pack.as_str()).unwrap_or(&lib_median);
                (
                    e.rel.clone(),
                    Described { spark: spark(p), tags: tags(z, reference, p), profile: Arc::new(p.clone()) },
                )
            })
            .collect();
        Self { by_rel, computed }
    }

    /// Message de progression standard (stderr) pour les commandes en ligne.
    pub fn load_verbose(lib: &Library) -> Self {
        Self::load(lib, |n| {
            if n >= 10 {
                eprintln!(
                    "profilage de {n} IR (une seule fois, gardé dans {})…",
                    cache_path(&lib.root).display()
                )
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(f: impl Fn(f32) -> f32, high_cut: f32) -> IrProfile {
        IrProfile {
            bands_db: dsp::profile::band_centers().into_iter().map(f).collect(),
            low_cut_hz: 40.0,
            high_cut_hz: high_cut,
        }
    }

    #[test]
    fn tags_are_relative_to_the_pack() {
        let flat = profile(|_| 0.0, 8000.0);
        let reference = zones(&flat);
        assert_eq!(tags(&reference, &reference, &flat), vec!["dans la moyenne"]);
        // plus de présence et d'air, plus de sub, aigus plus étendus
        let bright = profile(
            |f| {
                if f >= 3500.0 {
                    4.0
                } else if f < 80.0 {
                    3.0
                } else {
                    0.0
                }
            },
            12000.0,
        );
        assert_eq!(
            tags(&zones(&bright), &reference, &bright),
            vec!["brillant", "sub +3", "aigus jusqu'à 12,0 kHz"]
        );
        // sombre, aigus coupés plus tôt
        let dark = profile(|f| if f >= 3500.0 { -6.0 } else { 0.0 }, 4200.0);
        assert_eq!(tags(&zones(&dark), &reference, &dark), vec!["sombre", "aigus coupés à 4,2 kHz"]);
        // médium creusé + écart secondaire
        let scoop = profile(
            |f| {
                if (500.0..1500.0).contains(&f) {
                    -5.0
                } else if (80.0..200.0).contains(&f) {
                    2.6
                } else {
                    0.0
                }
            },
            8000.0,
        );
        assert_eq!(tags(&zones(&scoop), &reference, &scoop), vec!["médium creusé", "grave +3"]);
    }

    #[test]
    fn spark_has_twelve_levels_columns() {
        let s = spark(&profile(|f| if f > 3000.0 { -18.0 } else { 6.0 }, 3000.0));
        assert_eq!(s.chars().count(), 12);
        assert!(s.starts_with('█') && s.ends_with('▁'), "{s}");
    }

    #[test]
    fn medians() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(vec![4.0, 1.0, 2.0, 3.0]), 2.5);
    }

    #[test]
    fn cache_is_filled_once_and_follows_content() {
        let d = crate::library::tests::fake_library("profiles", &[]);
        let ir_dir = d.join("ir/Pack");
        std::fs::create_dir_all(&ir_dir).unwrap();
        let mut ir = vec![0.0f32; 2400];
        ir[0] = 1.0;
        dsp::wav::write_mono(&ir_dir.join("a.wav"), &ir, 48000).unwrap();
        ir[1] = -1.0; // dérivateur (passe-haut) : autre contenu
        dsp::wav::write_mono(&ir_dir.join("b.wav"), &ir, 48000).unwrap();
        let lib = Library::scan(&d);
        let mut asked = 0;
        let p = Profiles::load(&lib, |n| asked = n);
        assert_eq!((asked, p.computed, p.by_rel.len()), (2, 2, 2));
        let again = Profiles::load(&lib, |_| panic!("tout est en cache"));
        assert_eq!(again.computed, 0);
        // contenu modifié : recalcul de ce fichier seulement
        ir[2] = 0.5;
        dsp::wav::write_mono(&ir_dir.join("a.wav"), &ir, 48000).unwrap();
        let p = Profiles::load(&Library::scan(&d), |_| {});
        assert_eq!(p.computed, 1);
        assert!(p.by_rel["ir/Pack/b.wav"].profile.zone_db(40.0, 80.0) < -10.0, "b coupe le grave");
    }
}
