//! **Une seule horloge** : quand plusieurs appareils envoient des tops, un seul est suivi. Deux
//! horloges additionnées doubleraient le tempo mesuré et décaleraient la grille du clic pour de bon.
//!
//! La règle (ADR 0008 pour le délai de perte) : un appareil **imposé** par son nom, et lui seul ;
//! sinon **le premier qui bat**, gardé tant qu'il bat — 500 ms sans top, il est perdu et le
//! suivant qui bat prend la main. Le transport (`Start`, `Stop`, `Continue`, la position) suit la
//! même règle : il appartient à l'appareil de l'horloge, et passe tant qu'aucune horloge n'est
//! suivie (Ableton envoie `Start` avant son premier top).
//!
//! Machine pure, horloge fournie : elle tourne dans le fil de lecture, et se teste sans câble.

use std::time::{Duration, Instant};

/// Sans top pendant ce délai, l'appareil de l'horloge est perdu (ADR 0008).
pub(crate) const LOST: Duration = Duration::from_millis(500);

/// L'appareil dont l'horloge est suivie.
#[derive(Debug, Default)]
pub(crate) struct ClockChoice {
    /// Un morceau de nom imposé (`Settings::clock_from`), insensible à la casse.
    forced: Option<String>,
    /// L'appareil suivi, et l'instant de son dernier top.
    current: Option<(u16, Instant)>,
}

impl ClockChoice {
    pub(crate) fn new(forced: Option<&str>) -> Self {
        Self { forced: forced.map(str::to_lowercase), current: None }
    }

    /// Un top de l'appareil `device` (nommé `name`) à l'instant `now` : vrai s'il est compté.
    pub(crate) fn tick(&mut self, device: u16, name: &str, now: Instant) -> bool {
        if let Some(f) = &self.forced {
            let ok = name.to_lowercase().contains(f.as_str());
            if ok {
                self.current = Some((device, now));
            }
            return ok;
        }
        match self.current {
            Some((d, _)) if d == device => {}
            Some((_, last)) if now.saturating_duration_since(last) < LOST => return false,
            _ => {}
        }
        self.current = Some((device, now));
        true
    }

    /// Un message de transport de `device` : vrai s'il passe.
    pub(crate) fn transport(&self, device: u16, name: &str, now: Instant) -> bool {
        if let Some(f) = &self.forced {
            return name.to_lowercase().contains(f.as_str());
        }
        match self.current {
            Some((d, last)) => d == device || now.saturating_duration_since(last) >= LOST,
            None => true,
        }
    }

    /// L'appareil suivi à l'instant `now`, s'il bat encore.
    #[cfg(test)]
    pub(crate) fn following(&self, now: Instant) -> Option<u16> {
        self.current.filter(|(_, last)| now.saturating_duration_since(*last) < LOST).map(|(d, _)| d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    #[test]
    fn le_premier_qui_bat_garde_la_main_tant_qu_il_bat() {
        let t0 = Instant::now();
        let mut c = ClockChoice::new(None);
        assert!(c.tick(0, "UM-ONE", ms(t0, 0)), "le premier qui bat");
        assert!(!c.tick(1, "MC6 Pro", ms(t0, 10)), "un second est ignoré");
        assert!(c.tick(0, "UM-ONE", ms(t0, 20)));
        assert!(!c.tick(1, "MC6 Pro", ms(t0, 400)), "UM-ONE a battu il y a 380 ms : il garde la main");
        assert_eq!(c.following(ms(t0, 400)), Some(0));
        // UM-ONE se tait : 500 ms après son dernier top, le suivant prend la main
        assert!(c.tick(1, "MC6 Pro", ms(t0, 520)));
        assert!(!c.tick(0, "UM-ONE", ms(t0, 530)), "revenu, UM-ONE attend son tour");
        assert_eq!(c.following(ms(t0, 530)), Some(1));
        assert_eq!(c.following(ms(t0, 2000)), None, "plus personne ne bat");
    }

    #[test]
    fn un_appareil_impose_est_le_seul_compte() {
        let t0 = Instant::now();
        let mut c = ClockChoice::new(Some("um-one"));
        assert!(!c.tick(1, "MC6 Pro", ms(t0, 0)), "même premier, un autre n'est pas compté");
        assert!(c.tick(0, "UM-ONE MIDI 1", ms(t0, 10)), "le nom imposé, insensible à la casse");
        assert!(!c.tick(1, "MC6 Pro", ms(t0, 900)), "même muet, l'imposé ne cède pas sa place");
        assert!(!c.transport(1, "MC6 Pro", ms(t0, 900)));
        assert!(c.transport(0, "UM-ONE MIDI 1", ms(t0, 900)));
    }

    #[test]
    fn le_transport_suit_l_appareil_de_l_horloge() {
        let t0 = Instant::now();
        let mut c = ClockChoice::new(None);
        assert!(
            c.transport(1, "MC6 Pro", ms(t0, 0)),
            "aucune horloge : Start passe (Ableton l'envoie d'abord)"
        );
        assert!(c.tick(0, "UM-ONE", ms(t0, 5)));
        assert!(c.transport(0, "UM-ONE", ms(t0, 10)));
        assert!(!c.transport(1, "MC6 Pro", ms(t0, 10)), "le Stop d'un autre appareil ne nous arrête pas");
        assert!(c.transport(1, "MC6 Pro", ms(t0, 600)), "horloge perdue : le transport est libre");
    }
}
