//! **Le clic du métronome** (ADR 0008) : une salve courte par temps, **ajoutée** aux seules sorties
//! qui l'ont demandé (`model::routing::OutputRoute::click`) — les in-ears veulent la basse *et* le
//! clic, donc ce n'est pas une `Source` de plus. Un clic parti dans le sondier en concert est une
//! catastrophe : aucune sortie ne le reçoit sans l'avoir dit.
//!
//! Éteint tant qu'on ne l'allume pas : c'est une aide d'écoute, pas un son, et il ne vit donc pas
//! dans un preset.
//!
//! Le tempo est **la même case** que celle des effets de temps (ADR 0007) : le clic suit le tap et,
//! un jour, l'horloge MIDI. La **phase ne se remet pas à zéro** quand le tempo change — seul
//! l'écart entre deux temps change, ce qui est ce qui s'entend naturellement ; elle se recale sur
//! un **temps 1** demandé (un tap, un `Start` MIDI), que l'appelant signale en changeant la valeur
//! de la case `downbeat`.

use dsp::biquad::{Biquad, Shape};
use dsp::param::Control;
pub use model::config::ClickSound;

/// Durée d'une salve (ms) : au-delà, elle est éteinte de toute façon.
const BURST_MS: f32 = 25.0;
/// Attaque (ms) : assez courte pour percuter, assez longue pour ne pas être une marche d'escalier
/// (qui s'entendrait comme un grésillement, surtout rééchantillonnée).
const ATTACK_MS: f32 = 0.3;
/// Hauteur du clic, et celle du **premier temps** (accentué).
const CLICK_HZ: f32 = 1000.0;
const ACCENT_HZ: f32 = 2000.0;
/// `bois` : second partiel **inharmonique** (×2,6), ce qui fait la couleur d'un bloc de bois
/// plutôt que celle d'une flûte.
const WOOD_RATIO: f32 = 2.6;
/// Constantes de temps de décroissance (ms) : le temps de perdre 1/e.
const WOOD_TAU_MS: f32 = 7.0;
const TIC_TAU_MS: f32 = 1.5;

/// Ce qui pilote le clic. Les trois cases sont **vivantes** : allumer, changer de tempo ou recaler
/// le temps 1 s'entend au bloc suivant, sans rien reconstruire.
#[derive(Clone, Debug)]
pub struct MetronomeConfig {
    /// Gain **linéaire** du clic ; `0` (ou non fini) : éteint. C'est l'appelant qui convertit le
    /// niveau en dB de la config, pour qu'`engine` n'ait qu'un nombre à lire.
    pub gain: Control,
    /// Tempo (BPM) : la même case que les effets de temps.
    pub tempo: Control,
    /// **Compteur** de recalage du temps 1 : à chaque fois que sa valeur **change**, le clic repart
    /// sur un premier temps. C'est ce que `engine::tempo::Arbiter::beat_epoch` fournit.
    pub downbeat: Control,
    /// Temps par mesure ; le premier est accentué. 1 : aucun accent.
    pub beats_per_bar: u32,
    /// Le **son** du clic : ce qui passe dans un retour de basse ne se choisit qu'à l'oreille
    /// (#230).
    pub sound: ClickSound,
}

/// Le générateur, côté temps réel : aucune allocation, aucune panique possible.
pub struct Metronome {
    cfg: MetronomeConfig,
    sample_rate: f32,
    /// Échantillons depuis le dernier temps.
    pos: u32,
    /// Temps courant dans la mesure (0 : le premier, accentué).
    beat: u32,
    /// Dernière valeur vue de `downbeat` : un changement recale.
    last_downbeat: f32,
    /// Échantillons restants de la salve en cours (0 : aucune).
    burst_left: u32,
    burst_len: u32,
    /// Phase (radians) et incrément de la salve en cours.
    phase: f32,
    inc: f32,
    /// Enveloppe courante et son facteur par échantillon (décroissance exponentielle).
    env: f32,
    decay: f32,
    /// Échantillons restants de l'attaque (en deçà, l'enveloppe monte).
    attack_len: u32,
    /// Filtre de la salve de bruit (`tic`) et son générateur, déterministe.
    band: Biquad,
    seed: u32,
}

impl Metronome {
    pub fn new(cfg: MetronomeConfig, sample_rate: u32) -> Self {
        let sr = sample_rate.max(1) as f32;
        Self {
            last_downbeat: cfg.downbeat.get(),
            cfg,
            sample_rate: sr,
            pos: 0,
            beat: 0,
            burst_left: 0,
            burst_len: (BURST_MS * 1e-3 * sr).max(1.0) as u32,
            phase: 0.0,
            inc: 0.0,
            env: 0.0,
            decay: 1.0,
            attack_len: (ATTACK_MS * 1e-3 * sr).max(1.0) as u32,
            band: Biquad::new(Shape::HighPass, 2000.0, 0.707, sr as f64),
            seed: 0x1234_5678,
        }
    }

    /// Un bruit blanc déterministe dans [-1, 1] (générateur congruentiel : aucune allocation).
    #[inline]
    fn noise(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }

    /// Écrit le clic dans `out` (il l'**écrase**, l'appelant l'ajoute où il veut) et dit s'il y a
    /// quelque chose à ajouter. Éteint, la **grille des temps continue d'avancer** : rallumer tombe
    /// juste, au lieu de repartir d'un temps arbitraire.
    pub fn render(&mut self, out: &mut [f32]) -> bool {
        let per_beat = self.samples_per_beat();
        let bars = self.cfg.beats_per_bar.max(1);

        // un temps 1 demandé (tap, Start MIDI) : on repart du premier temps
        let d = self.cfg.downbeat.get();
        if d != self.last_downbeat {
            self.last_downbeat = d;
            self.pos = 0;
            self.beat = 0;
            self.burst_left = 0;
        }

        let gain = self.cfg.gain.get();
        if !gain.is_finite() || gain <= 0.0 {
            self.advance(out.len() as u32, per_beat, bars);
            return false;
        }
        for x in out.iter_mut() {
            if self.pos == 0 && self.burst_left == 0 {
                self.start_burst(bars);
            }
            *x = if self.burst_left > 0 { self.sample() * gain } else { 0.0 };
            self.pos += 1;
            if self.pos >= per_beat {
                self.pos = 0;
                self.beat = (self.beat + 1) % bars;
            }
        }
        true
    }

    /// Prépare la salve d'un temps : hauteur, filtre et décroissance selon le son choisi.
    fn start_burst(&mut self, bars: u32) {
        let accent = self.beat == 0 && bars > 1;
        self.burst_left = self.burst_len;
        self.phase = 0.0;
        self.env = 0.0;
        let hz = if accent { ACCENT_HZ } else { CLICK_HZ };
        // `exp(-dt/tau)` par échantillon, précalculé une fois par temps (hors boucle)
        let per_sample = |tau_ms: f32| (-1.0 / (tau_ms * 1e-3 * self.sample_rate)).exp();
        match self.cfg.sound {
            ClickSound::Bois => {
                self.inc = std::f32::consts::TAU * hz / self.sample_rate;
                self.decay = per_sample(WOOD_TAU_MS);
            }
            ClickSound::Tic => {
                self.decay = per_sample(TIC_TAU_MS);
                // un bruit n'a pas de hauteur : c'est le **filtre** qui distingue le premier
                // temps, plus brillant. Un passe-haut ôte aussi le grondement, qui n'a rien à
                // faire dans un retour de basse.
                let f = if accent { 3500.0 } else { 2000.0 };
                self.band = Biquad::new(Shape::HighPass, f, 0.707, self.sample_rate as f64);
            }
            ClickSound::Bip => {
                self.inc = std::f32::consts::TAU * hz / self.sample_rate;
                self.decay = 1.0; // l'enveloppe du bip est en cosinus, pas exponentielle
            }
        }
    }

    /// Un échantillon de la salve en cours. L'attaque monte en [`ATTACK_MS`] — assez court pour
    /// percuter, assez long pour ne pas produire une marche d'escalier.
    #[inline]
    fn sample(&mut self) -> f32 {
        let done = self.burst_len - self.burst_left;
        self.burst_left -= 1;
        match self.cfg.sound {
            ClickSound::Bip => {
                // l'ancien son : enveloppe en cosinus surélevé, douce et symétrique
                let k = done as f32 / self.burst_len as f32;
                let env = 0.5 * (1.0 - (std::f32::consts::TAU * k).cos());
                let v = self.phase.sin() * env;
                self.phase += self.inc;
                v
            }
            ClickSound::Bois => {
                let env = self.envelope(done);
                // deux partiels inharmoniques : la couleur d'un bloc de bois
                let v = (self.phase.sin() + 0.5 * (self.phase * WOOD_RATIO).sin()) * env / 1.5;
                self.phase += self.inc;
                v
            }
            ClickSound::Tic => {
                let env = self.envelope(done);
                let n = self.noise();
                self.band.tick(n) * env
            }
        }
    }

    /// Attaque linéaire puis décroissance exponentielle : l'enveloppe percussive.
    #[inline]
    fn envelope(&mut self, done: u32) -> f32 {
        if done < self.attack_len {
            self.env = (done + 1) as f32 / self.attack_len as f32;
        } else {
            self.env *= self.decay;
        }
        self.env
    }

    /// Échantillons entre deux temps, bornés : le tempo vient d'une case que personne ne garantit.
    fn samples_per_beat(&self) -> u32 {
        let bpm = dsp::tempo::clamp_bpm(self.cfg.tempo.get());
        (60.0 / bpm * self.sample_rate).round().max(1.0) as u32
    }

    /// Fait avancer la grille sans rien produire (clic éteint).
    fn advance(&mut self, n: u32, per_beat: u32, bars: u32) {
        self.burst_left = 0;
        let total = self.pos as u64 + n as u64;
        self.pos = (total % per_beat as u64) as u32;
        let beats = (total / per_beat as u64) % bars as u64;
        self.beat = (self.beat as u64 + beats) as u32 % bars;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48000;

    fn metronome(bpm: f32, bars: u32, gain: f32) -> (Metronome, MetronomeConfig) {
        sounded(bpm, bars, gain, ClickSound::default())
    }

    fn sounded(bpm: f32, bars: u32, gain: f32, sound: ClickSound) -> (Metronome, MetronomeConfig) {
        let cfg = MetronomeConfig {
            gain: Control::new(gain),
            tempo: Control::new(bpm),
            downbeat: Control::new(0.0),
            beats_per_bar: bars,
            sound,
        };
        (Metronome::new(cfg.clone(), SR), cfg)
    }

    /// Les débuts de salve. L'enveloppe en cosinus surélevé **commence à zéro** (c'est elle qui
    /// évite le clic parasite), donc le premier échantillon non nul arrive un cran après le temps :
    /// on le retire pour lire des positions musicales.
    fn beats(out: &[f32]) -> Vec<usize> {
        let mut v = Vec::new();
        let burst = (BURST_MS * 1e-3 * SR as f32) as usize;
        let mut i = 0;
        while i < out.len() {
            if out[i] != 0.0 {
                v.push(i.saturating_sub(1));
                i += burst;
            } else {
                i += 1;
            }
        }
        v
    }

    fn render(m: &mut Metronome, samples: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; samples];
        for chunk in out.chunks_mut(64) {
            m.render(chunk);
        }
        out
    }

    /// Un temps toutes les 60/BPM secondes, à l'échantillon près, et la hauteur du **premier**
    /// temps est plus haute : c'est l'accent.
    #[test]
    fn one_click_per_beat_with_an_accent_on_the_first() {
        let (mut m, _cfg) = metronome(120.0, 4, 1.0);
        let out = render(&mut m, SR as usize * 2); // 2 s = 4 temps à 120 BPM
        let hits = beats(&out);
        assert_eq!(hits, vec![0, 24000, 48000, 72000], "un temps toutes les 500 ms : {hits:?}");

        // l'accent : plus de passages par zéro dans la salve, donc une hauteur plus haute
        let zeros = |from: usize| {
            let n = (BURST_MS * 1e-3 * SR as f32) as usize;
            out[from..from + n].windows(2).filter(|w| w[0] * w[1] < 0.0).count()
        };
        assert!(zeros(0) > zeros(24000) * 3 / 2, "{} contre {}", zeros(0), zeros(24000));
        // 25 ms à 2 kHz : une centaine de passages par zéro
        assert!((90..=110).contains(&zeros(0)), "{}", zeros(0));
    }

    /// Éteint, **rien** n'est écrit — mais la grille avance, si bien que rallumer tombe juste.
    #[test]
    fn silent_when_off_but_the_grid_keeps_running() {
        let (mut m, cfg) = metronome(120.0, 4, 0.0);
        let mut buf = vec![0.0f32; 64];
        assert!(!m.render(&mut buf), "éteint : rien à ajouter");
        assert!(buf.iter().all(|x| *x == 0.0));

        // 750 ms éteint (un temps et demi), puis on allume : le prochain temps tombe 250 ms plus
        // tard, pas tout de suite
        let _ = render(&mut m, SR as usize * 750 / 1000 - 64);
        cfg.gain.set(1.0);
        let out = render(&mut m, SR as usize / 2);
        let hits = beats(&out);
        assert_eq!(hits.first().copied(), Some(12000), "250 ms après : {hits:?}");
    }

    /// Un **temps 1 demandé** (un tap) recale la grille, où qu'elle en soit.
    #[test]
    fn a_downbeat_realigns_the_grid() {
        let (mut m, cfg) = metronome(120.0, 4, 1.0);
        let _ = render(&mut m, 30_000); // au milieu d'un temps
        cfg.downbeat.set(1.0);
        let out = render(&mut m, SR as usize);
        let hits = beats(&out);
        assert_eq!(hits.first().copied(), Some(0), "le clic repart tout de suite : {hits:?}");
        assert_eq!(hits, vec![0, 24000], "puis la grille repart de là");
    }

    /// Le tempo change : l'écart entre deux temps suit, **sans** remettre la phase à zéro — c'est
    /// ce qui s'entend naturellement (ADR 0008).
    #[test]
    fn a_tempo_change_moves_the_beats_without_restarting_the_bar() {
        let (mut m, cfg) = metronome(60.0, 4, 1.0);
        let out = render(&mut m, SR as usize); // 1 s = 1 temps à 60 BPM
        assert_eq!(beats(&out), vec![0], "un seul temps en une seconde à 60 BPM");
        cfg.tempo.set(120.0);
        let out = render(&mut m, SR as usize);
        assert_eq!(beats(&out), vec![0, 24000], "à 120 BPM, deux temps : {:?}", beats(&out));
    }

    /// Une case qui dérape ne fait ni paniquer ni produire de valeur non finie.
    #[test]
    fn nothing_a_control_can_hold_breaks_it() {
        for (bpm, gain) in [(f32::NAN, 1.0), (0.0, 1.0), (1e9, 1.0), (120.0, f32::NAN), (120.0, -1.0)] {
            let (mut m, _cfg) = metronome(bpm, 4, gain);
            let out = render(&mut m, 4096);
            assert!(out.iter().all(|x| x.is_finite()), "bpm {bpm}, gain {gain}");
        }
        // un nombre de temps par mesure nul ne divise pas par zéro
        let (mut m, _cfg) = metronome(120.0, 0, 1.0);
        assert!(render(&mut m, 4096).iter().all(|x| x.is_finite()));
    }

    /// **Ce qui fait un clic percussif, mesuré** (#230) : le temps pour atteindre la crête
    /// (l'attaque) et celui pour retomber sous -40 dB de cette crête. `bip` montait en 12,5 ms,
    /// d'où un son tonal qui se noie dans une basse ; `bois` et `tic` attaquent en moins d'une
    /// milliseconde.
    #[test]
    fn the_percussive_sounds_attack_fast_and_die_quickly() {
        let ms = |n: usize| n as f32 * 1000.0 / SR as f32;
        let mut report = String::new();
        for sound in [ClickSound::Bois, ClickSound::Tic, ClickSound::Bip] {
            let (mut m, _cfg) = sounded(120.0, 4, 1.0, sound);
            let out = render(&mut m, SR as usize / 4); // un quart de seconde : une seule salve
            let (peak_at, peak) =
                out.iter()
                    .enumerate()
                    .fold((0, 0.0f32), |a, (i, x)| if x.abs() > a.1 { (i, x.abs()) } else { a });
            let floor = peak * 0.01; // -40 dB
            let end = out.iter().rposition(|x| x.abs() > floor).unwrap_or(0);
            report += &format!(
                "  {:5} attaque {:5.2} ms, extinction {:5.1} ms\n",
                sound.name(),
                ms(peak_at),
                ms(end)
            );
            assert!(peak > 0.1, "{} : crête {peak}", sound.name());
            match sound {
                // percussifs : attaque sous la milliseconde, et tout est fini en 25 ms
                ClickSound::Bois | ClickSound::Tic => {
                    assert!(ms(peak_at) < 1.0, "{} : attaque en {:.2} ms", sound.name(), ms(peak_at));
                    assert!(ms(end) < 26.0, "{} : extinction à {:.1} ms", sound.name(), ms(end));
                }
                // l'ancien son : attaque lente, c'est précisément ce qui lui manquait
                ClickSound::Bip => assert!(ms(peak_at) > 5.0, "{:.2} ms", ms(peak_at)),
            }
        }
        println!("\nsons de clic (#230) :\n{report}");
    }

    /// L'accent est plus **brillant** quel que soit le son : `tic` n'a pas de hauteur (c'est du
    /// bruit), seul son filtre change, donc on le vérifie par la brillance et non par la fréquence.
    #[test]
    fn the_accent_is_brighter_whatever_the_sound() {
        for sound in [ClickSound::Bois, ClickSound::Tic, ClickSound::Bip] {
            let (mut m, _cfg) = sounded(120.0, 4, 1.0, sound);
            let out = render(&mut m, SR as usize); // 4 temps, le premier accentué
            let brightness = |from: usize| {
                let n = (BURST_MS * 1e-3 * SR as f32) as usize;
                out[from..from + n].windows(2).filter(|w| w[0] * w[1] < 0.0).count()
            };
            assert!(
                brightness(0) > brightness(24000),
                "{} : l'accent doit être plus brillant ({} contre {})",
                sound.name(),
                brightness(0),
                brightness(24000)
            );
        }
    }
}
