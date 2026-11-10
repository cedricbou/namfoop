//! Réglages temps réel du processus, appliqués une fois par `Engine::start` et `raw::start` (#3) :
//! mémoire verrouillée, avertissement sur le gouverneur CPU, préchauffage des chaînes. Hors temps
//! réel ; jamais d'échec (un avertissement clair à la place).

use std::sync::Once;

static PROCESS: Once = Once::new();

/// Mémoire verrouillée et gouverneur : une seule fois par processus.
pub fn prepare_process(cpu: Option<usize>) {
    PROCESS.call_once(|| {
        if flush_supported() {
            log::info!("dénormalisés : flush-to-zero actif");
        } else {
            log::info!("dénormalisés : flush-to-zero non pris en charge sur cette architecture");
        }
        lock_memory();
        check_governor(cpu);
    });
}

/// Le processeur sait-il ramener les dénormalisés à zéro ?
pub const fn flush_supported() -> bool {
    cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
}

/// Active le flush-to-zero dans le thread courant (réglage par thread : à appeler dans le thread
/// qui calcule). Quand un son s'éteint (queue d'IR, de delay, filtre après la dernière note), les
/// échantillons passent sous ~1e-38, en format dénormalisé : chaque opération y coûte des dizaines
/// de fois plus, la charge grimpe pendant les silences. FTZ + DAZ (x86_64, MXCSR bits 15 et 6) ou
/// FZ (aarch64, FPCR bit 24) les remplacent par zéro, inaudible. Une instruction : sans allocation,
/// utilisable dans le callback.
#[inline]
pub fn flush_denormals() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut csr: u32 = 0;
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack));
        csr |= 0x8040; // FTZ (bit 15) | DAZ (bit 6)
        std::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, readonly));
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let mut fpcr: u64;
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
        fpcr |= 1 << 24; // FZ
        std::arch::asm!("msr fpcr, {}", in(reg) fpcr);
    }
}

/// `mlockall` seulement si la limite `RLIMIT_MEMLOCK` est illimitée : avec une limite finie,
/// `MCL_FUTURE` ferait échouer des allocations plus tard (au changement de chaîne).
pub fn should_lock(limit: libc::rlim_t) -> bool {
    limit == libc::RLIM_INFINITY
}

fn lock_memory() {
    let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    if unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut lim) } != 0 {
        return;
    }
    if !should_lock(lim.rlim_cur) {
        log::warn!(
            "mémoire non verrouillée (limite memlock {} Kio) : un défaut de page peut faire un pic ; \
             autoriser « memlock unlimited » (voir le guide, « Temps réel »)",
            lim.rlim_cur / 1024
        );
        return;
    }
    if unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE) } == 0 {
        log::info!("mémoire verrouillée (mlockall)");
    } else {
        log::warn!("mlockall refusé : {}", std::io::Error::last_os_error());
    }
}

/// Gouverneurs qui laissent la fréquence descendre entre deux buffers.
pub fn slow_governor(g: &str) -> bool {
    g.trim() != "performance"
}

fn check_governor(cpu: Option<usize>) {
    let path = |c: usize| format!("/sys/devices/system/cpu/cpu{c}/cpufreq/scaling_governor");
    let cpus: Vec<usize> = match cpu {
        Some(c) => vec![c],
        None => (0..std::thread::available_parallelism().map_or(1, |n| n.get())).collect(),
    };
    let slow: Vec<String> = cpus
        .iter()
        .filter_map(|c| std::fs::read_to_string(path(*c)).ok())
        .filter(|g| slow_governor(g))
        .map(|g| g.trim().to_string())
        .collect();
    if let Some(g) = slow.first() {
        log::warn!(
            "gouverneur CPU « {g} » : la fréquence baisse entre deux buffers, d'où des pics de charge ; \
             pour jouer : sudo cpupower frequency-set -g performance"
        );
    }
}

/// Fait jouer la chaîne hors temps réel au bloc réel avant sa mise en route : le premier passage
/// (pages, caches, allocations internes du C++ NAM) ne tombe pas sur le thread audio. Bruit
/// à -30 dBFS puis silence assez long pour que l'IR (≤ 200 ms) et le limiteur se vident.
pub fn prewarm(chain: &mut dsp::Chain, block: usize, sample_rate: u32) {
    if chain.is_empty() {
        return;
    }
    let block = block.max(1);
    let (noise, silence) = (sample_rate as usize / 4, sample_rate as usize / 2);
    let mut seed: u32 = 0x1234_5678;
    let mut buf = vec![0.0f32; block];
    let mut done = 0;
    while done < noise + silence {
        for x in buf.iter_mut() {
            *x = if done < noise {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.06
            } else {
                0.0
            };
        }
        chain.process(&mut buf);
        done += block;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dans un thread neuf : un dénormalisé existe avant, plus après.
    #[test]
    fn flush_to_zero_in_a_fresh_thread() {
        if !flush_supported() {
            return;
        }
        let (before, after) = std::thread::spawn(|| {
            let x = || std::hint::black_box(f32::MIN_POSITIVE) * std::hint::black_box(0.5);
            let before = x();
            flush_denormals();
            (before, x())
        })
        .join()
        .unwrap();
        assert!(before != 0.0 && before.is_subnormal(), "avant : {before:e}");
        assert_eq!(after, 0.0, "après");
    }

    #[test]
    fn decisions() {
        assert!(should_lock(libc::RLIM_INFINITY));
        assert!(!should_lock(8 * 1024 * 1024));
        assert!(
            slow_governor("powersave\n") && slow_governor("schedutil") && !slow_governor("performance\n")
        );
    }

    /// Après le préchauffage, la chaîne sonne comme une chaîne neuve (à la queue près).
    #[test]
    fn prewarmed_chain_sounds_like_a_fresh_one() {
        let model = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models/wavenet.nam");
        let spec = dsp::chain::AmpSpec { model: Some(model), ..Default::default() };
        let mut fresh = spec.build(48000, 1024).unwrap();
        let mut warm = spec.build(48000, 1024).unwrap();
        prewarm(&mut warm, 48, 48000);
        let signal: Vec<f32> = (0..48000)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 55.0 * i as f32 / 48000.0).sin())
            .collect();
        let (mut a, mut b) = (signal.clone(), signal);
        for (x, y) in a.chunks_mut(48).zip(b.chunks_mut(48)) {
            fresh.process(x);
            warm.process(y);
        }
        let err = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
        let peak = a.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(err < 1e-3 * peak, "écart {err} pour une crête {peak}");
    }
}
