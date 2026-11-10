//! Type de capture (baffle inclus ou non) : nom du fichier, puis dossiers, puis métadonnées.

use crate::recommend::{needs_ir, NeedsIr};
use crate::tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// Baffle inclus (amp_cab, full rig) : pas d'IR.
    Cab,
    /// Sans baffle (ampli seul, DI, pédale) : IR nécessaire.
    NoCab,
}

impl Capture {
    pub fn label(self) -> &'static str {
        match self {
            Capture::Cab => "baffle inclus",
            Capture::NoCab => "sans baffle",
        }
    }
    fn gear_type(self) -> &'static str {
        match self {
            Capture::Cab => "amp_cab",
            Capture::NoCab => "amp",
        }
    }
    pub fn cli(self) -> &'static str {
        match self {
            Capture::Cab => "amp-cab",
            Capture::NoCab => "amp",
        }
    }
    fn from_gear_type(t: &str) -> Option<Self> {
        match needs_ir(t) {
            NeedsIr::No => Some(Capture::Cab),
            NeedsIr::Yes => Some(Capture::NoCab),
            NeedsIr::Maybe => None,
        }
    }
}

pub(super) const CAB_WORDS: &[&str] =
    &["fullrig", "full rig", "amp cab", "ampcab", "with cab", "w cab", "amp and cab"];
pub(super) const NOCAB_WORDS: &[&str] =
    &["di", "nocab", "no cab", "amp only", "amponly", "without cab", "cabless", "no ir", "noir"];

pub(super) fn words_hint(text: &str) -> Option<(Capture, &'static str)> {
    let t = format!(" {} ", tokens(text).join(" "));
    let find = |list: &[&'static str]| list.iter().copied().find(|w| t.contains(&format!(" {w} ")));
    match (find(CAB_WORDS), find(NOCAB_WORDS)) {
        (Some(w), None) => Some((Capture::Cab, w)),
        (None, Some(w)) => Some((Capture::NoCab, w)),
        _ => None,
    }
}

/// Indice tiré du nom : le nom de fichier d'abord, puis ses dossiers (du plus proche au pack).
pub fn name_hint(stem: &str, dirs: &[&str]) -> Option<(Capture, String)> {
    if let Some((c, w)) = words_hint(stem) {
        return Some((c, format!("nom « {w} »")));
    }
    dirs.iter().rev().find_map(|d| words_hint(d).map(|(c, w)| (c, format!("dossier « {d} » ({w})"))))
}

/// Verdict sur le type de capture d'un modèle.
#[derive(Debug, PartialEq)]
pub struct Verdict {
    pub capture: Option<Capture>,
    /// D'où vient la décision.
    pub source: String,
    /// Correction à écrire (le type retenu contredit ou complète les métadonnées).
    pub correct: Option<&'static str>,
    /// Raison de vérifier à la main.
    pub doubt: Option<String>,
}

///
/// Le nom (fichier puis dossiers) prime sur les métadonnées : des packs vendus déclarent `amp`
/// pour des captures FullRig. Pas d'analyse du son : mesuré sur 66 captures basse, l'atténuation
/// des aigus ne distingue pas DI et FullRig (les baffles basse ont souvent un tweeter).
pub fn decide(meta_type: &str, hint: Option<(Capture, String)>) -> Verdict {
    let meta = Capture::from_gear_type(meta_type);
    let (capture, source) = match (hint, meta) {
        (Some((c, why)), _) => (Some(c), why),
        (None, Some(c)) => (Some(c), format!("métadonnées « {meta_type} »")),
        (None, None) => (None, String::new()),
    };
    let doubt =
        capture.is_none().then(|| "type de capture inconnu (ni le nom, ni les métadonnées)".to_string());
    let correct = capture.filter(|c| meta != Some(*c)).map(Capture::gear_type);
    Verdict { capture, source, correct, doubt }
}
