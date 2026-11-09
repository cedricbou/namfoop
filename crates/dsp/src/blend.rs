//! Mélanges d'IR (`.cab.toml`) : plusieurs micros / positions sommés en une seule IR, au
//! chargement. En live, le coût est celui d'une IR unique (convolution linéaire : la somme des
//! convolutions est la convolution de la somme).
//!
//! ```toml
//! name = "Fender 70 121 + Beta52"
//! align = true          # alignement temporel automatique (défaut : true)
//! max_ms = 200          # optionnel
//! [[ir]]
//! file = "ir/1970 Fender .../1970 Bassman Cabinet CTS - 121 Lower - Cap.wav"
//! gain_db = 0.0
//! [[ir]]
//! file = "ir/1970 Fender .../1970 Bassman Cabinet CTS - Beta52 Lower - Cap.wav"
//! gain_db = -3.0
//! invert = false
//! ```
//!
//! `file` : relatif au dossier du `.cab.toml`, puis au dossier parent (la bibliothèque quand le
//! mélange est rangé dans `library/cabs/`), ou absolu.
//!
//! Alignement : chaque IR est recadrée sur son départ (premier échantillon à plus de 10 % de sa
//! crête, avec 0,5 ms de marge), puis ajustée par corrélation croisée sur ±1 ms par rapport à la
//! première IR. Sans alignement, deux micros à des distances différentes se combinent en filtre en
//! peigne (creux dans le grave ou les médiums).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{db_to_gain, ir, Error, Result};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlendPart {
    pub file: PathBuf,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlendSpec {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_true")]
    pub align: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<f32>,
    #[serde(rename = "ir")]
    pub parts: Vec<BlendPart>,
    /// Dossier du fichier (pour résoudre les chemins relatifs) ; non sérialisé.
    #[serde(skip)]
    pub base: PathBuf,
}

pub fn is_cab_file(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().ends_with(".cab.toml"))
}

/// Nom affichable : nom de fichier sans `.wav` ni `.cab.toml`.
pub fn display_name(path: &Path) -> String {
    let n = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = n.to_lowercase();
    if lower.ends_with(".cab.toml") {
        n[..n.len() - ".cab.toml".len()].to_string()
    } else {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(n)
    }
}

fn err(path: &Path, msg: impl Into<String>) -> Error {
    Error::Ir(path.display().to_string(), msg.into())
}

impl BlendSpec {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| err(path, e.to_string()))?;
        let mut spec: BlendSpec =
            toml::from_str(&text).map_err(|e| err(path, format!("mélange invalide : {e}")))?;
        if spec.parts.is_empty() {
            return Err(err(path, "mélange sans IR ([[ir]] file = ...)"));
        }
        spec.base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        Ok(spec)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let body = toml::to_string_pretty(self).map_err(|e| err(path, e.to_string()))?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| err(path, e.to_string()))?;
        }
        std::fs::write(path, format!("# mélange d'IR namfoop (modifiable à la main)\n{body}"))
            .map_err(|e| err(path, e.to_string()))
    }

    /// Chemin réel d'une IR du mélange.
    pub fn resolve_file(&self, file: &Path) -> Option<PathBuf> {
        if file.is_absolute() {
            return file.exists().then(|| file.to_path_buf());
        }
        [Some(self.base.as_path()), self.base.parent()]
            .into_iter()
            .flatten()
            .map(|d| d.join(file))
            .find(|p| p.exists())
    }

    /// Calcule l'IR mélangée, brute (sans troncature ni normalisation).
    pub fn mix(&self, sample_rate: u32) -> Result<Vec<f32>> {
        let mut irs = Vec::new();
        for p in &self.parts {
            let path = self
                .resolve_file(&p.file)
                .ok_or_else(|| err(&self.base.join(&p.file), "IR du mélange introuvable"))?;
            irs.push(ir::load_raw(&path, sample_rate)?);
        }
        if self.align {
            for (i, polarity) in align(&mut irs, sample_rate).into_iter().enumerate() {
                if polarity < 0.0 && !self.parts[i].invert {
                    log::warn!(
                        "mélange « {} » : {} semble en opposition de phase avec la première IR (invert = true ?)",
                        self.name,
                        self.parts[i].file.display()
                    );
                }
            }
        }
        let len = irs.iter().map(Vec::len).max().unwrap_or(0);
        let mut out = vec![0.0f32; len];
        for (x, p) in irs.iter().zip(&self.parts) {
            let g = db_to_gain(p.gain_db) * if p.invert { -1.0 } else { 1.0 };
            for (o, v) in out.iter_mut().zip(x) {
                *o += g * v;
            }
        }
        Ok(out)
    }
}

/// Indice du départ : premier échantillon dont |x| dépasse 10 % de la crête.
pub fn onset(x: &[f32]) -> usize {
    let peak = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    x.iter().position(|v| v.abs() >= 0.1 * peak).unwrap_or(0)
}

/// Aligne les IR sur place (recadrage sur le départ, puis corrélation fine sur ±1 ms avec la
/// première). Renvoie le signe de la corrélation de chaque IR avec la première (1 pour elle-même).
pub fn align(irs: &mut [Vec<f32>], sample_rate: u32) -> Vec<f32> {
    let margin = (sample_rate as f32 * 0.0005) as usize;
    let mut shift = Vec::with_capacity(irs.len());
    for x in irs.iter_mut() {
        let cut = onset(x).saturating_sub(margin);
        x.drain(..cut);
        shift.push(cut as isize);
    }
    let r = (sample_rate as f32 * 0.001) as isize;
    let w = ((sample_rate as f32 * 0.02) as usize).min(irs.first().map_or(0, Vec::len));
    let mut signs = vec![1.0f32; irs.len()];
    if irs.len() < 2 || w == 0 {
        return signs;
    }
    let reference = irs[0][..w].to_vec();
    for (i, x) in irs.iter_mut().enumerate().skip(1) {
        // x[n + lag] ≈ référence[n] : lag > 0 = x est en retard
        let corr = |lag: isize| -> f32 {
            (0..w)
                .map(|n| {
                    let k = n as isize + lag;
                    if k < 0 || k as usize >= x.len() {
                        0.0
                    } else {
                        reference[n] * x[k as usize]
                    }
                })
                .sum()
        };
        let (best, c) =
            (-r..=r).map(|l| (l, corr(l))).max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap();
        signs[i] = c.signum();
        let total = shift[i] + best - shift[0];
        log::info!(
            "alignement : IR {} décalée de {total} échantillon(s) ({:.2} ms) par rapport à la 1re",
            i + 1,
            total as f32 * 1000.0 / sample_rate as f32
        );
        if best > 0 {
            x.drain(..best as usize);
        } else if best < 0 {
            x.splice(0..0, std::iter::repeat_n(0.0, (-best) as usize));
        }
    }
    signs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cab_ir(len: usize, delay: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; delay];
        v.extend((0..len).map(|i| (-(i as f32) / 120.0).exp() * ((i as f32) * 0.21).sin()));
        v
    }

    fn dir(tag: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("namfoop-blend-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("ir/pack")).unwrap();
        std::fs::create_dir_all(d.join("cabs")).unwrap();
        d
    }

    fn write(d: &Path, name: &str, x: &[f32], sr: u32) -> PathBuf {
        let p = d.join("ir/pack").join(name);
        crate::wav::write_mono(&p, x, sr).unwrap();
        p
    }

    fn spec(d: &Path, parts: &[(&str, f32, bool)], align: bool) -> PathBuf {
        let s = BlendSpec {
            name: "test".into(),
            align,
            max_ms: None,
            parts: parts
                .iter()
                .map(|(f, g, inv)| BlendPart {
                    file: PathBuf::from("ir/pack").join(f),
                    gain_db: *g,
                    invert: *inv,
                })
                .collect(),
            base: PathBuf::new(),
        };
        let p = d.join("cabs/test.cab.toml");
        s.save(&p).unwrap();
        p
    }

    fn max_diff(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn aligned_copies_sum_to_the_same_shape() {
        let d = dir("align");
        write(&d, "a.wav", &cab_ir(2000, 10), 48000);
        write(&d, "b.wav", &cab_ir(2000, 40), 48000); // même IR, 30 échantillons plus tard
        let mixed = BlendSpec::load(&spec(&d, &[("a.wav", 0.0, false), ("b.wav", 0.0, false)], true))
            .unwrap()
            .mix(48000)
            .unwrap();
        let single = BlendSpec::load(&spec(&d, &[("a.wav", 0.0, false)], true)).unwrap().mix(48000).unwrap();
        let doubled: Vec<f32> = single.iter().map(|x| 2.0 * x).collect();
        assert!(max_diff(&mixed, &doubled) < 1e-4, "écart {}", max_diff(&mixed, &doubled));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn unaligned_copies_comb_filter() {
        let d = dir("comb");
        write(&d, "a.wav", &cab_ir(2000, 10), 48000);
        write(&d, "b.wav", &cab_ir(2000, 40), 48000);
        let mixed = BlendSpec::load(&spec(&d, &[("a.wav", 0.0, false), ("b.wav", 0.0, false)], false))
            .unwrap()
            .mix(48000)
            .unwrap();
        let mut single = cab_ir(2000, 10);
        single.resize(mixed.len(), 0.0);
        let doubled: Vec<f32> = single.iter().map(|x| 2.0 * x).collect();
        assert!(max_diff(&mixed, &doubled) > 0.1, "sans alignement, la somme diffère");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn inverted_copy_cancels() {
        let d = dir("inv");
        write(&d, "a.wav", &cab_ir(1000, 0), 48000);
        let mixed = BlendSpec::load(&spec(&d, &[("a.wav", 0.0, false), ("a.wav", 0.0, true)], false))
            .unwrap()
            .mix(48000)
            .unwrap();
        assert!(mixed.iter().all(|x| x.abs() < 1e-6));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn gains_are_applied() {
        let d = dir("gain");
        let x = cab_ir(1000, 0);
        write(&d, "a.wav", &x, 48000);
        let mixed = BlendSpec::load(&spec(&d, &[("a.wav", -6.0, false), ("a.wav", -6.0, false)], false))
            .unwrap()
            .mix(48000)
            .unwrap();
        let g = 2.0 * db_to_gain(-6.0);
        let expected: Vec<f32> = x.iter().map(|v| v * g).collect();
        assert!(max_diff(&mixed, &expected) < 1e-4);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn resampled_parts_match() {
        let d = dir("sr");
        let x = cab_ir(2000, 20);
        write(&d, "a48.wav", &x, 48000);
        let x44 = ir::resample(&x, 48000, 44100);
        write(&d, "a44.wav", &x44, 44100);
        let mixed = BlendSpec::load(&spec(&d, &[("a48.wav", 0.0, false), ("a44.wav", 0.0, false)], true))
            .unwrap()
            .mix(48000)
            .unwrap();
        let single =
            BlendSpec::load(&spec(&d, &[("a48.wav", 0.0, false)], true)).unwrap().mix(48000).unwrap();
        let doubled: Vec<f32> = single.iter().map(|v| 2.0 * v).collect();
        let peak = doubled.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(max_diff(&mixed, &doubled) < 0.05 * peak, "écart {}", max_diff(&mixed, &doubled));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn clear_errors() {
        let d = dir("err");
        let p = d.join("cabs/bad.cab.toml");
        std::fs::write(&p, "name = 3").unwrap();
        assert!(BlendSpec::load(&p).unwrap_err().to_string().contains("mélange invalide"));
        std::fs::write(&p, "name = \"vide\"\nir = []\n").unwrap();
        assert!(BlendSpec::load(&p).unwrap_err().to_string().contains("sans IR"));
        let missing = spec(&d, &[("absent.wav", 0.0, false)], true);
        assert!(BlendSpec::load(&missing)
            .unwrap()
            .mix(48000)
            .unwrap_err()
            .to_string()
            .contains("introuvable"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn prepare_ir_accepts_cab_files() {
        let d = dir("prep");
        write(&d, "a.wav", &cab_ir(3000, 5), 48000);
        let cab = spec(&d, &[("a.wav", 0.0, false)], true);
        let ir = ir::prepare_ir(&cab, 48000, &ir::IrSettings::default()).unwrap();
        assert!(!ir.is_empty() && ir.iter().all(|x| x.is_finite()));
        assert_eq!(display_name(&cab), "test");
        let _ = std::fs::remove_dir_all(d);
    }
}
