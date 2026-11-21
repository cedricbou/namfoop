//! Routage temps réel : entrée basse -> chaîne -> sorties selon leur rôle (traité, DI, micro, muet).
//! Configuré par `namfoop setup` ([routing] de la config) ou les options --bass-in / --mic-in / --route.
//! Canaux numérotés à partir de 1 dans la config et la CLI, à partir de 0 en interne.

use crate::error::{bail, Result};
use crate::metronome::{Metronome, MetronomeConfig};
use audio_io::Io;

pub use model::routing::{OutputRoute, Routing, Source};

/// Sorties par défaut : 1 et 2 = basse traitée.
pub fn default_outputs(n_out: usize) -> Vec<OutputRoute> {
    (1..=n_out.min(2))
        .map(|channel| OutputRoute { channel, source: Source::Processed, click: false })
        .collect()
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
    /// Sorties qui reçoivent **aussi** le clic du métronome, **ajouté** à leur source (ADR 0008).
    click_outs: Vec<usize>,
    /// Le générateur de clic et son tampon. `None` : aucune sortie ne l'a demandé, donc rien n'est
    /// même calculé.
    metronome: Option<(Metronome, Vec<f32>)>,
    scratch: Vec<f32>,
}

impl Router {
    /// Prépare le routage (hors temps réel). Les numéros de canaux sont vérifiés par
    /// [`Router::validate`] une fois la carte ouverte.
    pub fn new(r: &Routing, max_block: usize) -> Result<Self> {
        Self::with_metronome(r, max_block, None, 48000)
    }

    /// Le même, plus le **clic du métronome** : il est ajouté aux sorties qui portent
    /// `click = true` (ADR 0008). Sans sortie qui le demande, le générateur n'est même pas créé —
    /// un clic ne peut donc pas fuir par accident.
    pub fn with_metronome(
        r: &Routing,
        max_block: usize,
        metronome: Option<MetronomeConfig>,
        sample_rate: u32,
    ) -> Result<Self> {
        if r.bass_in == 0 || r.mic_in == Some(0) || r.outputs.iter().any(|o| o.channel == 0) {
            bail!("les canaux sont numérotés à partir de 1");
        }
        let outs = if r.outputs.is_empty() { default_outputs(2) } else { r.outputs.clone() };
        if let Some(o) = outs.iter().find(|o| o.source == Source::Mic && r.mic_in.is_none()) {
            bail!("sortie {} = micro, mais aucune entrée micro n'est définie (--mic-in)", o.channel);
        }
        let n = max_block.max(64);
        let click_outs: Vec<usize> = outs.iter().filter(|o| o.click).map(|o| o.channel - 1).collect();
        let metronome = metronome
            .filter(|_| !click_outs.is_empty())
            .map(|cfg| (Metronome::new(cfg, sample_rate), vec![0.0; n]));
        Ok(Self {
            bass: r.bass_in - 1,
            mic: r.mic_in.map(|m| m - 1),
            routes: outs.iter().map(|o| (o.channel - 1, o.source)).collect(),
            click_outs,
            metronome,
            scratch: vec![0.0; n],
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
            // le clic, **ajouté** aux seules sorties qui l'ont demandé : jamais substitué à leur
            // source (les in-ears veulent la basse *et* le clic), jamais ailleurs
            if let Some((m, buf)) = self.metronome.as_mut() {
                let click = &mut buf[..len];
                if m.render(click) {
                    for &c in &self.click_outs {
                        if c >= n_out {
                            continue;
                        }
                        let out = &mut io.output(c)[range.clone()];
                        for (o, k) in out.iter_mut().zip(click.iter()) {
                            *o += *k;
                        }
                    }
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
                OutputRoute { channel: 1, source: Source::Processed, click: false },
                OutputRoute { channel: 2, source: Source::Di, click: false },
                OutputRoute { channel: 3, source: Source::Mic, click: false },
                OutputRoute { channel: 4, source: Source::Mute, click: false },
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
            outputs: vec![OutputRoute { channel: 5, source: Source::Di, click: false }],
            ..Default::default()
        })
        .is_err());
        assert!(v(Routing {
            outputs: vec![OutputRoute { channel: 1, source: Source::Mic, click: false }],
            ..Default::default()
        })
        .is_err());
        assert!(v(Routing::default()).is_ok());
    }

    #[test]
    fn invalid_channels_never_panic_in_process() {
        let r = Routing {
            bass_in: 9,
            outputs: vec![OutputRoute { channel: 7, source: Source::Di, click: false }],
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
        let r = Routing {
            outputs: vec![OutputRoute { channel: 2, source: Source::Di, click: false }],
            ..Default::default()
        };
        let (ins, mut outs) = io_buffers(2, 2, 256);
        let mut router = Router::new(&r, 64).unwrap();
        assert_no_alloc::assert_no_alloc(|| {
            router.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 256 }, |b| b.fill(0.5));
        });
    }
}

#[cfg(test)]
mod click_tests {
    use super::*;
    use audio_io::VecIo;
    use dsp::param::Control;

    const SR: u32 = 48000;

    fn routing(click_on: &[usize]) -> Routing {
        Routing {
            bass_in: 1,
            mic_in: None,
            outputs: (1..=4)
                .map(|channel| OutputRoute {
                    channel,
                    source: Source::Processed,
                    click: click_on.contains(&channel),
                })
                .collect(),
        }
    }

    fn cfg(gain: f32) -> MetronomeConfig {
        MetronomeConfig {
            gain: Control::new(gain),
            tempo: Control::new(120.0),
            downbeat: Control::new(0.0),
            beats_per_bar: 4,
            sound: crate::metronome::ClickSound::default(),
            clock_ticks: None,
            clock_phase: Control::new(f32::NAN),
        }
    }

    /// Rend les quatre sorties après `frames` échantillons de silence en entrée.
    fn run(r: &Routing, m: Option<MetronomeConfig>, frames: usize) -> Vec<Vec<f32>> {
        let mut router = Router::with_metronome(r, 64, m, SR).expect("routage");
        let ins = vec![vec![0.0f32; frames]];
        let mut outs = vec![vec![0.0f32; frames]; 4];
        router.process(&mut VecIo { ins: &ins, outs: &mut outs, frames }, |_| {});
        outs
    }

    /// **La garantie qui compte** : une sortie qui n'a pas demandé le clic n'en reçoit **rien**,
    /// métronome allumé ou non. Un clic parti dans le sondier en concert est une catastrophe.
    #[test]
    fn only_the_outputs_that_asked_for_it_get_the_click() {
        // sorties 3 et 4 : le clic ; 1 et 2 (le sondier) : rien
        let outs = run(&routing(&[3, 4]), Some(cfg(0.5)), SR as usize);
        assert!(outs[0].iter().all(|x| *x == 0.0), "sortie 1 : le clic a fui");
        assert!(outs[1].iter().all(|x| *x == 0.0), "sortie 2 : le clic a fui");
        for c in [2, 3] {
            let peak = outs[c].iter().fold(0.0f32, |a, x| a.max(x.abs()));
            assert!((0.4..=0.5).contains(&peak), "sortie {} : crête {peak}", c + 1);
        }
    }

    /// Aucune sortie ne le demande : le générateur n'est même pas créé, et la sortie est identique
    /// au bit près à ce qu'elle serait sans métronome.
    #[test]
    fn no_output_asking_means_no_click_at_all() {
        let with = run(&routing(&[]), Some(cfg(1.0)), 4096);
        let without = run(&routing(&[]), None, 4096);
        assert_eq!(with, without, "sans sortie demandeuse, rien ne change");
        assert!(with.iter().flatten().all(|x| *x == 0.0));
    }

    /// Gain à zéro : éteint, et la sortie est identique au bit près.
    #[test]
    fn a_zero_gain_is_bit_identical() {
        let off = run(&routing(&[3]), Some(cfg(0.0)), 4096);
        let none = run(&routing(&[3]), None, 4096);
        assert_eq!(off, none, "éteint, rien n'est ajouté");
    }

    /// Le clic s'**ajoute** à la source, il ne la remplace pas : les in-ears veulent la basse
    /// **et** le clic.
    #[test]
    fn the_click_is_added_to_the_source_not_substituted() {
        let r = routing(&[1]);
        let mut router = Router::with_metronome(&r, 64, Some(cfg(0.25)), SR).expect("routage");
        let frames = SR as usize / 2;
        let ins = vec![vec![0.0f32; frames]];
        let mut outs = vec![vec![0.0f32; frames]; 4];
        // la « chaîne » rend un continu à 0,5 : on doit retrouver 0,5 plus le clic
        router.process(&mut VecIo { ins: &ins, outs: &mut outs, frames }, |b| b.fill(0.5));
        // le clic oscille autour de zéro : la sortie s'écarte de 0,5 dans les deux sens, mais
        // jamais au point de perdre la source (gain 0,25)
        let lo = outs[0].iter().fold(f32::MAX, |a, x| a.min(*x));
        let hi = outs[0].iter().fold(f32::MIN, |a, x| a.max(*x));
        assert!(lo > 0.5 - 0.26, "la source a été remplacée : minimum {lo}");
        assert!(hi > 0.6, "le clic ne s'est pas ajouté : maximum {hi}");
        // loin d'une salve, la sortie est **exactement** la source
        assert_eq!(outs[0][SR as usize / 4], 0.5, "entre deux clics, rien n'est ajouté");
        // les autres sorties n'ont que la source
        assert!(outs[1].iter().all(|x| (*x - 0.5).abs() < 1e-6));
    }

    /// Dans le callback : aucune allocation, clic compris.
    #[test]
    fn the_click_does_not_allocate() {
        let r = routing(&[2, 3]);
        let mut router = Router::with_metronome(&r, 64, Some(cfg(0.5)), SR).expect("routage");
        let ins = vec![vec![0.1f32; 256]];
        let mut outs = vec![vec![0.0f32; 256]; 4];
        assert_no_alloc::assert_no_alloc(|| {
            for _ in 0..40 {
                router.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 256 }, |b| b.fill(0.2));
            }
        });
        assert!(outs.iter().flatten().all(|x| x.is_finite()));
    }
}
