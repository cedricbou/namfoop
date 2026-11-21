//! Ce qui tourne **dans le callback** : la détection du retour et l'enchaînement des passes.
//! Deux machines à états pures, sans allocation, sans verrou et sans panique possible, donc
//! testables sans carte son — c'est tout l'intérêt de les tenir à part.
//!
//! Le retour n'est **pas** cherché avec un seuil fixe : à travers PipeWire, l'impulsion d'un
//! échantillon passe par l'étage de volume et le convertisseur, qui l'étalent — sa crête peut
//! tomber très bas — à 85 % de volume de sortie, sous 0,05, soit -26 dBFS. Le critère est donc
//! **relatif** : la crête de la fenêtre d'écoute dit si quelque chose est revenu (comparée au bruit
//! de fond mesuré juste avant), et c'est le **début** de ce retour qui donne la latence — le
//! premier échantillon qui monte à [`ONSET_FRAC`] de cette crête.
//!
//! Chercher la **crête** elle-même rendrait la mesure bistable : un retour qui sonne a plusieurs
//! lobes de hauteur voisine, et l'instant du maximum saute de l'un à l'autre d'une passe à la
//! suivante (mesuré : 42 frames d'écart sur une sortie AUX, alors que chaque passe était stable).
//! Le début, lui, ne dépend pas de la forme de la réponse — et c'est aussi la définition de la
//! latence : le délai entre la note et le son.

use std::sync::Arc;
use std::time::Instant;

use engine::meters::LoadHistogram;

const AMPLITUDE: f32 = 0.7;
/// Le retour doit sortir d'au moins ça du bruit de fond (≈ +14 dB) pour être cru.
const MIN_RATIO: f32 = 5.0;
/// Bruit de fond plancher : évite de diviser par zéro sur une entrée parfaitement muette.
const NOISE_FLOOR: f32 = 1e-7;
/// Fraction de la crête de la fenêtre à laquelle le retour est jugé commencé (-12 dB).
const ONSET_FRAC: f32 = 0.25;
/// Fenêtres sans retour au bout desquelles une passe est abandonnée.
pub(super) const MAX_WEAK: u32 = 5;
/// Durée d'écoute après l'impulsion : elle borne la latence mesurable.
const WINDOW_S: f64 = 0.25;
/// Silence entre la fin d'une fenêtre et l'impulsion suivante.
const INTERVAL_S: f64 = 0.5;
/// Un cycle de mesure : la fenêtre d'écoute, puis l'intervalle.
pub(super) const CYCLE_S: f64 = WINDOW_S + INTERVAL_S;
/// Attente avant la **première** impulsion. Le flux met quelques secondes à s'établir : mesuré sur
/// une carte USB, l'aller-retour part de 269 frames, passe par 311 et ne se fixe à 305 qu'au bout
/// de 3 à 4 s. Une mesure prise pendant ce temps est fausse, et si c'est celle du trajet nu, c'est
/// la référence de tout le tableau qui est fausse.
pub(super) const SETTLE_FIRST_S: f64 = 3.0;
/// Attente après un changement de passe : la queue de la chaîne quittée (celle d'une réverbe) doit
/// s'éteindre avant qu'on mesure un bruit de fond.
pub(super) const SETTLE_PASS_S: f64 = 1.0;
/// Fenêtres émises **et affichées**, mais non comptées, en tête de chaque passe : elles absorbent
/// ce qui reste du transitoire, au démarrage comme au changement de chaîne. Les compter est le
/// piège — c'est ce qui faisait mentir la ligne témoin d'une chaîne.
pub(super) const WARMUP: u32 = 2;
/// Taille max de bloc pour laquelle les chaînes sont préparées (comme `run`). Le callback découpe
/// ce qu'il reçoit à cette taille, pour garder des tampons préalloués.
pub(super) const MAX_BLOCK: usize = 1024;
/// Longueur du bruit interne du mode « à côté » (1 s à 48 kHz), rejoué en boucle.
const NOISE_LEN: usize = 48_000;

#[derive(Debug, PartialEq)]
pub(super) enum Event {
    /// Retour trouvé : frames écoulées, crête et bruit de fond (linéaires).
    Measured { frames: u64, peak: f32, noise: f32 },
    /// Fenêtre écoulée sans rien qui sorte du bruit.
    TooWeak { peak: f32, noise: f32 },
}

/// Détection du retour, échantillon par échantillon (temps réel : aucune allocation).
///
/// Cycle : on accumule le bruit de fond, on émet l'impulsion, on écoute `window` frames en gardant
/// la plus grande valeur absolue et sa position, puis on conclut.
struct Detector {
    /// Durée d'écoute après l'impulsion (frames) : borne la latence mesurable.
    window: u64,
    /// Frames entre deux impulsions.
    interval: u64,
    next_emit: u64,
    /// Émission en cours : frame de l'impulsion, et nombre d'échantillons déjà écoutés.
    emitted: Option<(u64, usize)>,
    /// La fenêtre d'écoute, en valeurs absolues : `heard[j]` est la frame `impulsion + 1 + j`.
    /// Préallouée une fois pour toutes — la conclusion la relit, ce qu'un calcul au fil de l'eau ne
    /// permet pas : le seuil du début dépend de la crête, qu'on ne connaît qu'à la fin.
    heard: Vec<f32>,
    /// Bruit de fond : somme des carrés et nombre d'échantillons, avant l'impulsion.
    noise_sum: f64,
    noise_n: u32,
    /// Bruit de fond retenu pour la mesure en cours (RMS).
    noise: f32,
}

impl Detector {
    fn new(window: u64, interval: u64, first_emit: u64) -> Self {
        Self {
            window,
            interval,
            next_emit: first_emit,
            emitted: None,
            heard: vec![0.0; window.max(1) as usize],
            noise_sum: 0.0,
            noise_n: 0,
            noise: 0.0,
        }
    }

    /// Un échantillon d'entrée au temps `t`. Renvoie l'amplitude à émettre sur la sortie (0 sauf à
    /// l'instant de l'impulsion) et, quand la fenêtre se termine, le résultat de la mesure.
    #[inline]
    fn sample(&mut self, t: u64, x: f32) -> (f32, Option<Event>) {
        match self.emitted {
            // on écoute le retour : on enregistre la fenêtre, on conclut à la fin
            Some((at, n)) => {
                if let Some(slot) = self.heard.get_mut(n) {
                    *slot = x.abs();
                }
                let n = n + 1;
                if n as u64 >= self.window {
                    self.emitted = None;
                    self.next_emit = t + self.interval;
                    self.noise_sum = 0.0;
                    self.noise_n = 0;
                    return (0.0, Some(self.conclude()));
                }
                self.emitted = Some((at, n));
                (0.0, None)
            }
            // avant l'impulsion : on écoute le bruit de fond
            None => {
                self.noise_sum += (x as f64) * (x as f64);
                self.noise_n += 1;
                if t >= self.next_emit {
                    self.noise = (self.noise_sum / self.noise_n.max(1) as f64).sqrt() as f32;
                    self.emitted = Some((t, 0));
                    return (AMPLITUDE, None);
                }
                (0.0, None)
            }
        }
    }

    /// Conclut la fenêtre écoutée. La **crête** dit si quelque chose est revenu — elle doit sortir
    /// de `MIN_RATIO` fois le bruit de fond —, et c'est le **début** du retour qui donne la
    /// latence : le premier échantillon qui monte à `ONSET_FRAC` de cette crête, jamais sous le
    /// plancher du bruit.
    fn conclude(&self) -> Event {
        let (peak, noise) = (self.heard.iter().copied().fold(0.0f32, f32::max), self.noise);
        let floor = noise.max(NOISE_FLOOR) * MIN_RATIO;
        if peak <= floor {
            return Event::TooWeak { peak, noise };
        }
        let threshold = (peak * ONSET_FRAC).max(floor);
        let onset = self.heard.iter().position(|a| *a >= threshold).unwrap_or(0);
        // `heard[j]` est la frame impulsion + 1 + j
        Event::Measured { frames: onset as u64 + 1, peak, noise }
    }

    /// Repart à zéro, la prochaine impulsion au plus tôt à `at`. Entre deux passes, la chaîne
    /// change : sa queue (celle d'une réverbe) doit s'éteindre avant qu'on mesure un bruit de fond,
    /// sinon la passe suivante le trouverait trop haut et croirait ne rien voir revenir.
    fn restart(&mut self, at: u64) {
        self.emitted = None;
        self.next_emit = at;
        self.noise_sum = 0.0;
        self.noise_n = 0;
    }
}

/// Place de la chaîne pendant une passe de mesure.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Spot {
    /// Rien entre l'impulsion et la carte : le trajet matériel seul.
    Bare,
    /// L'impulsion traverse la chaîne : la latence mesurée contient ce que la chaîne lui fait.
    InPath,
    /// La chaîne tourne sur un bruit interne, sa sortie est jetée, l'impulsion part telle quelle :
    /// la latence reste celle du trajet nu, et seule la charge change.
    Beside,
}

impl Spot {
    pub(super) fn label(self) -> &'static str {
        match self {
            Spot::Bare => "—",
            Spot::InPath => "dans le trajet",
            Spot::Beside => "à côté",
        }
    }
}

/// Une passe : la chaîne (indice dans le catalogue bâti) et sa place.
#[derive(Clone, Copy)]
pub(super) struct Pass {
    pub(super) chain: Option<usize>,
    pub(super) spot: Spot,
}

/// Le trajet nu, puis deux passes par chaîne. L'ordre compte : la référence d'abord, pour que
/// l'écart des autres lignes ait un sens même si la mesure se dégrade en route.
pub(super) fn plan(chains: usize) -> Vec<Pass> {
    let mut v = vec![Pass { chain: None, spot: Spot::Bare }];
    for i in 0..chains {
        v.push(Pass { chain: Some(i), spot: Spot::InPath });
        v.push(Pass { chain: Some(i), spot: Spot::Beside });
    }
    v
}

/// Ce qui remonte du temps réel au thread d'affichage.
#[derive(Debug, PartialEq)]
pub(super) enum Report {
    /// Une fenêtre d'écoute conclue, dans la passe `pass`. `counted` est faux pour une fenêtre
    /// d'échauffement : elle s'affiche, elle ne compte pas.
    Window { pass: usize, event: Event, counted: bool },
    /// Le plan est fini : plus rien ne viendra.
    Done,
}

/// L'enchaînement des passes, **dans le callback** : comme `Detector`, une machine à états pure
/// qui n'alloue pas, ne verrouille pas et ne libère rien. Elle avance seule dès qu'une passe a ses
/// mesures (ou qu'elle a renoncé), si bien que le thread de contrôle n'a rien à piloter.
pub(super) struct Runner {
    det: Detector,
    passes: Vec<Pass>,
    chains: Vec<dsp::Chain>,
    loads: Arc<Vec<LoadHistogram>>,
    /// Mesures demandées par passe, et où en est la passe courante.
    count: u32,
    pass: usize,
    done: u32,
    weak: u32,
    /// Frames d'attente après un changement de passe ([`SETTLE_PASS_S`]).
    settle: u64,
    /// Fenêtres restant à ne pas compter dans la passe courante.
    warmup: u32,
    /// Tampons préalloués : le bloc de travail, et le bruit interne du mode « à côté ».
    buf: Vec<f32>,
    noise: Vec<f32>,
    noise_at: usize,
    frame: u64,
    /// Durée d'une frame (s) : convertit le temps passé dans la chaîne en part du budget.
    frame_secs: f64,
}

impl Runner {
    pub(super) fn new(
        rate: u32,
        count: u32,
        passes: Vec<Pass>,
        chains: Vec<dsp::Chain>,
        loads: Arc<Vec<LoadHistogram>>,
    ) -> Self {
        let frames = |seconds: f64| (rate as f64 * seconds) as u64;
        let det = Detector::new(frames(WINDOW_S), frames(INTERVAL_S), frames(SETTLE_FIRST_S));
        let mut seed = 0x1234_5678u32;
        let noise = (0..NOISE_LEN)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.6
            })
            .collect();
        Self {
            det,
            passes,
            chains,
            loads,
            count,
            pass: 0,
            done: 0,
            weak: 0,
            settle: frames(SETTLE_PASS_S),
            warmup: WARMUP,
            buf: vec![0.0; MAX_BLOCK],
            noise,
            noise_at: 0,
            frame: 0,
            frame_secs: 1.0 / rate as f64,
        }
    }

    fn finished(&self) -> bool {
        self.pass >= self.passes.len()
    }

    /// Un bloc du callback, découpé pour tenir dans les tampons préalloués.
    pub(super) fn block(&mut self, input: &[f32], out: &mut [f32], push: &mut impl FnMut(Report)) {
        for (inp, o) in input.chunks(MAX_BLOCK).zip(out.chunks_mut(MAX_BLOCK)) {
            self.chunk(inp, o, push);
        }
    }

    fn chunk(&mut self, input: &[f32], out: &mut [f32], push: &mut impl FnMut(Report)) {
        if self.finished() {
            out.fill(0.0);
            return;
        }
        let pass = self.pass;
        let Pass { chain, spot } = self.passes[pass];
        // émission et détection, échantillon par échantillon ; la sortie porte l'impulsion nue,
        // que le mode « dans le trajet » remplacera par celle de la chaîne
        for (k, (x, o)) in input.iter().zip(out.iter_mut()).enumerate() {
            let (emit, event) = self.det.sample(self.frame + k as u64, *x);
            self.buf[k] = emit;
            *o = emit;
            if let Some(event) = event {
                // une fenêtre sans retour compte toujours pour l'abandon : un câble débranché ne
                // doit pas faire durer la passe plus longtemps qu'avant
                if matches!(event, Event::TooWeak { .. }) {
                    self.weak += 1;
                }
                let counted = self.warmup == 0;
                if counted {
                    if matches!(event, Event::Measured { .. }) {
                        self.done += 1;
                    }
                } else {
                    self.warmup -= 1;
                }
                push(Report::Window { pass, event, counted });
                if self.done >= self.count || self.weak >= MAX_WEAK {
                    self.next_pass(self.frame + k as u64, push);
                }
            }
        }
        self.frame += input.len() as u64;
        // le travail DSP de la passe, chronométré comme le callback d'`engine`
        let Some(ci) = chain else { return };
        // les deux tranches ont la même longueur, mais la borner garde le callback sans panique
        // possible, même si un backend servait un jour deux longueurs différentes
        let n = input.len().min(out.len());
        let t = Instant::now();
        match spot {
            Spot::InPath => {
                self.chains[ci].process(&mut self.buf[..n]);
                out[..n].copy_from_slice(&self.buf[..n]);
            }
            Spot::Beside => {
                for s in self.buf[..n].iter_mut() {
                    *s = self.noise[self.noise_at];
                    self.noise_at = (self.noise_at + 1) % self.noise.len();
                }
                self.chains[ci].process(&mut self.buf[..n]);
            }
            Spot::Bare => {}
        }
        let load = t.elapsed().as_secs_f64() / (n as f64 * self.frame_secs);
        self.loads[pass].record(load as f32);
    }

    fn next_pass(&mut self, at: u64, push: &mut impl FnMut(Report)) {
        self.pass += 1;
        self.done = 0;
        self.weak = 0;
        self.warmup = WARMUP;
        self.det.restart(at + self.settle);
        if self.finished() {
            push(Report::Done);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Fait tourner le détecteur sur un signal d'entrée donné par `input(t)`, et rend les
    /// événements produits ainsi que les instants d'émission.
    fn run_detector(
        window: u64,
        interval: u64,
        n: u64,
        mut input: impl FnMut(u64, &[u64]) -> f32,
    ) -> Vec<Event> {
        let mut det = Detector::new(window, interval, 10);
        let mut emits: Vec<u64> = Vec::new();
        let mut events = Vec::new();
        for t in 0..n {
            let x = input(t, &emits);
            let (emit, event) = det.sample(t, x);
            if emit != 0.0 {
                emits.push(t);
            }
            if let Some(e) = event {
                events.push(e);
            }
        }
        events
    }

    /// Un retour franc : la latence trouvée est le décalage, quel que soit son niveau — jusqu'à
    /// -60 dBFS, là où un seuil fixe de 0,05 (-26 dBFS) ne verrait rien.
    #[test]
    fn the_delay_is_found_whatever_the_return_level() {
        for amplitude in [0.7f32, 0.05, 0.01, 0.001] {
            let events = run_detector(100, 200, 400, |t, emits| {
                // retour 37 frames après chaque impulsion, sur un bruit de fond très bas
                let back = emits.iter().any(|e| t == e + 37);
                if back {
                    amplitude
                } else {
                    1e-6
                }
            });
            assert!(!events.is_empty(), "amplitude {amplitude}");
            match &events[0] {
                Event::Measured { frames, peak, .. } => {
                    assert_eq!(*frames, 37, "amplitude {amplitude}");
                    assert!((peak - amplitude).abs() < 1e-9);
                }
                other => panic!("amplitude {amplitude} : {other:?}"),
            }
        }
    }

    /// Une impulsion **étalée** (ce que PipeWire renvoie quand son étage de volume la filtre) :
    /// c'est son **début** qui donne la latence, et non sa crête — qui arrive ici 8 frames plus
    /// tard, et qui sauterait d'un lobe à l'autre sur un retour qui sonne.
    #[test]
    fn a_smeared_return_is_measured_at_its_onset() {
        let events = run_detector(200, 400, 800, |t, emits| {
            let Some(e) = emits.iter().find(|e| t >= **e && t < **e + 200) else { return 1e-6 };
            // sinc étalé sur ~20 frames, culminant 50 frames après l'impulsion
            let d = t as f32 - (*e + 50) as f32;
            0.02 * (-(d * d) / 50.0).exp()
        });
        match &events[0] {
            // crête à 50, début (-12 dB de la crête) à 42
            Event::Measured { frames, .. } => assert_eq!(*frames, 42),
            other => panic!("{other:?}"),
        }
    }

    /// Rien d'autre que du bruit : la mesure dit qu'elle ne voit rien, elle n'invente pas une
    /// latence (le bruit a bien un maximum, mais il ne sort pas du bruit).
    #[test]
    fn pure_noise_is_reported_as_too_weak() {
        let mut seed = 12345u32;
        let events = run_detector(100, 200, 400, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.01
        });
        assert!(!events.is_empty());
        for e in &events {
            assert!(matches!(e, Event::TooWeak { .. }), "{e:?}");
        }
    }

    /// Entrée parfaitement muette (câble débranché) : pas de division par zéro, et pas de mesure.
    #[test]
    fn a_silent_input_does_not_divide_by_zero() {
        let events = run_detector(100, 200, 400, |_, _| 0.0);
        assert!(!events.is_empty());
        assert!(matches!(events[0], Event::TooWeak { peak: 0.0, .. }), "{:?}", events[0]);
    }

    /// Le détecteur émet bien une impulsion par cycle, à l'intervalle demandé.
    #[test]
    fn one_impulse_per_interval() {
        let mut det = Detector::new(100, 200, 10);
        let mut emits = Vec::new();
        for t in 0..1000 {
            let (emit, _) = det.sample(t, 0.0);
            if emit != 0.0 {
                assert_eq!(emit, AMPLITUDE);
                emits.push(t);
            }
        }
        // première à 10, puis une par (fenêtre + intervalle)
        assert_eq!(emits[0], 10);
        let gaps: Vec<u64> = emits.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|g| *g == 300), "{emits:?}");
    }
    /// Fait tourner l'automate bloc par bloc, en renvoyant sur l'entrée, `delay` frames plus tard,
    /// ce qui sort de la sortie : le câble de bouclage, simulé. `rate` est petit exprès — les
    /// fenêtres du détecteur en découlent, donc une passe tient en quelques milliers de frames.
    fn drive(chains: Vec<dsp::Chain>, count: u32, delays: &[u64]) -> Vec<Report> {
        let passes = plan(chains.len());
        let loads = Arc::new((0..passes.len()).map(|_| LoadHistogram::default()).collect::<Vec<_>>());
        let mut runner = Runner::new(400, count, passes.clone(), chains, loads);
        // bloc plus court que le retard du bouclage : un retour tombe toujours sur un bloc
        // suivant, jamais sur celui qu'on vient de lire
        let (block, mut reports) = (32usize, Vec::new());
        let mut back: BTreeMap<u64, f32> = BTreeMap::new();
        let (mut inp, mut out) = (vec![0.0f32; block], vec![0.0f32; block]);
        let (mut t, mut emits) = (0u64, 0usize);
        for _ in 0..4000 {
            for (k, x) in inp.iter_mut().enumerate() {
                *x = back.remove(&(t + k as u64)).unwrap_or(1e-6);
            }
            out.fill(0.0);
            runner.block(&inp, &mut out, &mut |r| reports.push(r));
            for (k, y) in out.iter().enumerate() {
                if y.abs() > 0.1 && !delays.is_empty() {
                    // le retard de la n-ième impulsion : la dernière valeur tient pour la suite,
                    // ce qui permet de simuler une dérive au démarrage puis un flux établi
                    let d = delays[emits.min(delays.len() - 1)];
                    back.insert(t + k as u64 + d, 0.5);
                    emits += 1;
                }
            }
            t += block as u64;
            if runner.finished() {
                break;
            }
        }
        reports
    }

    /// Les passes de chaque fenêtre, dans l'ordre, et si le plan s'est bien terminé. `only_counted`
    /// écarte les fenêtres d'échauffement.
    fn passes_of(reports: &[Report], only_counted: bool) -> (Vec<usize>, bool) {
        let done = matches!(reports.last(), Some(Report::Done));
        (
            reports
                .iter()
                .filter_map(|r| match r {
                    Report::Window { pass, counted, .. } if *counted || !only_counted => Some(*pass),
                    _ => None,
                })
                .collect(),
            done,
        )
    }

    /// Le plan : la référence nue d'abord — sans elle, l'écart des autres lignes ne voudrait rien
    /// dire —, puis deux passes par chaîne, dans le trajet puis à côté.
    #[test]
    fn the_plan_starts_with_the_bare_path_then_two_passes_per_chain() {
        let p = plan(2);
        assert_eq!(p.len(), 5);
        assert_eq!(p[0].chain, None);
        assert_eq!(p[0].spot, Spot::Bare);
        assert_eq!(
            p[1..].iter().map(|p| (p.chain.unwrap(), p.spot)).collect::<Vec<_>>(),
            [(0, Spot::InPath), (0, Spot::Beside), (1, Spot::InPath), (1, Spot::Beside)]
        );
        assert!(plan(0).len() == 1, "sans chaîne, la mesure d'aujourd'hui, et rien d'autre");
    }

    /// Chaque passe prend ses mesures puis cède la place, et le plan se termine de lui-même.
    #[test]
    fn every_pass_takes_its_measures_then_hands_over() {
        let reports = drive(vec![dsp::Chain::new(), dsp::Chain::new()], 3, &[37]);
        let (passes, done) = passes_of(&reports, true);
        assert!(done, "le plan n'a pas annoncé sa fin : {} rapports", reports.len());
        assert_eq!(passes, [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4]);
        for r in &reports {
            if let Report::Window { pass, event, .. } = r {
                assert!(matches!(event, Event::Measured { frames: 37, .. }), "passe {pass} : {event:?}");
            }
        }
    }

    /// Une passe qui ne voit rien revenir n'arrête pas les suivantes : elle renonce au bout de
    /// `MAX_WEAK` fenêtres et la mesure continue. C'est ce qui rend le tableau lisible en carte
    /// simulée, où rien ne boucle.
    #[test]
    fn a_pass_without_return_does_not_stop_the_others() {
        let reports = drive(vec![dsp::Chain::new()], 3, &[]);
        let (passes, done) = passes_of(&reports, false);
        assert!(done);
        assert_eq!(passes.len() as u32, 3 * MAX_WEAK);
        assert_eq!(passes.iter().copied().collect::<std::collections::BTreeSet<_>>(), [0, 1, 2].into());
        assert!(reports.iter().all(|r| !matches!(r, Report::Window { event: Event::Measured { .. }, .. })));
    }

    /// Les deux places de la chaîne font bien deux choses différentes : une chaîne qui coupe tout
    /// ne laisse rien partir quand elle est **dans le trajet**, alors que **à côté** elle tourne
    /// sans toucher à l'impulsion — c'est pour ça que la seconde ligne sert de témoin.
    #[test]
    fn in_the_path_the_chain_shapes_what_leaves_beside_it_does_not() {
        let mut chain = dsp::Chain::new();
        let fx: dsp::fx::FxSpec = "gain:db=-60".parse().unwrap();
        chain.push_boxed(fx.build(&dsp::fx::BuildCtx::new(400, MAX_BLOCK)).unwrap());
        let reports = drive(vec![chain], 2, &[37]);
        let measured: Vec<usize> = reports
            .iter()
            .filter_map(|r| match r {
                Report::Window { pass, event: Event::Measured { .. }, counted: true } => Some(*pass),
                _ => None,
            })
            .collect();
        assert_eq!(measured, [0, 0, 2, 2], "mesuré nu et à côté, rien dans le trajet : {reports:?}");
    }
    /// Les premières fenêtres d'une passe s'affichent mais ne comptent pas.
    #[test]
    fn the_first_windows_of_a_pass_are_not_counted() {
        let reports = drive(Vec::new(), 2, &[37]);
        let counted: Vec<bool> = reports
            .iter()
            .filter_map(|r| match r {
                Report::Window { counted, .. } => Some(*counted),
                Report::Done => None,
            })
            .collect();
        let attendu: Vec<bool> = (0..WARMUP).map(|_| false).chain([true, true]).collect();
        assert_eq!(counted, attendu);
    }

    /// Le cas qui faisait mentir le tableau : l'aller-retour **dérive** le temps que le flux
    /// s'établisse. Les fenêtres d'échauffement l'absorbent, et les mesures comptées sont toutes
    /// celles du régime établi — donc une dispersion nulle, et une référence juste.
    #[test]
    fn a_drifting_start_does_not_pollute_the_counted_measures() {
        let reports = drive(Vec::new(), 3, &[30, 41, 48]);
        let frames: Vec<u64> = reports
            .iter()
            .filter_map(|r| match r {
                Report::Window { event: Event::Measured { frames, .. }, counted: true, .. } => Some(*frames),
                _ => None,
            })
            .collect();
        assert_eq!(frames, [48, 48, 48], "la dérive du démarrage a été comptée");
    }
}
