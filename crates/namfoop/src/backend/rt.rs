//! Réglages du thread temps réel : priorité SCHED_FIFO et flush-to-zero des dénormaux.

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
