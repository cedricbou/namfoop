//! Réglages du thread temps réel : priorité SCHED_FIFO, flush-to-zero des dénormaux, cœur
//! dédié et pile préchargée (#3).

/// Active FTZ/DAZ : les dénormaux font exploser le coût CPU des filtres IIR / réverbes.
pub fn disable_denormals() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        #[allow(deprecated)]
        use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
        #[allow(deprecated)]
        _mm_setcsr(_mm_getcsr() | 0x8040); // FTZ (bit 15) | DAZ (bit 6)
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let mut fpcr: u64;
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
        fpcr |= 1 << 24; // FZ
        std::arch::asm!("msr fpcr, {}", in(reg) fpcr);
    }
}

/// Passe le thread courant en SCHED_FIFO. Essaie d'abord directement (groupe `realtime`
/// ou root), puis via rtkit (D-Bus) si dispo. Renvoie la priorité obtenue.
pub fn promote_current_thread(priority: i32) -> Option<i32> {
    unsafe {
        let param = libc::sched_param { sched_priority: priority };
        if libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &param) == 0 {
            return Some(priority);
        }
    }
    via_rtkit(priority.min(20))
}

fn via_rtkit(priority: i32) -> Option<i32> {
    // rtkit exige une limite RLIMIT_RTTIME finie sur le processus.
    unsafe {
        let lim = libc::rlimit { rlim_cur: 200_000, rlim_max: 200_000 }; // 200 ms en µs
        libc::setrlimit(libc::RLIMIT_RTTIME, &lim);
    }
    let tid = unsafe { libc::gettid() };
    let pid = std::process::id();
    let ok = std::process::Command::new("busctl")
        .args([
            "--system",
            "call",
            "org.freedesktop.RealtimeKit1",
            "/org/freedesktop/RealtimeKit1",
            "org.freedesktop.RealtimeKit1",
            "MakeThreadRealtimeWithPID",
            "ttu",
            &pid.to_string(),
            &tid.to_string(),
            &priority.to_string(),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    ok.then_some(priority)
}

/// Épingle le thread courant sur le cœur `cpu` (prépare `isolcpus` sur le Pi). À appeler au
/// démarrage du thread audio, avant le régime établi.
pub fn pin_current_thread(cpu: usize) -> bool {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_SET(cpu, &mut set);
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set) == 0
    }
}

/// Touche 256 Kio de pile : les pages sont présentes avant le premier callback (pas de défaut
/// de page en jouant ; verrouillées si `mlockall` est actif).
#[inline(never)]
pub fn prefault_stack() {
    let mut buf = [0u8; 256 * 1024];
    for i in (0..buf.len()).step_by(4096) {
        buf[i] = 1;
    }
    std::hint::black_box(&buf);
}

/// Réglages communs au démarrage d'un thread audio que namfoop crée lui-même (ALSA, sim).
pub fn setup_audio_thread(cpu: Option<usize>) {
    if let Some(c) = cpu {
        if pin_current_thread(c) {
            log::info!("thread audio épinglé sur le cœur {c}");
        } else {
            log::warn!("impossible d'épingler le thread audio sur le cœur {c}");
        }
    }
    prefault_stack();
}
