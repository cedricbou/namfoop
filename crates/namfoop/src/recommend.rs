//! Le modèle a-t-il besoin d'une IR, et laquelle conseiller ?
//!
//! `gear_type` des captures : `amp_cab` / `full_rig` = baffle inclus, `amp` / `pedal` / `preamp` =
//! sans baffle. Les IR sont classées par score : IR par défaut de la config, même marque que le
//! modèle (cherchée dans tous les champs : certains packs mettent le micro dans `gear_make`),
//! baffle basse plutôt que guitare, une seule IR par pack dans les premières suggestions.

use crate::library::{tokens, Entry, Kind, Library, NamMeta};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedsIr {
    Yes,
    No,
    Maybe,
}

pub fn needs_ir(gear_type: &str) -> NeedsIr {
    match gear_type.to_lowercase().as_str() {
        "amp" | "pedal" | "preamp" | "pedal_amp" => NeedsIr::Yes,
        "amp_cab" | "full_rig" | "amp_pedal_cab" | "cab" => NeedsIr::No,
        _ => NeedsIr::Maybe,
    }
}

const BRANDS: &[&str] = &[
    "ampeg",
    "fender",
    "mesa",
    "darkglass",
    "laney",
    "trace",
    "swr",
    "peavey",
    "markbass",
    "aguilar",
    "orange",
    "hartke",
    "eden",
    "ashdown",
    "gallien",
    "gk",
    "warwick",
    "marshall",
    "sunn",
    "acoustic",
];

/// Indices de baffle basse / guitare dans un nom d'IR.
const BASS_CAB: &[&str] =
    &["8x10", "810", "4x10", "410", "2x15", "215", "1x15", "115", "bass", "2x12", "212"];
const GUITAR_CAB: &[&str] = &["4x12", "412", "v30", "1x12", "greenback", "tweed", "4x12a"];

fn brands_in(text: &str) -> Vec<&'static str> {
    let t = tokens(text);
    // « Mesa_Boogie », « MESA _ BOOGIE » : mots entiers ; « dg212ne » ne doit pas compter
    BRANDS.iter().copied().filter(|b| t.iter().any(|w| w == b)).collect()
}

pub struct Suggestion<'a> {
    pub entry: &'a Entry,
    pub score: i32,
    pub reason: String,
}

/// IR conseillées pour ce modèle, meilleures d'abord (une par pack en tête de liste).
pub fn recommend<'a>(
    lib: &'a Library,
    model: &Entry,
    meta: &NamMeta,
    default_ir: Option<&str>,
) -> Vec<Suggestion<'a>> {
    let model_text =
        format!("{} {} {} {} {}", meta.gear_make, meta.gear_model, meta.name, model.pack, model.stem);
    let model_brands = brands_in(&model_text);
    let default = default_ir.and_then(|q| lib.resolve_ir(q).ok());
    let preferred = meta.ir.as_deref().and_then(|q| lib.resolve_ir(q).ok());
    let mut all: Vec<Suggestion> = lib
        .entries
        .iter()
        .filter(|e| matches!(e.kind, Kind::Ir | Kind::Cab))
        .map(|e| {
            let mut score = 0;
            let mut why = Vec::new();
            if e.kind == Kind::Cab {
                score += 100;
                why.push("ton mélange".to_string());
            }
            if preferred.as_deref() == Some(e.path.as_path()) {
                score += 2000;
                why.push("IR préférée du modèle".to_string());
            }
            if default.as_deref() == Some(e.path.as_path()) {
                score += 1000;
                why.push("IR par défaut (config)".to_string());
            }
            let ir_text = format!("{} {}", e.pack, e.stem);
            let ir_brands = brands_in(&ir_text);
            if let Some(b) = model_brands.iter().find(|b| ir_brands.contains(b)) {
                score += 50;
                why.push(format!("même marque ({b})"));
            }
            let t = tokens(&ir_text);
            let has = |list: &[&str]| list.iter().any(|k| t.iter().any(|w| w == k || w.contains(k)));
            if has(BASS_CAB) {
                score += 20;
                why.push("baffle basse".into());
            }
            if has(GUITAR_CAB) {
                score -= 30;
                why.push("baffle guitare".into());
            }
            Suggestion { entry: e, score, reason: why.join(", ") }
        })
        .collect();
    all.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.entry.rel.cmp(&b.entry.rel)));
    // diversité : la meilleure IR de chaque pack d'abord, puis le reste
    let mut seen = std::collections::HashSet::new();
    let (mut first, mut rest): (Vec<_>, Vec<_>) = (Vec::new(), Vec::new());
    for s in all {
        if seen.insert(s.entry.pack.clone()) {
            first.push(s);
        } else {
            rest.push(s);
        }
    }
    first.extend(rest);
    first
}

/// Ligne de conseil pour `models` et `run`.
pub fn advice(lib: &Library, model: &Entry, meta: &NamMeta, default_ir: Option<&str>) -> String {
    match needs_ir(&meta.gear_type) {
        NeedsIr::No => "IR : incluse dans la capture (pas d'IR à ajouter)".into(),
        need => {
            let sugg = recommend(lib, model, meta, default_ir);
            let head = if need == NeedsIr::Yes { "IR requise" } else { "IR : type de capture inconnu" };
            match sugg.first() {
                Some(s) => format!("{head} — conseillée : {} {} ({})", s.entry.id, s.entry.stem, s.reason),
                None => format!("{head} — aucune IR dans la bibliothèque"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::fake_library;

    fn meta(gear_type: &str, make: &str) -> NamMeta {
        NamMeta {
            name: make.into(),
            gear_type: gear_type.into(),
            gear_make: make.into(),
            gear_model: make.into(),
            tone_type: String::new(),
            sizes: vec![],
            loudness: None,
            ..Default::default()
        }
    }

    fn lib() -> (std::path::PathBuf, Library) {
        let d = fake_library(
            "reco",
            &[
                ("nam/MESA BASS 800D/MESA BASS 800D - MODERN.nam", "{}"),
                ("nam/ProCo Rat 2/ProCo Rat 2 - 10_2_9.nam", "{}"),
                ("ir/Mesa Boogie ST 4x12A V30/TF MESA ST 4X12A V30 57 2.wav", ""),
                ("ir/Mesa Powerhouse 410 & 410+215/Mesa Powerhouse 410 (SM57 + MD421).wav", ""),
                ("ir/Bass Ampeg SVT 8x10/Ampeg 8x10 57 AH.wav", ""),
                ("ir/Bass Ampeg SVT 8x10/Ampeg 8x10 4033 AH.wav", ""),
                ("ir/Darkglass DG212NE Cab IR/Darkglass DG212NE SM57.wav", ""),
                ("ir/Fender Tweed Pro/TF '56 TWEED PRO 1X15 C15N 57.wav", ""),
            ],
        );
        let l = Library::scan(&d);
        (d, l)
    }

    fn model<'a>(l: &'a Library, pack: &str) -> &'a Entry {
        l.of_kind(Kind::Model).find(|e| e.pack.contains(pack)).unwrap()
    }

    #[test]
    fn needs_ir_from_gear_type() {
        assert_eq!(needs_ir("amp"), NeedsIr::Yes);
        assert_eq!(needs_ir("pedal"), NeedsIr::Yes);
        assert_eq!(needs_ir("amp_cab"), NeedsIr::No);
        assert_eq!(needs_ir(""), NeedsIr::Maybe);
    }

    #[test]
    fn mesa_amp_gets_mesa_bass_cab_not_guitar_cab() {
        let (d, l) = lib();
        let s = recommend(&l, model(&l, "800D"), &meta("amp", "Mesa Boogie Bass 800D"), None);
        assert!(s[0].entry.stem.contains("Powerhouse 410"), "{}", s[0].entry.stem);
        let guitar = s.iter().position(|x| x.entry.stem.contains("4X12A")).unwrap();
        assert!(guitar > 2, "le 4x12 guitare n'est pas dans le trio de tête");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn pedal_without_brand_gets_bass_cabs_one_per_pack() {
        let (d, l) = lib();
        let s = recommend(&l, model(&l, "Rat"), &meta("pedal", "ProCo RAT 2"), None);
        let top3: Vec<&str> = s.iter().take(3).map(|x| x.entry.pack.as_str()).collect();
        let mut uniq = top3.clone();
        uniq.dedup();
        assert_eq!(uniq.len(), 3, "trois packs différents : {top3:?}");
        assert!(s.iter().take(3).all(|x| x.score > 0), "baffles basse d'abord");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn default_ir_from_config_comes_first() {
        let (d, l) = lib();
        let s = recommend(&l, model(&l, "Rat"), &meta("pedal", "ProCo"), Some("dg212ne sm57"));
        assert!(s[0].entry.stem.contains("DG212NE"), "{}", s[0].entry.stem);
        assert!(s[0].reason.contains("par défaut"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn advice_lines() {
        let (d, l) = lib();
        let m = model(&l, "800D");
        assert!(advice(&l, m, &meta("amp_cab", "x"), None).contains("incluse"));
        let a = advice(&l, m, &meta("amp", "Mesa"), None);
        assert!(a.starts_with("IR requise — conseillée : i"), "{a}");
        let _ = std::fs::remove_dir_all(d);
    }
}
