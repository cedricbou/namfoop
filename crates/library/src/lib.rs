//! Bibliothèque de modèles / IR : index avec identifiants courts, recherche, inventaire.
//!
//! Identifiant : une lettre (`m` modèle, `i` IR, `c` mélange d'IR) + 3 caractères base32 tirés d'un hash FNV-1a du
//! chemin relatif à la bibliothèque. Stable tant que le fichier ne bouge pas ; allongé seulement
//! en cas de collision avec un autre fichier du même type (rare).

/// La carte des mesures d'un morceau : section, mesure, temps, et le Song Position Pointer.
pub mod bars;
pub mod error;
/// L'échange de la bibliothèque entre machines : la valise sur la clé USB.
pub mod exchange;
/// Une ligne de grille d'accords (`{start_of_grid}`) : ses mesures, ses marges.
pub mod grid;
pub mod import;
pub mod instrument;
pub mod model_profile;
pub mod overrides;
pub mod profile;
/// Le prompteur d'un morceau : un fichier ChordPro lu, pas écrit.
pub mod prompt;
pub mod recommend;
pub mod set;

pub use error::{Error, Result};

use crate::error::bail;
use std::path::{Path, PathBuf};

/// Ordre « naturel » : les nombres sont comparés comme des nombres (Mix 2 < Mix 10), le texte
/// sans tenir compte de la casse.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn chunks(s: &str) -> Vec<(bool, String)> {
        let mut v: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let d = c.is_ascii_digit();
            match v.last_mut() {
                Some((kind, text)) if *kind == d => text.push(c),
                _ => v.push((d, c.to_string())),
            }
        }
        v
    }
    let (ca, cb) = (chunks(a), chunks(b));
    for ((da, ta), (db, tb)) in ca.iter().zip(&cb) {
        let o = if *da && *db {
            let (na, nb) = (ta.trim_start_matches('0'), tb.trim_start_matches('0'));
            na.len().cmp(&nb.len()).then_with(|| na.cmp(nb))
        } else {
            ta.to_lowercase().cmp(&tb.to_lowercase())
        };
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    ca.len().cmp(&cb.len()).then_with(|| a.cmp(b))
}

pub fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    for p in entries {
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            out.push(p);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Model,
    Ir,
    /// Mélange d'IR (`library/cabs/*.cab.toml`).
    Cab,
    /// Preset : une chaîne enregistrée (`library/presets/*.preset.toml`).
    Preset,
    /// Set de concert : des morceaux, chacun avec ses sons (`library/sets/*.set.toml`).
    Set,
}

impl Kind {
    fn prefix(self) -> char {
        match self {
            Kind::Model => 'm',
            Kind::Ir => 'i',
            Kind::Cab => 'c',
            Kind::Preset => 'p',
            Kind::Set => 's',
        }
    }
    fn ext(self) -> &'static str {
        match self {
            Kind::Model => "nam",
            Kind::Ir => "wav",
            Kind::Cab | Kind::Preset | Kind::Set => "toml",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Kind::Model => "modèle",
            Kind::Ir => "IR",
            Kind::Cab => "mélange",
            Kind::Preset => "preset",
            Kind::Set => "set",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: Kind,
    pub id: String,
    pub path: PathBuf,
    /// Chemin relatif à la bibliothèque (avec `/`).
    pub rel: String,
    /// Dossier du fichier sous `nam/`, `ir/` ou `cabs/` (le « pack », sous-dossiers compris :
    /// « PopPunkBass_NAMA2/DI »).
    pub pack: String,
    /// Nom de fichier sans extension.
    pub stem: String,
}

const B32: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn b32(mut h: u64, n: usize) -> String {
    (0..n)
        .map(|_| {
            let c = B32[(h & 31) as usize] as char;
            h >>= 5;
            c
        })
        .collect()
}

/// Mots d'un texte (alphanumériques), en minuscules : « Angel_v2 » -> [angel, v2].
pub fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

pub struct Library {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    /// Corrections de l'utilisateur (`library/overrides.toml`).
    pub overrides: crate::overrides::Overrides,
}

/// Pack d'un fichier : son dossier relatif, sans le premier niveau `nam/`, `ir/` ou `cabs/`.
fn pack_of(rel: &str) -> String {
    let mut parts: Vec<&str> = rel.split('/').collect();
    parts.pop();
    if parts.len() > 1 && ["nam", "ir", "cabs"].contains(&parts[0]) {
        parts.remove(0);
    }
    parts.join("/")
}

impl Library {
    pub fn scan(root: &Path) -> Self {
        let mut entries = Vec::new();
        for kind in [Kind::Model, Kind::Ir, Kind::Cab, Kind::Preset, Kind::Set] {
            let mut files = Vec::new();
            walk(root, kind.ext(), &mut files);
            // les enregistrements de calibration de mes instruments ne sont pas des IR
            let instruments = crate::instrument::dir(root);
            files.retain(|p| !p.starts_with(&instruments));
            // trois sortes de TOML dans la bibliothèque : les mélanges, les presets et les sets
            match kind {
                Kind::Cab => files.retain(|p| dsp::blend::is_cab_file(p)),
                Kind::Preset => files.retain(|p| dsp::preset::is_preset_file(p)),
                Kind::Set => files.retain(|p| crate::set::is_set_file(p)),
                _ => {}
            }
            let mut batch: Vec<Entry> = files
                .into_iter()
                .map(|path| {
                    let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                    let pack = pack_of(&rel);
                    let stem = match kind {
                        Kind::Preset => dsp::preset::display_name(&path),
                        Kind::Set => crate::set::display_name(&path),
                        _ => dsp::blend::display_name(&path),
                    };
                    Entry { kind, id: String::new(), path, rel, pack, stem }
                })
                .collect();
            assign_ids(&mut batch);
            entries.extend(batch);
        }
        let overrides = crate::overrides::Overrides::load(root).unwrap_or_else(|e| {
            log::warn!("{e:#} : corrections ignorées");
            Default::default()
        });
        Self { root: root.to_path_buf(), entries, overrides }
    }

    /// Métadonnées d'un modèle, corrections de l'utilisateur appliquées.
    pub fn meta(&self, path: &Path) -> Result<NamMeta> {
        let mut m = read_meta(path)?;
        if !self.overrides.is_empty() {
            if let Some(o) = self.overrides.model.get(&content_hash(path)?) {
                o.apply(&mut m);
            }
        }
        Ok(m)
    }

    /// Tous les fichiers dont le chemin contient tous les mots (pour les actions groupées).
    pub fn matches(&self, query: &str, kind: Kind) -> Vec<&Entry> {
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
        self.of_kind(kind).filter(|e| words.iter().all(|w| e.rel.to_lowercase().contains(w))).collect()
    }

    /// Chemin relatif à la bibliothèque (pour l'écrire dans un fichier de la bibliothèque).
    pub fn relative(&self, path: &Path) -> String {
        self.by_path(path).map(|e| e.rel.clone()).unwrap_or_else(|| path.display().to_string())
    }

    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.kind == kind)
    }

    pub fn by_path(&self, path: &Path) -> Option<&Entry> {
        let canon = |p: &Path| std::fs::canonicalize(p).ok();
        let target = canon(path)?;
        self.entries.iter().find(|e| canon(&e.path).as_ref() == Some(&target))
    }

    /// Résout une requête : identifiant, puis chemin existant, puis mots-clés.
    /// Si plusieurs fichiers contiennent tous les mots, on départage dans l'ordre :
    /// mots entiers dans le nom de fichier, expression exacte dans le nom, mots entiers dans le
    /// chemin. Le premier critère qui désigne un seul fichier l'emporte.
    pub fn resolve(&self, query: &str, kind: Kind) -> Result<PathBuf> {
        self.resolve_in(query, &[kind])
    }

    /// Pour `-i` : mélanges et IR.
    pub fn resolve_ir(&self, query: &str) -> Result<PathBuf> {
        self.resolve_in(query, &[Kind::Cab, Kind::Ir])
    }

    /// Un preset par son identifiant court ou ses mots-clés (`--preset`).
    pub fn resolve_preset(&self, query: &str) -> Result<PathBuf> {
        self.resolve_in(query, &[Kind::Preset])
    }

    /// Le preset chargé, prêt à jouer (chemins relatifs résolus dans la bibliothèque).
    pub fn preset(&self, path: &Path) -> Result<dsp::preset::PresetSpec> {
        let mut spec = dsp::preset::PresetSpec::load(path)?;
        // le fichier peut être ailleurs que dans `presets/` : la bibliothèque, c'est `self.root`
        spec.base = self.root.clone();
        Ok(spec)
    }

    /// Le chemin d'un son par son identifiant court : c'est le **filet** des presets quand un
    /// fichier a bougé (`PresetSpec::to_blocks`).
    pub fn path_of_id(&self, id: &str) -> Option<PathBuf> {
        self.entries.iter().find(|e| e.id.eq_ignore_ascii_case(id)).map(|e| e.path.clone())
    }

    /// Où enregistrer un preset : `library/presets/<nom>.preset.toml`.
    pub fn preset_path(&self, name: &str) -> PathBuf {
        self.root.join("presets").join(format!("{}.preset.toml", name.replace(['/', '\\'], "-")))
    }

    /// Un set par son identifiant court ou ses mots-clés (`run --set`).
    pub fn resolve_set(&self, query: &str) -> Result<PathBuf> {
        self.resolve_in(query, &[Kind::Set])
    }

    /// Le set chargé, contrôles bloquants appliqués (§ 3 de `docs/design/set.md`).
    pub fn set(&self, path: &Path) -> Result<crate::set::SetSpec> {
        crate::set::SetSpec::load(path)
    }

    /// Les avertissements d'un set, résolus contre **cette** bibliothèque : un preset introuvable
    /// ou un prompteur absent n'empêche pas le set de jouer, il marque le son (§ 3).
    pub fn set_warnings(&self, spec: &crate::set::SetSpec) -> Vec<crate::set::Warning> {
        spec.warnings(&|id| self.resolve_preset(id).is_ok(), &|p| p.is_file())
    }

    /// Le dossier des sets : `library/sets/`. C'est là que se résolvent les chemins de prompteur
    /// d'un set tout juste créé, qui n'a pas encore de fichier.
    pub fn sets_dir(&self) -> PathBuf {
        self.root.join("sets")
    }

    /// Où enregistrer un set : `library/sets/<nom>.set.toml`.
    pub fn set_path(&self, name: &str) -> PathBuf {
        self.root.join("sets").join(format!("{}.set.toml", name.replace(['/', '\\'], "-")))
    }

    fn resolve_in(&self, query: &str, kinds: &[Kind]) -> Result<PathBuf> {
        let kind = kinds[kinds.len() - 1];
        let pool = || self.entries.iter().filter(|e| kinds.contains(&e.kind));
        let q = query.trim();
        if let Some(e) = pool().find(|e| e.id.eq_ignore_ascii_case(q)) {
            return Ok(e.path.clone());
        }
        let p = PathBuf::from(q);
        if p.is_file() {
            return Ok(p);
        }
        let words: Vec<String> = q.to_lowercase().split_whitespace().map(String::from).collect();
        if words.is_empty() {
            bail!("requête vide");
        }
        let hits: Vec<&Entry> =
            pool().filter(|e| words.iter().all(|w| e.rel.to_lowercase().contains(w))).collect();
        if hits.len() > 1 {
            let qtok = tokens(q);
            let phrase = words.join(" ");
            let tiers: [&dyn Fn(&Entry) -> bool; 3] = [
                &|e| {
                    let t = tokens(&e.stem);
                    qtok.iter().all(|w| t.contains(w))
                },
                &|e| e.stem.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ").contains(&phrase),
                &|e| {
                    let t = tokens(&e.rel);
                    qtok.iter().all(|w| t.contains(w))
                },
            ];
            for tier in tiers {
                let narrowed: Vec<&Entry> = hits.iter().copied().filter(|e| tier(e)).collect();
                if narrowed.len() == 1 {
                    return Ok(narrowed[0].path.clone());
                }
            }
            // dernier départage, le cas « v2 » : un nom dont tous les mots se retrouvent dans
            // chacun des autres candidats (« Angel » ⊂ « Angel_v2 ») est la version de base.
            // « Ultra Lo » / « Ultra Hi » restent ambigus : aucun n'est inclus dans l'autre.
            let toks: Vec<Vec<String>> = hits.iter().map(|e| tokens(&e.stem)).collect();
            let base: Vec<usize> = (0..hits.len())
                .filter(|&i| (0..hits.len()).all(|j| j == i || toks[i].iter().all(|t| toks[j].contains(t))))
                .collect();
            if let [i] = base.as_slice() {
                return Ok(hits[*i].path.clone());
            }
        }
        match hits.as_slice() {
            [one] => Ok(one.path.clone()),
            [] => bail!(
                "aucun {} ne correspond à « {q} » dans {} (identifiants : `namfoop {}`)",
                kind.label(),
                self.root.display(),
                // les presets ont leur propre inventaire
                match kind {
                    Kind::Preset => "presets",
                    Kind::Set => "sets",
                    _ => "models",
                }
            ),
            many => {
                let list: Vec<String> =
                    many.iter().take(15).map(|e| format!("  {}  {}", e.id, e.rel)).collect();
                bail!(
                    "« {q} » est ambigu ({} {}s), précise ou donne l'identifiant :\n{}{}",
                    many.len(),
                    kind.label(),
                    list.join("\n"),
                    if many.len() > 15 { "\n  ..." } else { "" }
                )
            }
        }
    }
}

/// Identifiants : 3 caractères, allongés pour les entrées en collision (même type).
fn assign_ids(batch: &mut [Entry]) {
    let hashes: Vec<u64> = batch.iter().map(|e| fnv1a(&e.rel)).collect();
    let mut done = vec![false; batch.len()];
    for n in 3..=12 {
        let codes: Vec<String> = hashes.iter().map(|h| b32(*h, n)).collect();
        let mut count: std::collections::HashMap<&str, usize> = Default::default();
        for c in &codes {
            *count.entry(c).or_default() += 1;
        }
        for i in 0..batch.len() {
            if !done[i] && (count[codes[i].as_str()] == 1 || n == 12) {
                batch[i].id = format!("{}{}", batch[i].kind.prefix(), codes[i]);
                done[i] = true;
            }
        }
        if done.iter().all(|d| *d) {
            break;
        }
    }
}

/// Empreinte du contenu d'un fichier (FNV-1a 64 bits, hexadécimal) : identifie un modèle quel que
/// soit son nom ou son emplacement (corrections, doublons à l'import).
pub fn content_hash(path: &Path) -> Result<String> {
    Ok(bytes_hash(&std::fs::read(path)?))
}

pub fn bytes_hash(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Résolution d'un fichier de la bibliothèque : `ext` = "nam" (modèle) ou "wav" (IR ou mélange).
pub fn resolve(query: &str, library: &Path, ext: &str) -> Result<PathBuf> {
    let lib = Library::scan(library);
    if ext == "nam" {
        lib.resolve(query, Kind::Model)
    } else {
        lib.resolve_ir(query)
    }
}

/// Résumé d'un .nam lu depuis son JSON (et éventuellement corrigé, voir [`Library::meta`]).
#[derive(Clone, Debug, Default)]
pub struct NamMeta {
    pub name: String,
    pub gear_type: String,
    pub gear_make: String,
    pub gear_model: String,
    pub tone_type: String,
    pub sizes: Vec<String>,
    pub loudness: Option<f64>,
    /// Réglage d'entrée propre au modèle (dB, correction de l'utilisateur), ajouté à `--input-db`.
    pub input_db: Option<f32>,
    /// IR préférée pour ce modèle (chemin relatif à la bibliothèque, correction de l'utilisateur).
    pub ir: Option<String>,
    /// Champs corrigés par l'utilisateur (affichage).
    pub corrected: Vec<&'static str>,
}

/// Valeurs de remplissage laissées par certains outils d'entraînement : équivalent à « absent ».
pub fn placeholder(v: &str) -> bool {
    let l = v.trim().to_lowercase();
    l.is_empty()
        || ["null", "none", "unknown", "n/a", "-", "?"].contains(&l.as_str())
        || l.starts_with("t3k-null")
        || l.starts_with("tz-")
}

fn wavenet_channels(model: &serde_json::Value) -> String {
    let arch = model["architecture"].as_str().unwrap_or("?");
    let cfg = &model["config"];
    match arch {
        "WaveNet" => {
            let ch: Vec<String> = cfg["layers"]
                .as_array()
                .map(|l| l.iter().map(|x| x["channels"].to_string()).collect())
                .unwrap_or_default();
            format!("WaveNet {}ch", ch.join("/"))
        }
        "LSTM" => format!("LSTM {}h", cfg["hidden_size"]),
        a => a.to_string(),
    }
}

pub fn read_meta(path: &Path) -> Result<NamMeta> {
    let v: serde_json::Value = serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
    let m = &v["metadata"];
    let s = |k: &str| m[k].as_str().filter(|v| !placeholder(v)).unwrap_or("").trim().to_string();
    let arch = v["architecture"].as_str().unwrap_or("?");
    let sizes = if arch == "SlimmableContainer" {
        v["config"]["submodels"]
            .as_array()
            .map(|subs| subs.iter().map(|s| wavenet_channels(&s["model"])).collect())
            .unwrap_or_default()
    } else {
        vec![wavenet_channels(&v)]
    };
    Ok(NamMeta {
        name: s("name"),
        gear_type: s("gear_type"),
        gear_make: s("gear_make"),
        gear_model: s("gear_model"),
        tone_type: s("tone_type"),
        sizes,
        loudness: m["loudness"].as_f64(),
        ..Default::default()
    })
}

#[cfg(test)]
pub mod tests;
