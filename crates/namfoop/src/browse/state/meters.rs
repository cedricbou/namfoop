//! Vumètres du synoptique : où ils se trouvent ([`MeterAt`]) et ce qu'ils montrent
//! ([`MeterDisplay`] : montée immédiate, descente 28 dB/s, § 3 de la spécification).

use std::collections::HashMap;

use dsp::meter::ChainMeters;

use super::State;

/// Repère d'un vumètre : l'affichage garde une valeur lissée par repère.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeterAt {
    Input,
    Output,
    /// Bloc de la série principale.
    Block(usize),
    /// Gain d'une branche : ce qu'elle envoie à la somme.
    BranchSum(usize, usize),
    /// Bloc d'une branche.
    BranchBlock(usize, usize, usize),
}

/// Descente des vumètres (dB par seconde), § 3 de la spécification.
pub const FALL_DB_PER_S: f32 = 28.0;
/// Plancher des barres.
pub const FLOOR_DB: f32 = -60.0;

/// Ce que les barres montrent : montée immédiate, descente à [`FALL_DB_PER_S`]. L'action du comp et
/// du gate n'est pas lissée ici : elle suit déjà le `release` de l'effet (mesuré par `dsp::meter`).
#[derive(Clone, Debug, Default)]
pub struct MeterDisplay {
    db: HashMap<MeterAt, f32>,
    action: HashMap<MeterAt, f32>,
    last_ms: Option<u64>,
}

impl MeterDisplay {
    /// Lit la table (ce qui remet ses maxima à zéro) et met les barres à jour. `input` et `output`
    /// sont les crêtes linéaires publiées par `engine`.
    pub fn update(&mut self, now_ms: u64, table: Option<&ChainMeters>, input: f32, output: f32) {
        let dt = match self.last_ms {
            Some(t) if now_ms > t => (now_ms - t) as f32 / 1000.0,
            _ => 0.0,
        };
        self.last_ms = Some(now_ms);
        let fall = FALL_DB_PER_S * dt;
        let mut seen: Vec<MeterAt> = vec![MeterAt::Input, MeterAt::Output];
        self.set(MeterAt::Input, input, fall);
        self.set(MeterAt::Output, output, fall);
        if let Some(t) = table {
            for (i, b) in t.blocks.iter().enumerate() {
                let at = MeterAt::Block(i);
                seen.push(at);
                self.set(at, b.peak.take(), fall);
                if let Some(a) = &b.action {
                    self.action.insert(at, a.take());
                }
            }
            for s in &t.splits {
                for (k, br) in s.branches.iter().enumerate() {
                    let at = MeterAt::BranchSum(s.block, k);
                    seen.push(at);
                    self.set(at, br.sum.take(), fall);
                    for (j, b) in br.blocks.iter().enumerate() {
                        let at = MeterAt::BranchBlock(s.block, k, j);
                        seen.push(at);
                        self.set(at, b.peak.take(), fall);
                        if let Some(a) = &b.action {
                            self.action.insert(at, a.take());
                        }
                    }
                }
            }
        }
        // la chaîne a changé : on oublie les barres qui n'existent plus
        if self.db.len() > seen.len() {
            self.db.retain(|k, _| seen.contains(k));
            self.action.retain(|k, _| seen.contains(k));
        }
    }

    fn set(&mut self, at: MeterAt, peak: f32, fall: f32) {
        let db = if peak > 0.0 { dsp::gain_to_db(peak) } else { FLOOR_DB };
        let shown = self.db.entry(at).or_insert(FLOOR_DB);
        *shown = if db >= *shown { db } else { (*shown - fall).max(db).max(FLOOR_DB) };
    }

    /// Niveau affiché (dBFS, jamais sous [`FLOOR_DB`]).
    pub fn db(&self, at: MeterAt) -> f32 {
        self.db.get(&at).copied().unwrap_or(FLOOR_DB)
    }

    /// Action affichée : réduction du comp (dB) ou fermeture du gate (0 … 1).
    pub fn action(&self, at: MeterAt) -> f32 {
        self.action.get(&at).copied().unwrap_or(0.0)
    }
}

impl State {
    /// Met les vumètres du synoptique à jour (appelé à chaque image par la boucle, avec les crêtes
    /// d'entrée et de sortie d'`engine`). Lire la table remet ses maxima à zéro.
    pub fn update_meters(&mut self, now_ms: u64, in_peak: f32, out_peak: f32) {
        let table = self.chain_meters.clone();
        self.meters.update(now_ms, table.as_ref(), in_peak, out_peak);
    }

    /// La chaîne installée a changé : nouvelle table de vumètres.
    pub fn set_chain_meters(&mut self, table: ChainMeters) {
        self.chain_meters = Some(table);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Montée immédiate, descente à 28 dB/s, plancher à -60 dB (horloge fournie).
    #[test]
    fn meters_rise_at_once_and_fall_at_28_db_per_second() {
        let mut d = MeterDisplay::default();
        d.update(0, None, dsp::db_to_gain(-6.0), 0.0);
        assert!((d.db(MeterAt::Input) - -6.0).abs() < 0.01, "{}", d.db(MeterAt::Input));
        assert_eq!(d.db(MeterAt::Output), FLOOR_DB, "rien en sortie");
        // 100 ms de silence : -2,8 dB
        d.update(100, None, 0.0, 0.0);
        assert!((d.db(MeterAt::Input) - -8.8).abs() < 0.01, "{}", d.db(MeterAt::Input));
        // une crête plus forte remonte tout de suite
        d.update(200, None, dsp::db_to_gain(-2.0), 0.0);
        assert!((d.db(MeterAt::Input) - -2.0).abs() < 0.01, "{}", d.db(MeterAt::Input));
        // et la descente s'arrête au plancher
        d.update(10_000, None, 0.0, 0.0);
        assert_eq!(d.db(MeterAt::Input), FLOOR_DB);
    }
}
