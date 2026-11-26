//! La ligne `vérifié` du bilan d'un import : après l'import, chaque preset trouvera-t-il ses sons,
//! et chaque set ses presets ? Le plan est appliqué **en mémoire** — la liste des fichiers d'après,
//! chaque fichier lu là où il sera pris (la valise pour ce qui arrive, ici sinon) — puis chaque
//! preset et chaque set est résolu contre elle. Rien ne s'écrit.
//!
//! C'est un avertissement, pas un refus : le set s'ouvre et le reste joue, comme pour tout son
//! introuvable. N'est signalé que ce que l'import fait perdre — un son qui part, ou un preset qui
//! arrive sans les siens —, pas un défaut déjà là avant.

use std::collections::BTreeSet;
use std::path::Path;

use super::import::{ImportPlan, Op};
use super::Inventory;
use crate::Library;

/// Ce qu'un preset ou un set perdrait.
#[derive(Clone, Debug, PartialEq)]
pub struct Loss {
    /// Le preset ou le set : son nom.
    pub who: String,
    /// « un modèle qui part : Darkglass B7K v2 ».
    pub what: String,
}

/// Le verdict : les presets et les sets d'après l'import, et ce qu'ils perdraient.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Check {
    pub presets: usize,
    pub sets: usize,
    pub losses: Vec<Loss>,
}

/// Un son cité par un preset : son chemin dans la bibliothèque, et son identifiant court en filet.
#[derive(Clone, Debug, PartialEq)]
struct Cited {
    file: String,
    id: Option<String>,
}

/// Les sons d'un preset : toute table qui porte un `file` (un modèle, une IR, un mélange), où
/// qu'elle soit dans la chaîne — branches d'un parallèle comprises.
fn cited(v: &toml::Value, out: &mut Vec<Cited>) {
    match v {
        toml::Value::Table(t) => {
            if let Some(f) = t.get("file").and_then(|f| f.as_str()) {
                let id = t.get("id").and_then(|i| i.as_str()).map(String::from);
                out.push(Cited { file: f.into(), id });
            }
            t.values().for_each(|x| cited(x, out));
        }
        toml::Value::Array(a) => a.iter().for_each(|x| cited(x, out)),
        _ => {}
    }
}

/// Une bibliothèque d'un moment (avant, ou après l'import) : ses fichiers, et où lire chacun.
struct State<'a> {
    files: BTreeSet<String>,
    lib: Library,
    read: &'a dyn Fn(&str) -> Option<String>,
}

impl State<'_> {
    fn has(&self, c: &Cited) -> bool {
        self.files.contains(&c.file) || c.id.as_deref().is_some_and(|id| self.lib.path_of_id(id).is_some())
    }

    fn sounds(&self, rel: &str) -> Vec<Cited> {
        let mut out = Vec::new();
        if let Some(v) = (self.read)(rel).and_then(|t| toml::from_str::<toml::Value>(&t).ok()) {
            cited(&v, &mut out);
        }
        out
    }

    /// Les presets que cite un set et qu'il ne trouve pas.
    fn missing_presets(&self, rel: &str) -> Vec<String> {
        let Some(spec) = (self.read)(rel).and_then(|t| toml::from_str::<crate::set::SetSpec>(&t).ok()) else {
            return vec![];
        };
        let mut out: Vec<String> = Vec::new();
        for song in &spec.songs {
            for s in &song.sounds {
                if self.lib.resolve_preset(&s.preset).is_err() && !out.contains(&s.preset) {
                    out.push(s.preset.clone());
                }
            }
        }
        out
    }
}

fn what(file: &str, gone: bool) -> String {
    let noun = if super::suffix_ci(file, ".nam").is_some() {
        "un modèle"
    } else if super::suffix_ci(file, ".cab.toml").is_some() {
        "un mélange"
    } else {
        "une IR"
    };
    let name = file.rsplit('/').next().unwrap_or(file);
    let name = [".nam", ".wav", ".cab.toml"].iter().find_map(|s| super::suffix_ci(name, s)).unwrap_or(name);
    format!("{noun} {} : {name}", if gone { "qui part" } else { "introuvable" })
}

/// Vérifie le plan contre la bibliothèque d'ici (`here`, son inventaire `local`) et la valise.
pub fn verify(plan: &ImportPlan, here: &Path, suitcase: &Path, local: &Inventory) -> Check {
    let before_files: BTreeSet<String> = local.files.keys().cloned().collect();
    let mut after_files = before_files.clone();
    for op in &plan.ops {
        match op {
            Op::Remove(p) => {
                after_files.remove(p);
            }
            Op::Copy(p) | Op::Put(p, _) => {
                after_files.insert(p.clone());
            }
        }
    }
    let from_here = |rel: &str| std::fs::read_to_string(here.join(rel)).ok();
    let after_read = |rel: &str| -> Option<String> {
        match plan.ops.iter().find(|o| o.path() == rel) {
            Some(Op::Copy(_)) => std::fs::read_to_string(suitcase.join(rel)).ok(),
            Some(Op::Put(_, data)) => String::from_utf8(data.clone()).ok(),
            Some(Op::Remove(_)) => None,
            None => from_here(rel),
        }
    };
    let before = State {
        lib: Library::of_files(here, &before_files.iter().cloned().collect::<Vec<_>>()),
        files: before_files,
        read: &from_here,
    };
    let after = State {
        lib: Library::of_files(here, &after_files.iter().cloned().collect::<Vec<_>>()),
        files: after_files,
        read: &after_read,
    };
    let mut check = Check {
        presets: after.lib.of_kind(crate::Kind::Preset).count(),
        sets: after.lib.of_kind(crate::Kind::Set).count(),
        losses: vec![],
    };
    for p in after.lib.of_kind(crate::Kind::Preset) {
        let was = if before.files.contains(&p.rel) { before.sounds(&p.rel) } else { vec![] };
        for c in after.sounds(&p.rel) {
            if after.has(&c) {
                continue;
            }
            // un défaut déjà là avant l'import n'est pas une perte
            if was.contains(&c) && !before.has(&c) {
                continue;
            }
            check.losses.push(Loss { who: p.stem.clone(), what: what(&c.file, before.has(&c)) });
        }
    }
    for s in after.lib.of_kind(crate::Kind::Set) {
        let was = if before.files.contains(&s.rel) { before.missing_presets(&s.rel) } else { vec![] };
        for name in after.missing_presets(&s.rel) {
            if was.contains(&name) {
                continue;
            }
            let gone = before.lib.resolve_preset(&name).is_ok();
            let how = if gone { "qui part" } else { "introuvable" };
            check.losses.push(Loss { who: s.stem.clone(), what: format!("un preset {how} : {name}") });
        }
    }
    check
}
