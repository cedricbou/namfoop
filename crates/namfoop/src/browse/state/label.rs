//! Étiquettes du synoptique : ce qui est écrit dans chaque case (§ 2 de
//! `docs/design/browse-chaine.md`).

use dsp::fx::FxSpec;

use super::chain::{Branch, Leaf};
use super::{IrChoice, State};

/// Valeur lisible : sans décimales inutiles (`150`, `4.5`).
pub fn num(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Étiquette d'un effet dans le synoptique (§ 2 de la spécification).
pub fn fx_label(spec: &FxSpec) -> String {
    let p = |name: &str| spec.get(name).map(num).unwrap_or_default();
    match spec.kind() {
        k @ ("hpf" | "lpf") => format!("{k} {}", p("freq")),
        "comp" => format!("comp {}:1", p("ratio")),
        "gate" => format!("gate {}", p("threshold")),
        k => k.to_string(),
    }
}

impl State {
    /// Étiquette d'un bloc simple (`nam·b7k`, `ir·sm57`, `comp 4:1`…).
    pub fn leaf_label(&self, l: &Leaf) -> String {
        match l {
            Leaf::Fx(s) => fx_label(&s.spec),
            Leaf::Nam { model, .. } => match model {
                Some(m) => format!("nam·{}", short(&self.models[*m].label)),
                None => "nam·(à choisir)".into(),
            },
            Leaf::Ir { ir, .. } => match ir {
                IrChoice::None => "ir·(à choisir)".into(),
                IrChoice::Item(i) => format!("ir·{}", short(&self.irs[*i].label)),
                IrChoice::Blend => "ir·mélange".into(),
            },
        }
    }

    /// Largeur du contenu d'une branche (ses blocs, ou `son direct` si elle est vide).
    pub fn branch_width(&self, b: &Branch) -> usize {
        if b.blocks.is_empty() {
            return "son direct".chars().count();
        }
        b.blocks.iter().map(|l| self.leaf_label(l).chars().count()).sum::<usize>() + 3 * (b.blocks.len() - 1)
    }
}

/// Étiquette courte d'un modèle ou d'une IR : le synoptique est large, on coupe à 16 caractères.
fn short(label: &str) -> String {
    label.chars().take(16).collect::<String>().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_specification() {
        let f = |s: &str| fx_label(&s.parse::<FxSpec>().unwrap());
        assert_eq!(f("lpf:freq=250"), "lpf 250");
        assert_eq!(f("hpf:freq=30"), "hpf 30");
        assert_eq!(f("comp:ratio=4"), "comp 4:1");
        assert_eq!(f("gate:threshold=-60"), "gate -60");
        assert_eq!(f("reverb"), "reverb");
        assert_eq!(num(4.5), "4.5");
        assert_eq!(num(150.0), "150");
    }
}
