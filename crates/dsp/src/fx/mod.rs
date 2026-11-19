//! Effets : une seule description par effet, partagée par la CLI (`--pre` / `--post`, `namfoop fx`),
//! `browse`, les presets et les CC MIDI. Voir `docs/adr/0005-effets-et-chaines.md`.
//!
//! Un effet = des paramètres sérialisables (`FxSpec`, des données) + un [`Stage`] (le calcul, construit
//! par [`FxSpec::build`]). Chaque paramètre est décrit par un [`ParamInfo`] (nom, unité, plage, défaut,
//! pas, aide) : bornes vérifiées, aide et éditeurs générés depuis la même table.
//!
//! Ajouter un effet : un fichier qui implémente [`Effect`], une variante de [`FxSpec`] et une ligne
//! dans [`catalog`] (voir `crates/dsp/CLAUDE.md`, « Ajouter un effet »).
//!
//! Le **parallèle** n'est pas un effet : c'est un bloc de structure de la chaîne
//! (`crate::block::SplitSpec`, ADR 0006). Écrit en mini-syntaxe, il est refusé avec un message qui
//! dit où le décrire ([`STRUCTURAL_KINDS`]).

pub mod chorus;
pub mod comp;
pub mod delay;
pub mod filter;
pub mod gain;
pub mod gate;
pub mod reverb;

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::tempo::{self, Tempo};
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
    /// **Division rythmique** (ADR 0008) : la valeur est un **indice** dans
    /// [`tempo::DIVISIONS`], dont le 0 est `libre` — l'effet garde alors son temps en
    /// millisecondes. Un scalaire comme les autres (min 0, max [`tempo::max_index`], pas 1) ; seul
    /// son affichage diffère, voir [`Unit::format`].
    Division,
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
            Unit::Division => "note",
        }
    }

    /// La valeur d'un paramètre telle qu'on la **montre** : `-6 dB`, `400 ms`, et pour une
    /// division son étiquette (`1/8.`) plutôt que son indice, que personne ne veut compter.
    ///
    /// Un seul endroit pour cette règle : `namfoop fx`, le panneau de réglages de `browse` et la
    /// mini-syntaxe doivent tous dire la même chose.
    pub fn format(self, v: f32) -> String {
        if self == Unit::Division {
            return tempo::label_at(v).to_string();
        }
        let n = format!("{v:.2}");
        let n = n.trim_end_matches('0').trim_end_matches('.');
        format!("{n} {}", self.symbol())
    }
}

/// Ce que [`Effect::build`] a besoin de savoir, une fois pour toute la chaîne.
///
/// Un contexte plutôt que des arguments qui s'ajoutent un à un : le tempo est le premier réglage
/// que **plusieurs** étages dérivent d'une même valeur (ADR 0008), et le suivant (`set_param`)
/// voyagera par ici sans toucher aux onze effets.
#[derive(Clone, Debug)]
pub struct BuildCtx {
    pub sample_rate: u32,
    pub max_block: usize,
    /// Le tempo dont les paramètres `division` tirent leur durée : figé (`run`, `render`, les
    /// rendus de référence) ou **vivant** (`browse`).
    pub tempo: Tempo,
}

impl BuildCtx {
    /// Le contexte d'une chaîne sans tempo particulier : [`tempo::DEFAULT_BPM`], figé.
    pub fn new(sample_rate: u32, max_block: usize) -> Self {
        Self { sample_rate, max_block, tempo: Tempo::default() }
    }

    /// Le même contexte à un tempo donné.
    pub fn with_tempo(sample_rate: u32, max_block: usize, tempo: Tempo) -> Self {
        Self { sample_rate, max_block, tempo }
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
    fn build(&self, ctx: &BuildCtx) -> Result<Box<dyn Stage>>;
    /// Le paramètre qu'une division rythmique **remplace** quand elle agit (`time` pour le
    /// `delay`) : l'écran doit le montrer comme dérivé, et refuser de le régler (ADR 0008).
    /// Chaque effet le déclare — rien n'est deviné depuis les unités.
    fn division_target(&self) -> Option<&'static str> {
        None
    }
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
            FxSpec::Delay($e) => $body,
            FxSpec::Chorus($e) => $body,
            FxSpec::Reverb($e) => $body,
        }
    };
}

/// Un exemplaire par défaut de chaque effet : ce que proposent `namfoop fx`, le catalogue de
/// `browse` et les tests génériques. Le parallèle n'y est pas (c'est un bloc de structure).
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
        FxSpec::Delay(Default::default()),
        FxSpec::Chorus(Default::default()),
        FxSpec::Reverb(Default::default()),
    ]
}

/// Noms qui ne sont pas des effets mais des **blocs de structure** de la chaîne : refusés en
/// mini-syntaxe avec un message qui dit où les décrire, plutôt qu'un « effet inconnu » sec.
pub const STRUCTURAL_KINDS: &[&str] = &["split", "parallele", "parallèle"];

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

    /// L'unité d'un paramètre, pour l'afficher ou le lire (`None` : nom inconnu).
    pub fn unit_of(&self, name: &str) -> Option<Unit> {
        self.params().iter().find(|p| p.name == name).map(|p| p.unit)
    }

    /// La valeur d'un paramètre telle qu'on la montre (`1/8.`, `-6 dB`) ; `None` : nom inconnu.
    pub fn show(&self, name: &str) -> Option<String> {
        Some(self.unit_of(name)?.format(self.get(name)?))
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

    /// Quand une division rythmique agit : le paramètre qu'elle remplace et la durée (ms) qu'elle
    /// lui impose à ce tempo. `None` : aucune division réglée, le paramètre agit normalement.
    pub fn division_ms(&self, bpm: f32) -> Option<(&'static str, f32)> {
        let target = with_effect!(self, e => e.division_target())?;
        let p = self.params().iter().find(|p| p.unit == Unit::Division)?;
        let ms = tempo::at(self.get(p.name)?).ms_at(bpm)?;
        Some((target, ms))
    }

    /// Durée de la queue après la fin du signal (s) : voir [`Effect::tail_seconds`].
    pub fn tail_seconds(&self) -> f32 {
        with_effect!(self, e => e.tail_seconds())
    }

    /// Construit l'étage (hors temps réel).
    pub fn build(&self, ctx: &BuildCtx) -> Result<Box<dyn Stage>> {
        with_effect!(self, e => e.build(ctx))
    }

    /// Cet effet publie-t-il une **action** pour le synoptique de `browse` : la réduction
    /// de gain du compresseur, la fermeture du gate ?
    pub fn has_action_meter(&self) -> bool {
        matches!(self, FxSpec::Comp(_) | FxSpec::Gate(_))
    }

    /// Comme [`FxSpec::build`], mais branche le vumètre d'action sur les effets qui en publient un
    /// ([`FxSpec::has_action_meter`]) ; les autres l'ignorent.
    pub fn build_metered(
        &self,
        ctx: &BuildCtx,
        action: Option<&crate::meter::Meter>,
    ) -> Result<Box<dyn Stage>> {
        let Some(m) = action.filter(|_| self.has_action_meter()) else {
            return self.build(ctx);
        };
        Ok(match self {
            FxSpec::Comp(p) => Box::new(p.stage(ctx.sample_rate).with_meter(m.clone())),
            FxSpec::Gate(p) => Box::new(p.stage(ctx.sample_rate).with_meter(m.clone())),
            _ => return self.build(ctx),
        })
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
                // une division s'écrit par son nom de note : `delay:division=1/8.`
                if p.unit == Unit::Division {
                    write!(f, "{sep}{}={}", p.name, tempo::label_at(v))?;
                } else {
                    write!(f, "{sep}{}={v}", p.name)?;
                }
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
        let kind = kind.trim();
        if STRUCTURAL_KINDS.contains(&kind.to_lowercase().as_str()) {
            let what = "le parallèle n'est pas un effet mais un bloc de la chaîne : ses branches ne";
            let why = "se décrivent pas en texte, seulement dans un preset";
            let hint = "--crossover HZ pour garder le grave clair sous le son saturé";
            return Err(Error::Fx(format!("{kind} : {what} {why} ; {hint}")));
        }
        let mut fx = FxSpec::default_of(kind)?;
        for arg in args.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            let Some((k, v)) = arg.split_once('=') else {
                return Err(Error::Fx(format!("{} : « {arg} » : forme attendue param=valeur", fx.kind())));
            };
            let (k, v) = (k.trim(), v.trim());
            // une division s'écrit `1/8.` aussi bien que `4` : sans ça, il faut compter les crans
            let v: f32 = match fx.unit_of(k).filter(|u| *u == Unit::Division).and_then(|_| tempo::index_of(v))
            {
                Some(i) => i,
                None => v.parse().map_err(|_| {
                    let what = if fx.unit_of(k) == Some(Unit::Division) {
                        let labels: Vec<&str> = tempo::DIVISIONS.iter().map(|d| d.label).collect();
                        format!("n'est pas une division ({})", labels.join(", "))
                    } else {
                        "n'est pas un nombre".to_string()
                    };
                    Error::Fx(format!("{} : {k} = « {v} » {what}", fx.kind()))
                })?,
            };
            fx.set(k, v)?;
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
            FxSpec::Delay(_) => 8,
            FxSpec::Chorus(_) => 9,
            FxSpec::Reverb(_) => 10,
        };
        const COUNT: usize = 11;
        let mut seen = [false; COUNT];
        for f in catalog() {
            seen[index(&f)] = true;
        }
        assert!(seen.iter().all(|s| *s), "catalogue incomplet");
        let mut kinds: Vec<&str> = catalog().iter().map(|f| f.kind()).collect();
        kinds.dedup();
        assert_eq!(kinds.len(), catalog().len(), "noms d'effets en double");
    }

    /// Une division s'écrit et se relit par son **nom de note**, pas par son indice : personne ne
    /// veut compter les crans dans un exemple du guide (ADR 0008).
    #[test]
    fn a_division_is_written_and_read_as_a_note() {
        let fx: FxSpec = "delay:division=1/8.".parse().expect("division par étiquette");
        assert_eq!(fx.get("division"), Some(tempo::index_of("1/8.").unwrap()));
        // l'écriture rend l'étiquette, et le tour est complet
        assert_eq!(fx.to_string(), "delay:division=1/8.");
        assert_eq!(fx.to_string().parse::<FxSpec>().unwrap(), fx);
        assert_eq!(fx.show("division").as_deref(), Some("1/8."));

        // l'indice reste accepté : c'est le même paramètre scalaire
        assert_eq!("delay:division=4".parse::<FxSpec>().unwrap(), fx);
        // `libre` est le défaut, donc rien n'est écrit
        assert_eq!("delay:division=libre".parse::<FxSpec>().unwrap().to_string(), "delay");

        // une étiquette inconnue dit ce qui est accepté, au lieu de « n'est pas un nombre »
        let err = "delay:division=1/3".parse::<FxSpec>().expect_err("refusé").to_string();
        assert!(err.contains("1/8."), "{err}");
        assert!(err.contains("division"), "{err}");
        // et un paramètre ordinaire garde son message
        let err = "delay:time=vite".parse::<FxSpec>().expect_err("refusé").to_string();
        assert!(err.contains("n'est pas un nombre"), "{err}");
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

    /// Le parallèle n'est pas un effet : il n'est pas au catalogue, et écrit en mini-syntaxe il
    /// renvoie à l'endroit où ses branches se décrivent (ADR 0006).
    #[test]
    fn the_parallel_is_not_an_effect() {
        assert!(catalog().iter().all(|f| f.kind() != "split"), "split n'est pas un effet");
        for text in ["split", "split:freq=200", "Parallèle", "parallele:freq=150"] {
            let err = text.parse::<FxSpec>().unwrap_err().to_string();
            assert!(err.contains("bloc de la chaîne"), "{text} : {err}");
            assert!(err.contains("dans un preset") && err.contains("--crossover"), "{text} : {err}");
        }
        // un nom vraiment inconnu garde son message habituel
        let err = "fuzz".parse::<FxSpec>().unwrap_err().to_string();
        assert!(err.contains("effet « fuzz » inconnu"), "{err}");
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
