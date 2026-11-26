//! Ce que la question d'un élément à trancher montre de chaque version : son **résumé** (la chaîne
//! d'un preset, les morceaux d'un set, le trim d'une calibration…) et **ce qui diffère** entre les
//! deux, trois écarts au plus. Fonctions pures sur les fichiers relus des deux côtés : rien ne
//! s'écrit.

use super::{Element, Family};
use crate::instrument::Instrument;
use crate::overrides::Overrides;
use crate::Library;

/// Une version d'un élément : l'élément, et la bibliothèque qui le porte.
#[derive(Clone, Copy)]
pub struct Version<'a> {
    pub element: &'a Element,
    pub library: &'a Library,
}

fn text(v: &Version, file: &str) -> Option<String> {
    std::fs::read_to_string(v.library.root.join(file)).ok()
}

fn table(v: &Version) -> Option<toml::Table> {
    text(v, v.element.file()).and_then(|t| toml::from_str(&t).ok())
}

/// Le dB d'un nombre : « -17,5 dB ».
fn db(v: f32) -> String {
    format!("{v:+.1} dB").replace('.', ",")
}

/// La sous-clé d'un élément rangé dans un fichier qui en porte plusieurs.
fn sub(e: &Element) -> Option<&str> {
    e.path.split_once('#').map(|(_, k)| k)
}

fn calibration(v: &Version) -> Option<crate::instrument::Calibration> {
    let i: Instrument = toml::from_str(&text(v, v.element.file())?).ok()?;
    i.calibration.get(sub(v.element)?).cloned()
}

fn correction(v: &Version) -> Option<crate::overrides::ModelOverride> {
    let o: Overrides = toml::from_str(&text(v, v.element.file())?).ok()?;
    o.model.get(sub(v.element)?).cloned()
}

/// Le résumé d'une version.
pub fn summary(v: &Version) -> String {
    let e = v.element;
    match e.kind {
        Family::Preset => v.library.chain_line(&v.library.root.join(&e.path)),
        Family::Set => {
            let n =
                table(v).and_then(|t| t.get("song").and_then(|s| s.as_array()).map(Vec::len)).unwrap_or(0);
            format!("{n} {}", if n > 1 { "morceaux" } else { "morceau" })
        }
        Family::Calibration => match calibration(v) {
            Some(c) if c.skipped => "jouée sans calibrer".into(),
            Some(c) => {
                let mut s = c.trim_db.map_or_else(|| "sans trim".into(), |t| format!("trim {}", db(t)));
                if let Some(d) = c.date {
                    s.push_str(&format!(" · {d}"));
                }
                s
            }
            None => String::new(),
        },
        Family::Correction => correction(v).map(|o| fields(&o).join(" · ")).unwrap_or_default(),
        Family::Instrument => table(v)
            .map(|t| {
                let n = t.get("number").and_then(|n| n.as_integer()).unwrap_or(0);
                let d = t.get("default").and_then(|d| d.as_bool()).unwrap_or(false);
                format!("numéro {n}{}", if d { " · par défaut" } else { "" })
            })
            .unwrap_or_default(),
        _ => crate::exchange::human_size(e.size),
    }
}

/// Les champs posés d'une correction : « type amp_cab », « entrée -3,0 dB ».
fn fields(o: &crate::overrides::ModelOverride) -> Vec<String> {
    let mut v = Vec::new();
    let mut put = |label: &str, x: &Option<String>| {
        if let Some(x) = x {
            v.push(format!("{label} {x}"));
        }
    };
    put("type", &o.gear_type);
    put("marque", &o.gear_make);
    put("modèle", &o.gear_model);
    put("nom", &o.name);
    put("IR", &o.ir);
    if let Some(d) = o.input_db {
        v.push(format!("entrée {}", db(d)));
    }
    v
}

/// Ce qui diffère entre la version d'ici et celle de la valise, trois écarts au plus, puis le
/// compte des autres.
pub fn differs(here: &Version, there: &Version) -> String {
    let all = match here.element.kind {
        Family::Preset => preset_diff(here, there),
        Family::Set => set_diff(here, there),
        Family::Calibration => {
            let (a, b) = (calibration(here).unwrap_or_default(), calibration(there).unwrap_or_default());
            let mut v = Vec::new();
            if a.trim_db != b.trim_db {
                v.push(format!("trim {} ici, {} dans la valise", opt_db(a.trim_db), opt_db(b.trim_db)));
            }
            if a.skipped != b.skipped {
                v.push(if a.skipped {
                    "jouée sans calibrer ici".into()
                } else {
                    "jouée sans calibrer dans la valise".into()
                });
            }
            if a.wav != b.wav || (a.trim_db == b.trim_db && a.skipped == b.skipped) {
                v.push("enregistrement différent".into());
            }
            v
        }
        Family::Correction => {
            let (a, b) = (correction(here).map(|o| fields(&o)), correction(there).map(|o| fields(&o)));
            list_diff(&a.unwrap_or_default(), &b.unwrap_or_default())
        }
        _ => {
            let (a, b) = (here.element.size, there.element.size);
            if a == b {
                vec!["même taille, contenu différent".into()]
            } else {
                vec![format!(
                    "{} ici, {} dans la valise",
                    crate::exchange::human_size(a),
                    crate::exchange::human_size(b)
                )]
            }
        }
    };
    cap(all)
}

fn opt_db(v: Option<f32>) -> String {
    v.map_or_else(|| "aucun".into(), db)
}

/// Trois au plus, puis « … et 4 autres ».
fn cap(v: Vec<String>) -> String {
    let mut s = v.iter().take(3).cloned().collect::<Vec<_>>().join(" · ");
    if v.len() > 3 {
        s.push_str(&format!(" … et {} autres", v.len() - 3));
    }
    s
}

/// Ce qui n'est que d'un côté de deux listes.
fn list_diff(a: &[String], b: &[String]) -> Vec<String> {
    let mut v: Vec<String> = a.iter().filter(|x| !b.contains(x)).map(|x| format!("{x} ici")).collect();
    v.extend(b.iter().filter(|x| !a.contains(x)).map(|x| format!("{x} dans la valise")));
    v
}

fn set_diff(here: &Version, there: &Version) -> Vec<String> {
    let titles = |v: &Version| -> Vec<String> {
        table(v)
            .and_then(|t| t.get("song").and_then(|s| s.as_array()).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|s| s.get("title").and_then(|t| t.as_str()).map(String::from))
            .collect()
    };
    let (a, b) = (titles(here), titles(there));
    let mut v: Vec<String> =
        a.iter().filter(|x| !b.contains(x)).map(|x| format!("« {x} » ici seulement")).collect();
    v.extend(b.iter().filter(|x| !a.contains(x)).map(|x| format!("« {x} » dans la valise seulement")));
    if v.is_empty() {
        v.push(if a == b {
            "mêmes morceaux, réglages différents".into()
        } else {
            "ordre des morceaux".into()
        });
    }
    v
}

/// Le nom d'un bloc de preset, tel que la chaîne le montre : l'effet, ou la sorte de bloc.
fn block_label(b: &toml::Value) -> String {
    b.get("fx")
        .and_then(|f| f.get("type"))
        .and_then(|t| t.as_str())
        .or_else(|| b.get("kind").and_then(|k| k.as_str()))
        .unwrap_or("bloc")
        .to_string()
}

fn preset_diff(here: &Version, there: &Version) -> Vec<String> {
    let (Some(a), Some(b)) = (table(here), table(there)) else {
        return vec!["fichier illisible d'un côté".into()];
    };
    let mut v = Vec::new();
    let num =
        |t: &toml::Table, k: &str| t.get(k).and_then(|x| x.as_float().or(x.as_integer().map(|i| i as f64)));
    let (va, vb) = (num(&a, "volume_db").unwrap_or(0.0), num(&b, "volume_db").unwrap_or(0.0));
    if va != vb {
        v.push(format!("volume {} dans la valise", db((vb - va) as f32)));
    }
    if num(&a, "tempo_bpm") != num(&b, "tempo_bpm") {
        let t = |x: Option<f64>| x.map_or_else(|| "aucun".into(), |x| format!("{x}"));
        v.push(format!("tempo {} ici, {} dans la valise", t(num(&a, "tempo_bpm")), t(num(&b, "tempo_bpm"))));
    }
    let blocks = |t: &toml::Table| -> Vec<toml::Value> {
        t.get("block").and_then(|b| b.as_array()).cloned().unwrap_or_default()
    };
    let (ba, bb) = (blocks(&a), blocks(&b));
    let labels = |x: &[toml::Value]| x.iter().map(block_label).collect::<Vec<_>>();
    let (la, lb) = (labels(&ba), labels(&bb));
    for l in la.iter().filter(|l| !lb.contains(l)) {
        v.push(format!("{l} présent ici seulement"));
    }
    for l in lb.iter().filter(|l| !la.contains(l)) {
        v.push(format!("{l} présent dans la valise seulement"));
    }
    // un bloc présent des deux côtés : ses paramètres, un par un
    for x in &ba {
        let l = block_label(x);
        let Some(y) = bb.iter().find(|y| block_label(y) == l) else { continue };
        let mut flat_a = Vec::new();
        let mut flat_b = Vec::new();
        flatten("", x, &mut flat_a);
        flatten("", y, &mut flat_b);
        for (k, va) in &flat_a {
            match flat_b.iter().find(|(kb, _)| kb == k) {
                Some((_, vb)) if vb != va => v.push(format!("{l} {k} {va} ici, {vb} dans la valise")),
                None => v.push(format!("{l} {k} ici seulement")),
                _ => {}
            }
        }
        for (k, _) in flat_b.iter().filter(|(k, _)| !flat_a.iter().any(|(ka, _)| ka == k)) {
            v.push(format!("{l} {k} dans la valise seulement"));
        }
    }
    if v.is_empty() {
        v.push("même chaîne, écriture différente".into());
    }
    v
}

/// Les valeurs d'un bloc, par chemin (`fx.mix`), sans la sorte ni le type qui le nomment.
fn flatten(prefix: &str, v: &toml::Value, out: &mut Vec<(String, String)>) {
    match v {
        toml::Value::Table(t) => {
            for (k, x) in t {
                if (prefix.is_empty() && k == "kind") || (prefix == "fx" && k == "type") {
                    continue;
                }
                let p = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten(&p, x, out);
            }
        }
        x => out.push((prefix.strip_prefix("fx.").unwrap_or(prefix).to_string(), x.to_string())),
    }
}
