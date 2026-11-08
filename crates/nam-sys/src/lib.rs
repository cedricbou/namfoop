//! Liaison Rust vers NeuralAmpModelerCore (C++, MIT, https://github.com/sdatkinson/NeuralAmpModelerCore).
//!
//! Cycle de vie : [`Model::load`] puis [`Model::reset`] hors temps réel, puis [`Model::process`]
//! dans le callback audio (sans allocation).

use std::ffi::{c_char, c_int, CStr, CString};
use std::path::Path;

#[repr(C)]
struct NamModel {
    _private: [u8; 0],
}

extern "C" {
    fn nam_load(path: *const c_char, err: *mut c_char, err_len: c_int) -> *mut NamModel;
    fn nam_reset(m: *mut NamModel, sr: f64, max_block: c_int, err: *mut c_char, err_len: c_int) -> c_int;
    fn nam_process(m: *mut NamModel, input: *const f32, output: *mut f32, n: c_int);
    fn nam_expected_sample_rate(m: *const NamModel) -> f64;
    fn nam_loudness(m: *const NamModel, has: *mut c_int) -> f64;
    fn nam_set_slimmable_size(m: *mut NamModel, size: f64) -> c_int;
    fn nam_slimmable_breakpoints(m: *const NamModel, out: *mut f64, cap: c_int) -> c_int;
    fn nam_free(m: *mut NamModel);
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("chemin invalide : {0}")]
    Path(String),
    #[error("chargement NAM : {0}")]
    Load(String),
    #[error("reset NAM : {0}")]
    Reset(String),
    #[error("modèle non préparé (appeler reset)")]
    NotReset,
}

pub struct Model {
    ptr: *mut NamModel,
    max_block: usize,
}

// Le modèle C++ n'a pas d'état partagé : il peut être déplacé vers le thread audio.
unsafe impl Send for Model {}

fn err_buf() -> Vec<u8> {
    vec![0u8; 512]
}

fn err_string(buf: &[u8]) -> String {
    CStr::from_bytes_until_nul(buf).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Model {
    /// Charge un fichier `.nam` (ou `.wav`, vu comme un modèle linéaire).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let p = path.as_ref();
        let c = CString::new(p.to_str().ok_or_else(|| Error::Path(p.display().to_string()))?)
            .map_err(|_| Error::Path(p.display().to_string()))?;
        let mut err = err_buf();
        let ptr = unsafe { nam_load(c.as_ptr(), err.as_mut_ptr().cast(), err.len() as c_int) };
        if ptr.is_null() {
            return Err(Error::Load(format!("{} : {}", p.display(), err_string(&err))));
        }
        Ok(Self { ptr, max_block: 0 })
    }

    /// Alloue les buffers et fait le prewarm. À appeler hors temps réel, avant `process`.
    pub fn reset(&mut self, sample_rate: f64, max_block: usize) -> Result<(), Error> {
        let mut err = err_buf();
        let rc = unsafe {
            nam_reset(self.ptr, sample_rate, max_block as c_int, err.as_mut_ptr().cast(), err.len() as c_int)
        };
        if rc != 0 {
            return Err(Error::Reset(err_string(&err)));
        }
        self.max_block = max_block;
        Ok(())
    }

    /// Traitement mono. Temps réel. Les blocs plus grands que `max_block` sont découpés.
    #[inline]
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), output.len());
        assert!(self.max_block > 0, "Model::process avant reset");
        for (i, o) in input.chunks(self.max_block).zip(output.chunks_mut(self.max_block)) {
            unsafe { nam_process(self.ptr, i.as_ptr(), o.as_mut_ptr(), i.len() as c_int) };
        }
    }

    pub fn expected_sample_rate(&self) -> f64 {
        unsafe { nam_expected_sample_rate(self.ptr) }
    }

    /// Loudness de sortie en dB pour une entrée typique, si le modèle la fournit.
    pub fn loudness_db(&self) -> Option<f64> {
        let mut has = 0;
        let v = unsafe { nam_loudness(self.ptr, &mut has) };
        (has != 0).then_some(v)
    }

    /// Choisit la taille d'un modèle « slimmable » (0.0 léger .. 1.0 complet).
    /// Hors temps réel. Renvoie false si le modèle n'est pas slimmable.
    pub fn set_slimmable_size(&mut self, size: f64) -> bool {
        unsafe { nam_set_slimmable_size(self.ptr, size.clamp(0.0, 1.0)) == 0 }
    }

    /// Seuils internes entre les tailles disponibles ; `None` si non slimmable.
    pub fn slimmable_breakpoints(&self) -> Option<Vec<f64>> {
        let n = unsafe { nam_slimmable_breakpoints(self.ptr, std::ptr::null_mut(), 0) };
        if n < 0 {
            return None;
        }
        let mut v = vec![0.0; n as usize];
        unsafe { nam_slimmable_breakpoints(self.ptr, v.as_mut_ptr(), n) };
        Some(v)
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe { nam_free(self.ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/NeuralAmpModelerCore/example_models").join(name)
    }

    fn run(model: &mut Model, n: usize) -> Vec<f32> {
        let input: Vec<f32> =
            (0..n).map(|i| 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 110.0 / 48000.0).sin()).collect();
        let mut out = vec![0.0; n];
        for (i, o) in input.chunks(64).zip(out.chunks_mut(64)) {
            model.process(i, o);
        }
        out
    }

    #[test]
    fn loads_and_processes_examples() {
        for name in ["wavenet.nam", "lstm.nam", "A2.nam", "slimmable_container.nam"] {
            let mut m = Model::load(example(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            m.reset(48000.0, 64).unwrap();
            let out = run(&mut m, 4800);
            assert!(out.iter().all(|x| x.is_finite()), "{name}: NaN/inf");
            let rms = (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt();
            assert!(rms > 1e-4, "{name}: sortie silencieuse ({rms})");
        }
    }

    #[test]
    fn slimmable_container_switches_size() {
        let mut m = Model::load(example("slimmable_container.nam")).unwrap();
        let bp = m.slimmable_breakpoints().expect("slimmable");
        assert!(!bp.is_empty());
        assert!(m.set_slimmable_size(0.0));
        m.reset(48000.0, 64).unwrap();
        let small = run(&mut m, 2400);
        assert!(m.set_slimmable_size(1.0));
        m.reset(48000.0, 64).unwrap();
        let big = run(&mut m, 2400);
        assert_ne!(small, big);
    }

    #[test]
    fn bad_file_is_an_error() {
        assert!(Model::load("/nonexistent.nam").is_err());
    }

    #[test]
    fn oversize_block_is_chunked() {
        let mut m = Model::load(example("wavenet.nam")).unwrap();
        m.reset(48000.0, 32).unwrap();
        let input = vec![0.1f32; 100];
        let mut out = vec![0.0f32; 100];
        m.process(&input, &mut out);
        assert!(out.iter().all(|x| x.is_finite()));
    }
}
