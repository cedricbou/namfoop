//! Routage temps réel : entrée basse -> chaîne -> sorties selon leur rôle (traité, DI, micro, muet).
//! Configuré par `namfoop setup` ([routing] de la config) ou les options --bass-in / --mic-in / --route.
//! Canaux numérotés à partir de 1 dans la config et la CLI, à partir de 0 en interne.

use crate::error::{bail, Result};
use audio_io::Io;

pub use model::routing::{OutputRoute, Routing, Source};

/// Sorties par défaut : 1 et 2 = basse traitée.
pub fn default_outputs(n_out: usize) -> Vec<OutputRoute> {
    (1..=n_out.min(2)).map(|channel| OutputRoute { channel, source: Source::Processed }).collect()
}

#[derive(Default, Clone, Copy, Debug)]
pub struct RouteStats {
    pub in_peak: f32,
    pub out_peak: f32,
    /// Échantillons d'entrée à pleine échelle (ADC saturé).
    pub in_clips: u32,
    /// Échantillons traités au-delà de 0 dBFS.
    pub out_clips: u32,
}

/// Seuil d'écrêtage d'entrée : au-delà, l'ADC est (quasi) saturé.
pub const IN_CLIP: f32 = 0.999;

pub struct Router {
    bass: usize,
    mic: Option<usize>,
    routes: Vec<(usize, Source)>,
    scratch: Vec<f32>,
}

impl Router {
    /// Prépare le routage (hors temps réel). Les numéros de canaux sont vérifiés par
    /// [`Router::validate`] une fois la carte ouverte.
    pub fn new(r: &Routing, max_block: usize) -> Result<Self> {
        if r.bass_in == 0 || r.mic_in == Some(0) || r.outputs.iter().any(|o| o.channel == 0) {
            bail!("les canaux sont numérotés à partir de 1");
        }
        let outs = if r.outputs.is_empty() { default_outputs(2) } else { r.outputs.clone() };
        if let Some(o) = outs.iter().find(|o| o.source == Source::Mic && r.mic_in.is_none()) {
            bail!("sortie {} = micro, mais aucune entrée micro n'est définie (--mic-in)", o.channel);
        }
        Ok(Self {
            bass: r.bass_in - 1,
            mic: r.mic_in.map(|m| m - 1),
            routes: outs.iter().map(|o| (o.channel - 1, o.source)).collect(),
            scratch: vec![0.0; max_block.max(64)],
        })
    }

    /// Vérifie le routage pour une carte à `n_in` entrées / `n_out` sorties.
    pub fn validate(&self, n_in: usize, n_out: usize) -> Result<()> {
        let check_in = |c: usize, what: &str| -> Result<()> {
            if c >= n_in {
                bail!("{what} : entrée {} inexistante (la carte a {n_in} entrée(s))", c + 1);
            }
            Ok(())
        };
        check_in(self.bass, "basse")?;
        if let Some(m) = self.mic {
            check_in(m, "micro")?;
        }
        if let Some((c, _)) = self.routes.iter().find(|(c, _)| *c >= n_out) {
            bail!("sortie {} inexistante (la carte a {n_out} sortie(s))", c + 1);
        }
        Ok(())
    }

    /// Entrée de la basse (à partir de 0).
    pub fn bass(&self) -> usize {
        self.bass
    }

    pub fn describe(&self) -> String {
        let mut s = format!("basse = entrée {}", self.bass + 1);
        if let Some(m) = self.mic {
            s += &format!(", micro = entrée {}", m + 1);
        }
        for (c, src) in &self.routes {
            s += &format!(", sortie {} = {}", c + 1, src.label());
        }
        s
    }

    /// Temps réel : aucune allocation. `chain` traite la basse sur place.
    /// Par sécurité, un canal hors limites est ignoré (jamais de panic dans le thread audio).
    pub fn process(&mut self, io: &mut dyn Io, mut chain: impl FnMut(&mut [f32])) -> RouteStats {
        let frames = io.frames();
        let (n_in, n_out) = (io.n_in(), io.n_out());
        let mut st = RouteStats::default();
        for c in 0..n_out {
            if !self.routes.iter().any(|(r, _)| *r == c) {
                io.output(c).fill(0.0);
            }
        }
        if self.bass >= n_in {
            io.silence();
            return st;
        }
        let max = self.scratch.len();
        let mut start = 0;
        while start < frames {
            let len = (frames - start).min(max);
            let range = start..start + len;
            let buf = &mut self.scratch[..len];
            buf.copy_from_slice(&io.input(self.bass)[range.clone()]);
            for x in buf.iter() {
                let a = x.abs();
                st.in_peak = st.in_peak.max(a);
                st.in_clips += (a >= IN_CLIP) as u32;
            }
            chain(buf);
            for x in buf.iter() {
                let a = x.abs();
                st.out_peak = st.out_peak.max(a);
                st.out_clips += (a > 1.0) as u32;
            }
            for &(c, src) in &self.routes {
                if c >= n_out {
                    continue;
                }
                match src {
                    Source::Processed => io.output(c)[range.clone()].copy_from_slice(buf),
                    Source::Di => {
                        let (i, o) = io.in_out(self.bass, c);
                        o[range.clone()].copy_from_slice(&i[range.clone()]);
                    }
                    Source::Mic => match self.mic {
                        Some(m) if m < n_in => {
                            let (i, o) = io.in_out(m, c);
                            o[range.clone()].copy_from_slice(&i[range.clone()]);
                        }
                        _ => io.output(c)[range.clone()].fill(0.0),
                    },
                    Source::Mute => io.output(c)[range.clone()].fill(0.0),
                }
            }
            start += len;
        }
        st
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_io::VecIo;

    fn io_buffers(n_in: usize, n_out: usize, frames: usize) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let ins = (0..n_in).map(|c| vec![0.1 * (c + 1) as f32; frames]).collect();
        let outs = vec![vec![9.0f32; frames]; n_out]; // valeurs parasites : doivent être écrasées
        (ins, outs)
    }

    #[test]
    fn routes_each_source_to_its_output() {
        let r = Routing {
            bass_in: 2,
            mic_in: Some(3),
            outputs: vec![
                OutputRoute { channel: 1, source: Source::Processed },
                OutputRoute { channel: 2, source: Source::Di },
                OutputRoute { channel: 3, source: Source::Mic },
                OutputRoute { channel: 4, source: Source::Mute },
            ],
        };
        let (ins, mut outs) = io_buffers(3, 5, 100);
        let mut router = Router::new(&r, 64).unwrap(); // 100 > 64 : découpage
        router.validate(3, 5).unwrap();
        let st = router.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 100 }, |b| {
            b.iter_mut().for_each(|x| *x *= 2.0)
        });
        assert!(outs[0].iter().all(|x| (*x - 0.4).abs() < 1e-6), "traité = basse (0.2) × 2");
        assert!(outs[1].iter().all(|x| (*x - 0.2).abs() < 1e-6), "DI = basse");
        assert!(outs[2].iter().all(|x| (*x - 0.3).abs() < 1e-6), "micro");
        assert!(outs[3].iter().all(|x| *x == 0.0), "muet");
        assert!(outs[4].iter().all(|x| *x == 0.0), "non routée = silence");
        assert!((st.in_peak - 0.2).abs() < 1e-6 && (st.out_peak - 0.4).abs() < 1e-6);
    }

    #[test]
    fn default_routing_is_processed_on_1_and_2() {
        let (ins, mut outs) = io_buffers(2, 3, 32);
        let mut router = Router::new(&Routing::default(), 64).unwrap();
        router.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 32 }, |_| {});
        assert!(outs[0].iter().chain(&outs[1]).all(|x| (*x - 0.1).abs() < 1e-6));
        assert!(outs[2].iter().all(|x| *x == 0.0));
    }

    #[test]
    fn rejects_out_of_range_channels() {
        let v = |r: Routing| Router::new(&r, 64).and_then(|x| x.validate(2, 2));
        assert!(v(Routing { bass_in: 3, ..Default::default() }).is_err());
        assert!(v(Routing { bass_in: 0, ..Default::default() }).is_err());
        assert!(v(Routing {
            outputs: vec![OutputRoute { channel: 5, source: Source::Di }],
            ..Default::default()
        })
        .is_err());
        assert!(v(Routing {
            outputs: vec![OutputRoute { channel: 1, source: Source::Mic }],
            ..Default::default()
        })
        .is_err());
        assert!(v(Routing::default()).is_ok());
    }

    #[test]
    fn invalid_channels_never_panic_in_process() {
        let r = Routing {
            bass_in: 9,
            outputs: vec![OutputRoute { channel: 7, source: Source::Di }],
            ..Default::default()
        };
        let (ins, mut outs) = io_buffers(2, 2, 32);
        let mut router = Router::new(&r, 64).unwrap();
        router.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 32 }, |_| {});
        assert!(outs.iter().flatten().all(|x| *x == 0.0));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn process_does_not_allocate() {
        let r =
            Routing { outputs: vec![OutputRoute { channel: 2, source: Source::Di }], ..Default::default() };
        let (ins, mut outs) = io_buffers(2, 2, 256);
        let mut router = Router::new(&r, 64).unwrap();
        assert_no_alloc::assert_no_alloc(|| {
            router.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 256 }, |b| b.fill(0.5));
        });
    }
}
