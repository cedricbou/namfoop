use crate::param::Control;
use crate::{db_to_gain, Stage};

/// Gain fixe (en dB à la construction).
pub struct Gain {
    gain: f32,
    name: String,
}

impl Gain {
    pub fn db(db: f32, name: impl Into<String>) -> Self {
        Self { gain: db_to_gain(db), name: name.into() }
    }
}

impl Stage for Gain {
    fn process(&mut self, buf: &mut [f32]) {
        if self.gain != 1.0 {
            for x in buf {
                *x *= self.gain;
            }
        }
    }
    fn name(&self) -> &str {
        &self.name
    }
}

/// Durée de la rampe d'un gain vivant, en millisecondes : la même que le fondu de chaîne
/// (`swap.rs`), pour n'avoir qu'un seul temps de fondu dans le produit.
pub const RAMP_MS: f32 = 20.0;

/// Gain **vivant** : il suit une case de réglage en dB ([`Control`]), réglable pendant que le son
/// joue, et rampe linéairement vers chaque nouvelle valeur sur [`RAMP_MS`] (ADR 0007).
///
/// La rampe compte en **échantillons** et traverse les frontières de bloc : le son ne dépend donc
/// pas de la taille de bloc (48 en ALSA, 64 en JACK, 1000 en `render`), ce qu'une rampe étalée sur
/// le buffer ne tiendrait pas. Sur la plage du volume d'un preset (-24 à +12 dB), un saut étalé sur
/// un bloc de 48 serait un transitoire, et sur un bloc de 1000 un glissando.
///
/// En régime établi — la case n'a pas bougé depuis le bloc précédent — le travail est exactement
/// celui de [`Gain`] : une multiplication par échantillon, et pas même une conversion dB.
pub struct LiveGain {
    db: Control,
    /// Les bits du dB lu au bloc précédent : rien à recalculer tant qu'ils ne changent pas.
    last_bits: u32,
    /// Gain linéaire appliqué à l'échantillon courant.
    cur: f32,
    /// Gain linéaire visé.
    target: f32,
    /// Échantillons de rampe restants (0 : régime établi).
    left: u32,
    /// Longueur d'une rampe complète, en échantillons (au moins 1).
    ramp_len: u32,
    name: String,
}

impl LiveGain {
    /// La case est **lue tout de suite** : une chaîne préchargée depuis dix minutes ne doit pas
    /// démarrer sur une valeur périmée — avec la plage du volume d'un preset, le saut irait
    /// jusqu'à 36 dB.
    pub fn new(db: Control, sample_rate: u32, name: impl Into<String>) -> Self {
        let v = db.get();
        let ramp_len = ((RAMP_MS / 1000.0 * sample_rate as f32) as u32).max(1);
        Self {
            last_bits: v.to_bits(),
            cur: Self::gain_of(v),
            target: Self::gain_of(v),
            left: 0,
            ramp_len,
            name: name.into(),
            db,
        }
    }

    /// Le gain linéaire d'une valeur en dB, en ignorant une valeur non finie (un contrôleur qui
    /// déraperait ne doit pas éteindre ni saturer la sortie).
    fn gain_of(db: f32) -> f32 {
        if db.is_finite() {
            db_to_gain(db)
        } else {
            1.0
        }
    }
}

impl Stage for LiveGain {
    fn process(&mut self, buf: &mut [f32]) {
        // une lecture atomique par bloc ; la conversion en gain n'a lieu que si la valeur a changé
        let v = self.db.get();
        let bits = v.to_bits();
        if bits != self.last_bits {
            self.last_bits = bits;
            let target = Self::gain_of(v);
            if target != self.target {
                self.target = target;
                self.left = self.ramp_len;
            }
        }
        if self.left == 0 {
            if self.cur != 1.0 {
                for x in buf {
                    *x *= self.cur;
                }
            }
            return;
        }
        for x in buf.iter_mut() {
            if self.left > 0 {
                // distance restante / échantillons restants : un pas constant de Δ / ramp_len, et
                // la cible atteinte exactement au dernier
                self.cur += (self.target - self.cur) / self.left as f32;
                self.left -= 1;
                if self.left == 0 {
                    self.cur = self.target;
                }
            }
            *x *= self.cur;
        }
    }

    /// Hors temps réel : recale sur la case. `engine::bank` l'appelle sur chaque chaîne qui revient,
    /// donc une chaîne remise en réserve repart de la valeur du moment, sans rampe.
    fn reset(&mut self) {
        let v = self.db.get();
        self.last_bits = v.to_bits();
        self.cur = Self::gain_of(v);
        self.target = self.cur;
        self.left = 0;
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn run(stage: &mut dyn Stage, n: usize, block: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(n);
        let mut buf = vec![0.0; block];
        while out.len() < n {
            buf.iter_mut().for_each(|x| *x = 1.0);
            stage.process(&mut buf);
            out.extend_from_slice(&buf);
        }
        out.truncate(n);
        out
    }

    /// Case immobile : un gain vivant est **identique au bit** à un gain fixe, et ne dépend pas de
    /// la taille de bloc (c'est ce qui prouve que la case est lue à la construction).
    #[test]
    fn a_still_control_is_bit_identical_to_a_fixed_gain() {
        for db in [-24.0, -6.0, 0.0, 12.0] {
            let want = run(&mut Gain::db(db, "g"), 300, 64);
            for block in [16, 48, 64, 1000] {
                let mut live = LiveGain::new(Control::new(db), SR, "volume");
                assert_eq!(run(&mut live, 300, block), want, "db={db} bloc={block}");
            }
        }
    }

    /// La rampe dure `RAMP_MS`, en échantillons, et traverse les frontières de bloc : la cible est
    /// atteinte exactement au bout, et aucun pas ne dépasse Δ / `ramp_len`.
    #[test]
    fn the_ramp_lasts_the_fixed_time_and_crosses_block_boundaries() {
        let c = Control::new(0.0);
        let len = (RAMP_MS / 1000.0 * SR as f32) as usize; // 960 échantillons à 48 kHz
        for block in [16, 64, 1000] {
            let mut live = LiveGain::new(c.clone(), SR, "volume");
            c.set(0.0);
            live.reset();
            run(&mut live, block, block); // régime établi à 1.0
            c.set(12.0);
            let out = run(&mut live, len + block, block);
            let target = db_to_gain(12.0);
            assert!(out[0] > 1.0 && out[0] < target, "la rampe commence : {}", out[0]);
            assert!(
                (out[len - 1] - target).abs() < 1e-5,
                "cible atteinte au bout de la rampe (bloc {block}) : {}",
                out[len - 1]
            );
            assert_eq!(out[len + block - 1], target, "puis elle y reste");
            let step = (target - 1.0) / len as f32;
            for w in out[..len].windows(2) {
                assert!(w[1] - w[0] <= step * 1.001, "pas trop grand : {} > {step}", w[1] - w[0]);
            }
        }
    }

    /// Une valeur non finie est ignorée : la sortie ne s'éteint pas et ne sature pas.
    #[test]
    fn a_non_finite_control_is_ignored() {
        let c = Control::new(f32::NAN);
        let mut live = LiveGain::new(c.clone(), SR, "volume");
        assert_eq!(run(&mut live, 64, 64), vec![1.0; 64], "NaN à la construction : gain neutre");
        c.set(f32::INFINITY);
        assert!(run(&mut live, 64, 64).iter().all(|x| *x == 1.0), "et en cours de route aussi");
    }
}
