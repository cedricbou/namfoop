//! Effets : une seule description par effet, partagée par la CLI (`--pre` / `--post`, `namfoop fx`),
//! `browse`, les presets (#6) et les CC MIDI (#7). Voir `docs/adr/0005-effets-et-chaines.md`.
//!
//! Un effet = des paramètres sérialisables (`FxSpec`, des données) + un [`Stage`] (le calcul, construit
//! par [`FxSpec::build`]). Chaque paramètre est décrit par un [`ParamInfo`] (nom, unité, plage, défaut,
//! pas, aide) : bornes vérifiées, aide et éditeurs générés depuis la même table.
//!
//! Ajouter un effet : un fichier qui implémente [`Effect`], une variante de [`FxSpec`] et une ligne
//! dans [`catalog`] (voir `crates/dsp/CLAUDE.md`, « Ajouter un effet »).

pub mod chorus;
pub mod comp;
pub mod delay;
pub mod filter;
pub mod gain;
pub mod gate;
pub mod reverb;
pub mod split;

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{Error, Result, Stage};

/// Unité d'un paramètre (affichage et aide).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Db,
    Ms,
    Hz,
    Ratio,
    Percent,
    Q,
    Seconds,
    /// Pente de filtre (dB/octave).
    DbOct,
}

impl Unit {
    pub fn symbol(self) -> &'static str {
        match self {
            Unit::Db => "dB",
            Unit::Ms => "ms",
            Unit::Hz => "Hz",
            Unit::Ratio => ":1",
            Unit::Percent => "%",
            Unit::Q => "Q",
            Unit::Seconds => "s",
            Unit::DbOct => "dB/oct",
        }
    }
}

/// Description d'un paramètre d'effet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamInfo {
    /// Nom dans la mini-syntaxe, les presets et les CC (`threshold`).
    pub name: &'static str,
    pub unit: Unit,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// Pas de l'éditeur de `browse`.
    pub step: f32,
    /// Aide en français, reprise par `namfoop fx` et le guide.
    pub help: &'static str,
}

/// Ce que chaque effet implémente (ses paramètres sont une structure sérialisable).
pub trait Effect: Clone + Default {
    /// Nom de l'effet (`gain`) : mini-syntaxe, presets, `namfoop fx`.
    const KIND: &'static str;
    /// Aide d'une ligne.
    const HELP: &'static str;
    const PARAMS: &'static [ParamInfo];
    fn get(&self, name: &str) -> Option<f32>;
    /// Écrit un paramètre déjà vérifié (nom connu, valeur dans la plage).
    fn put(&mut self, name: &str, v: f32);
    /// Construit l'étage (hors temps réel : peut allouer).
    fn build(&self, sample_rate: u32, max_block: usize) -> Result<Box<dyn Stage>>;
    /// Durée (s) pendant laquelle l'effet peut encore sonner après la fin du signal, au-delà des
    /// 10 s couvertes par défaut (delay, réverbe). Le test `silence_after_the_tail` attend cette
    /// durée, plafonnée à [`MAX_TEST_TAIL_SECONDS`] pour les réglages du catalogue.
    fn tail_seconds(&self) -> f32 {
        0.0
    }
}

/// Plus longue queue admise, pour chaque réglage de `FxSpec::extremes`, par les tests génériques.
pub const MAX_TEST_TAIL_SECONDS: f32 = 90.0;

/// Un effet réglé : type + paramètres. Sérialisé avec un champ `type` (`type = "gain"`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FxSpec {
    Gain(gain::GainParams),
    Gate(gate::GateParams),
    Comp(comp::CompParams),
    Hpf(filter::HpfParams),
    Lpf(filter::LpfParams),
    Peak(filter::PeakParams),
    Lowshelf(filter::LowshelfParams),
    Highshelf(filter::HighshelfParams),
    Split(split::SplitParams),
    Delay(delay::DelayParams),
    Chorus(chorus::ChorusParams),
    Reverb(reverb::ReverbParams),
}

/// Applique `$body` à l'effet contenu, quel que soit son type.
macro_rules! with_effect {
    ($spec:expr, $e:ident => $body:expr) => {
        match $spec {
            FxSpec::Gain($e) => $body,
            FxSpec::Gate($e) => $body,
            FxSpec::Comp($e) => $body,
            FxSpec::Hpf($e) => $body,
            FxSpec::Lpf($e) => $body,
            FxSpec::Peak($e) => $body,
            FxSpec::Lowshelf($e) => $body,
            FxSpec::Highshelf($e) => $body,
            FxSpec::Split($e) => $body,
            FxSpec::Delay($e) => $body,
            FxSpec::Chorus($e) => $body,
            FxSpec::Reverb($e) => $body,
        }
    };
}

/// Un exemplaire par défaut de chaque effet.
pub fn catalog() -> Vec<FxSpec> {
    vec![
        FxSpec::Gain(Default::default()),
        FxSpec::Gate(Default::default()),
        FxSpec::Comp(Default::default()),
        FxSpec::Hpf(Default::default()),
        FxSpec::Lpf(Default::default()),
        FxSpec::Peak(Default::default()),
        FxSpec::Lowshelf(Default::default()),
        FxSpec::Highshelf(Default::default()),
        FxSpec::Split(Default::default()),
        FxSpec::Delay(Default::default()),
        FxSpec::Chorus(Default::default()),
        FxSpec::Reverb(Default::default()),
    ]
}

fn kind_of<E: Effect>(_: &E) -> &'static str {
    E::KIND
}
fn help_of<E: Effect>(_: &E) -> &'static str {
    E::HELP
}
fn params_of<E: Effect>(_: &E) -> &'static [ParamInfo] {
    E::PARAMS
}

impl FxSpec {
    pub fn kind(&self) -> &'static str {
        with_effect!(self, e => kind_of(e))
    }

    pub fn help(&self) -> &'static str {
        with_effect!(self, e => help_of(e))
    }

    pub fn params(&self) -> &'static [ParamInfo] {
        with_effect!(self, e => params_of(e))
    }

    pub fn get(&self, name: &str) -> Option<f32> {
        with_effect!(self, e => e.get(name))
    }

    /// Règle un paramètre ; refuse un nom inconnu ou une valeur hors plage, avec un message clair.
    pub fn set(&mut self, name: &str, v: f32) -> Result<()> {
        let kind = self.kind();
        let params = self.params();
        let Some(p) = params.iter().find(|p| p.name == name) else {
            let names: Vec<&str> = params.iter().map(|p| p.name).collect();
            return Err(Error::Fx(format!("{kind} : paramètre « {name} » inconnu ({})", names.join(", "))));
        };
        if !v.is_finite() || v < p.min || v > p.max {
            return Err(Error::Fx(format!(
                "{kind} : {name} = {v} hors plage ({} … {} {})",
                p.min,
                p.max,
                p.unit.symbol()
            )));
        }
        with_effect!(self, e => e.put(name, v));
        Ok(())
    }

    /// Durée de la queue après la fin du signal (s) : voir [`Effect::tail_seconds`].
    pub fn tail_seconds(&self) -> f32 {
        with_effect!(self, e => e.tail_seconds())
    }

    /// Construit l'étage (hors temps réel).
    pub fn build(&self, sample_rate: u32, max_block: usize) -> Result<Box<dyn Stage>> {
        with_effect!(self, e => e.build(sample_rate, max_block))
    }

    /// Réglages à éprouver par les tests génériques : les défauts, puis chaque paramètre à son
    /// minimum et à son maximum (les autres aux défauts).
    pub fn extremes(&self) -> Vec<FxSpec> {
        let base = FxSpec::default_of(self.kind()).expect("effet du catalogue");
        let mut out = vec![base.clone()];
        for p in self.params() {
            for v in [p.min, p.max] {
                let mut f = base.clone();
                f.set(p.name, v).expect("borne de la table");
                out.push(f);
            }
        }
        out
    }

    /// L'effet `kind` aux réglages par défaut.
    pub fn default_of(kind: &str) -> Result<FxSpec> {
        catalog().into_iter().find(|f| f.kind() == kind).ok_or_else(|| {
            let kinds: Vec<&str> = catalog().iter().map(|f| f.kind()).collect();
            Error::Fx(format!("effet « {kind} » inconnu ({})", kinds.join(", ")))
        })
    }
}

/// Mini-syntaxe `nom:param=valeur,param=valeur` (ex. `gain:db=-6`) ; seuls les paramètres qui
/// diffèrent du défaut sont écrits.
impl fmt::Display for FxSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind())?;
        let mut sep = ':';
        for p in self.params() {
            let v = self.get(p.name).unwrap_or(p.default);
            if v != p.default {
                write!(f, "{sep}{}={v}", p.name)?;
                sep = ',';
            }
        }
        Ok(())
    }
}

impl std::str::FromStr for FxSpec {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let (kind, args) = s.trim().split_once(':').unwrap_or((s.trim(), ""));
        let mut fx = FxSpec::default_of(kind.trim())?;
        for arg in args.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            let Some((k, v)) = arg.split_once('=') else {
                return Err(Error::Fx(format!("{} : « {arg} » : forme attendue param=valeur", fx.kind())));
            };
            let v: f32 = v.trim().parse().map_err(|_| {
                Error::Fx(format!("{} : {} = « {} » n'est pas un nombre", fx.kind(), k.trim(), v.trim()))
            })?;
            fx.set(k.trim(), v)?;
        }
        Ok(fx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chaque variante de `FxSpec` est dans le catalogue (le `match` force à compléter ce test).
    #[test]
    fn catalog_covers_every_variant() {
        let index = |f: &FxSpec| match f {
            FxSpec::Gain(_) => 0,
            FxSpec::Gate(_) => 1,
            FxSpec::Comp(_) => 2,
            FxSpec::Hpf(_) => 3,
            FxSpec::Lpf(_) => 4,
            FxSpec::Peak(_) => 5,
            FxSpec::Lowshelf(_) => 6,
            FxSpec::Highshelf(_) => 7,
            FxSpec::Split(_) => 8,
            FxSpec::Delay(_) => 9,
            FxSpec::Chorus(_) => 10,
            FxSpec::Reverb(_) => 11,
        };
        const COUNT: usize = 12;
        let mut seen = [false; COUNT];
        for f in catalog() {
            seen[index(&f)] = true;
        }
        assert!(seen.iter().all(|s| *s), "catalogue incomplet");
        let mut kinds: Vec<&str> = catalog().iter().map(|f| f.kind()).collect();
        kinds.dedup();
        assert_eq!(kinds.len(), catalog().len(), "noms d'effets en double");
    }

    /// Les tables de paramètres sont cohérentes (défaut dans la plage, pas positif, aide écrite).
    #[test]
    fn param_tables_are_consistent() {
        for f in catalog() {
            assert!(!f.help().is_empty());
            for p in f.params() {
                assert!(p.min <= p.default && p.default <= p.max, "{} {}", f.kind(), p.name);
                assert!(p.step > 0.0 && !p.help.is_empty(), "{} {}", f.kind(), p.name);
                assert_eq!(f.get(p.name), Some(p.default), "{} {} : défaut", f.kind(), p.name);
            }
        }
    }

    #[test]
    fn text_syntax_roundtrip_and_errors() {
        let fx: FxSpec = "gain:db=-6".parse().unwrap();
        assert_eq!(fx.get("db"), Some(-6.0));
        assert_eq!(fx.to_string(), "gain:db=-6");
        assert_eq!(fx.to_string().parse::<FxSpec>().unwrap(), fx);
        assert_eq!("gain".parse::<FxSpec>().unwrap(), FxSpec::default_of("gain").unwrap());
        assert_eq!(FxSpec::default_of("gain").unwrap().to_string(), "gain");
        for f in catalog() {
            assert_eq!(f.to_string().parse::<FxSpec>().unwrap(), f, "{}", f.kind());
        }
        let err = |s: &str| s.parse::<FxSpec>().unwrap_err().to_string();
        assert!(err("fuzz").contains("effet « fuzz » inconnu (gain"), "{}", err("fuzz"));
        assert!(err("gain:gain=3").contains("paramètre « gain » inconnu (db)"));
        assert!(err("gain:db=99").contains("hors plage"));
        assert!(err("gain:db=fort").contains("n'est pas un nombre"));
        assert!(err("gain:db").contains("param=valeur"));
    }

    #[test]
    fn toml_roundtrip() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct File {
            fx: Vec<FxSpec>,
        }
        let mut g = FxSpec::default_of("gain").unwrap();
        g.set("db", -3.5).unwrap();
        let file = File { fx: vec![g, FxSpec::default_of("gain").unwrap()] };
        let text = toml::to_string(&file).unwrap();
        assert!(text.contains("type = \"gain\""), "{text}");
        assert_eq!(toml::from_str::<File>(&text).unwrap(), file);
        // paramètre absent du fichier = défaut
        let partial: File = toml::from_str("[[fx]]\ntype = \"gain\"\n").unwrap();
        assert_eq!(partial.fx[0], FxSpec::default_of("gain").unwrap());
    }
}
