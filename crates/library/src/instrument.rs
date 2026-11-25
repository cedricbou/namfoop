//! Mes instruments (`docs/design/instruments.md` § 2, § 11, § 14) : un fichier
//! `library/instruments/<id>.toml` par instrument, ses enregistrements de calibration à côté. Ils
//! vivent dans la bibliothèque parce qu'elle se copie d'un ordinateur à l'autre, et qu'une
//! calibration reste vraie tant que l'instrument et la carte sont les mêmes.
//!
//! Une calibration vaut pour **un instrument sur une carte** : la table `[calibration.<carte>]` a
//! pour clé l'identifiant du profil de carte. Ce module ne choisit ni la carte ni l'instrument
//! joué : c'est la préparation du binaire. Il lit, contrôle, désigne et écrit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{bail, Context, Result};

/// Le dossier des instruments : `library/instruments/`.
pub fn dir(library: &Path) -> PathBuf {
    library.join("instruments")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Instrument {
    /// Le nom du fichier sans `.toml` : rempli à la lecture, jamais écrit.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    /// Sa ligne dans la grille, et plus tard son numéro au pied. Unique.
    pub number: u32,
    /// Un seul instrument le porte ; sans lui, c'est le plus petit numéro.
    #[serde(default, skip_serializing_if = "is_false")]
    pub default: bool,
    /// Une calibration par carte, par l'identifiant du profil de carte.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub calibration: BTreeMap<String, Calibration>,
}

/// La calibration d'un instrument sur une carte. `skipped` (« jouée sans ») n'a ni trim ni
/// enregistrement : c'est une réponse retenue, pas une mesure.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_db: Option<f32>,
    /// Crête robuste visée à l'entrée des modèles lors de la mesure (dBFS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_dbfs: Option<f32>,
    /// L'enregistrement, relatif au dossier des instruments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wav: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<toml::value::Datetime>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub skipped: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Calibration {
    /// Une mesure, pas une réponse « jouée sans ».
    pub fn measured(&self) -> bool {
        !self.skipped && self.trim_db.is_some()
    }
}

impl Instrument {
    /// « 1 BB 735A » : le numéro devant, comme dans la grille et à l'écran.
    pub fn label(&self) -> String {
        format!("{} {}", self.number, self.name)
    }

    /// La calibration **mesurée** sur cette carte.
    pub fn on_card(&self, card: &str) -> Option<&Calibration> {
        self.calibration.get(card).filter(|c| c.measured())
    }

    /// La mesure la plus récente, toutes cartes confondues (la date manquante passe en dernier) :
    /// la règle « hors carte » du § 4.
    pub fn latest(&self) -> Option<(&str, &Calibration)> {
        self.calibration
            .iter()
            .filter(|(_, c)| c.measured())
            .max_by(|a, b| a.1.date.map(|d| d.to_string()).cmp(&b.1.date.map(|d| d.to_string())))
            .map(|(k, c)| (k.as_str(), c))
    }

    /// Le nom de l'enregistrement de cet instrument sur cette carte.
    pub fn wav_name(&self, card: &str) -> String {
        format!("{}.{card}.wav", self.id)
    }

    /// Range une mesure pour cette carte ; elle efface une réponse « jouée sans ».
    pub fn set_calibration(&mut self, card: &str, trim_db: f32, target_dbfs: f32, date: &str) -> Result<()> {
        let date = date.parse().with_context(|| format!("date « {date} » illisible"))?;
        let wav = self.wav_name(card);
        self.calibration.insert(
            card.into(),
            Calibration {
                trim_db: Some(trim_db),
                target_dbfs: Some(target_dbfs),
                wav: Some(wav),
                date: Some(date),
                skipped: false,
            },
        );
        Ok(())
    }

    fn file(&self, library: &Path) -> PathBuf {
        dir(library).join(format!("{}.toml", self.id))
    }
}

/// Le chemin absolu d'un enregistrement (`wav` d'une calibration).
pub fn wav_path(library: &Path, wav: &str) -> PathBuf {
    dir(library).join(wav)
}

/// Tous les instruments, dans l'ordre de leurs numéros. Une bibliothèque sans dossier
/// `instruments/` n'en a aucun. Deux instruments du même numéro, ou deux par défaut, sont une
/// erreur qui nomme les deux fichiers.
pub fn load_all(library: &Path) -> Result<Vec<Instrument>> {
    let Ok(rd) = std::fs::read_dir(dir(library)) else {
        return Ok(vec![]);
    };
    let mut paths: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    let mut out: Vec<Instrument> = Vec::with_capacity(paths.len());
    for p in &paths {
        let text = std::fs::read_to_string(p).with_context(|| format!("lecture de {}", p.display()))?;
        let mut i: Instrument =
            toml::from_str(&text).with_context(|| format!("instrument {}", p.display()))?;
        i.id = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        out.push(i);
    }
    out.sort_by_key(|i| i.number);
    let file = |i: &Instrument| format!("{}.toml", i.id);
    for w in out.windows(2) {
        if w[0].number == w[1].number {
            bail!(
                "deux instruments portent le numéro {} : {} et {} (dans {})",
                w[0].number,
                file(&w[0]),
                file(&w[1]),
                dir(library).display()
            );
        }
    }
    let defaults: Vec<&Instrument> = out.iter().filter(|i| i.default).collect();
    if let [a, b, ..] = defaults[..] {
        bail!(
            "deux instruments par défaut : {} et {} (dans {}) — un seul porte `default = true`",
            file(a),
            file(b),
            dir(library).display()
        );
    }
    Ok(out)
}

/// L'instrument par défaut : celui qui porte `default`, sinon le plus petit numéro.
pub fn default_of(list: &[Instrument]) -> Option<&Instrument> {
    list.iter().find(|i| i.default).or_else(|| list.iter().min_by_key(|i| i.number))
}

/// Désigne un instrument comme `-m` et `-i` désignent un son : son numéro, son identifiant ou son
/// nom (casse ignorée), sinon quelques lettres — chaque mot doit se trouver dans le nom. Aucun ou
/// plusieurs : une erreur qui les liste.
pub fn find<'a>(list: &'a [Instrument], query: &str) -> Result<&'a Instrument> {
    match find_opt(list, query)? {
        Some(i) => Ok(i),
        None if list.is_empty() => bail!(
            "aucun instrument « {query} » : la bibliothèque n'en a aucun. On en crée un avec \
             `namfoop calibrate --instrument \"<nom>\"`"
        ),
        None => bail!("aucun instrument « {query} ». Mes instruments : {}", labels(list.iter())),
    }
}

fn labels<'a>(v: impl Iterator<Item = &'a Instrument>) -> String {
    v.map(|i| i.label()).collect::<Vec<_>>().join(", ")
}

/// Comme [`find`], mais aucun instrument désigné n'est pas une erreur (`calibrate` le crée) ;
/// plusieurs le restent.
pub fn find_opt<'a>(list: &'a [Instrument], query: &str) -> Result<Option<&'a Instrument>> {
    let q = query.trim().to_lowercase();
    if let Ok(n) = q.parse::<u32>() {
        if let Some(i) = list.iter().find(|i| i.number == n) {
            return Ok(Some(i));
        }
    }
    if let Some(i) = list.iter().find(|i| i.id.to_lowercase() == q || i.name.to_lowercase() == q) {
        return Ok(Some(i));
    }
    let words: Vec<&str> = q.split_whitespace().collect();
    let hits: Vec<&Instrument> = list
        .iter()
        .filter(|i| {
            let hay = format!("{} {}", i.name.to_lowercase(), i.id.to_lowercase());
            !words.is_empty() && words.iter().all(|w| hay.contains(w))
        })
        .collect();
    match hits[..] {
        [one] => Ok(Some(one)),
        [] => Ok(None),
        _ => bail!("« {query} » désigne plusieurs instruments : {}. Précise", labels(hits.into_iter())),
    }
}

/// Un identifiant de fichier tiré d'un nom : minuscules, chiffres, tirets.
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "instrument".into()
    } else {
        s
    }
}

/// Un instrument neuf : le premier numéro libre, un identifiant libre, et par défaut s'il est le
/// premier (§ 11). Rien n'est écrit.
pub fn create(list: &[Instrument], name: &str) -> Instrument {
    let number = (1..).find(|n| list.iter().all(|i| i.number != *n)).unwrap_or(1);
    let base = slug(name);
    let mut id = base.clone();
    let mut k = 2;
    while list.iter().any(|i| i.id == id) {
        id = format!("{base}-{k}");
        k += 1;
    }
    Instrument {
        id,
        name: name.trim().into(),
        number,
        default: list.is_empty(),
        calibration: BTreeMap::new(),
    }
}

/// Écrit un instrument (crée le dossier), et rend le chemin du fichier.
pub fn save(library: &Path, i: &Instrument) -> Result<PathBuf> {
    let p = i.file(library);
    std::fs::create_dir_all(dir(library))
        .with_context(|| format!("création de {}", dir(library).display()))?;
    let text = toml::to_string(i)?;
    std::fs::write(&p, text).with_context(|| format!("écriture de {}", p.display()))?;
    Ok(p)
}

/// Fait de `id` l'instrument par défaut, et retire la marque des autres : rend les instruments
/// modifiés, à écrire.
pub fn make_default(list: &[Instrument], id: &str) -> Vec<Instrument> {
    list.iter()
        .filter(|i| i.default != (i.id == id))
        .map(|i| Instrument { default: i.id == id, ..i.clone() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fake_library;

    const BB: &str = r#"name = "BB 735A"
number = 1
default = true

[calibration.itwo]
trim_db = -17.5
target_dbfs = -30.0
wav = "bb-735a.itwo.wav"
date = 2026-11-24

[calibration.umc404hd]
skipped = true
"#;

    /// Un instrument relu à l'identique après écriture, deux cartes dans un même fichier.
    #[test]
    fn round_trip_keeps_every_card() {
        let lib = fake_library("instr-rt", &[("instruments/bb-735a.toml", BB)]);
        let list = load_all(&lib).unwrap();
        assert_eq!(list.len(), 1);
        let bb = &list[0];
        assert_eq!(bb.id, "bb-735a");
        assert_eq!(bb.on_card("itwo").and_then(|c| c.trim_db), Some(-17.5));
        assert!(bb.on_card("umc404hd").is_none(), "« jouée sans » n'est pas une mesure");
        assert!(bb.calibration["umc404hd"].skipped);
        save(&lib, bb).unwrap();
        assert_eq!(std::fs::read_to_string(lib.join("instruments/bb-735a.toml")).unwrap(), BB);
        assert_eq!(load_all(&lib).unwrap()[0], *bb);
    }

    /// Les enregistrements de calibration vivent dans la bibliothèque, mais ne sont pas des IR.
    #[test]
    fn calibration_recordings_are_not_irs() {
        let lib = fake_library(
            "instr-scan",
            &[
                ("ir/pack/cab.wav", ""),
                ("instruments/bb-735a.itwo.wav", ""),
                ("instruments/bb-735a.toml", BB),
            ],
        );
        let scanned = crate::Library::scan(&lib);
        let irs: Vec<&str> = scanned.of_kind(crate::Kind::Ir).map(|e| e.rel.as_str()).collect();
        assert_eq!(irs, ["ir/pack/cab.wav"]);
    }

    #[test]
    fn two_defaults_are_refused_naming_both_files() {
        let lib = fake_library(
            "instr-2def",
            &[
                ("instruments/a.toml", "name = \"A\"\nnumber = 1\ndefault = true\n"),
                ("instruments/b.toml", "name = \"B\"\nnumber = 2\ndefault = true\n"),
            ],
        );
        let e = load_all(&lib).unwrap_err().to_string();
        assert!(e.contains("a.toml") && e.contains("b.toml") && e.contains("par défaut"), "{e}");
    }

    #[test]
    fn two_instruments_with_one_number_are_refused() {
        let lib = fake_library(
            "instr-2num",
            &[
                ("instruments/a.toml", "name = \"A\"\nnumber = 3\n"),
                ("instruments/b.toml", "name = \"B\"\nnumber = 3\n"),
            ],
        );
        let e = load_all(&lib).unwrap_err().to_string();
        assert!(e.contains("a.toml") && e.contains("b.toml") && e.contains("numéro 3"), "{e}");
    }

    fn three() -> Vec<Instrument> {
        let mut v = vec![];
        for name in ["BB 735A", "Fretless", "Fretless 5"] {
            let i = create(&v, name);
            v.push(i);
        }
        v
    }

    #[test]
    fn created_instruments_take_free_numbers_and_the_first_is_default() {
        let v = three();
        assert_eq!(v.iter().map(|i| i.number).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(
            v.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["bb-735a", "fretless", "fretless-5"]
        );
        assert!(v[0].default && !v[1].default);
        assert_eq!(default_of(&v).unwrap().number, 1);
        // sans marque, le plus petit numéro
        let none: Vec<_> = v.iter().map(|i| Instrument { default: false, ..i.clone() }).collect();
        assert_eq!(default_of(&none).unwrap().number, 1);
    }

    #[test]
    fn find_by_number_name_or_a_few_letters() {
        let v = three();
        assert_eq!(find(&v, "2").unwrap().name, "Fretless");
        assert_eq!(find(&v, "bb 735a").unwrap().number, 1);
        assert_eq!(find(&v, "735").unwrap().number, 1);
        // le nom exact départage « Fretless » de « Fretless 5 »
        assert_eq!(find(&v, "fretless").unwrap().number, 2);
        let e = find(&v, "fret").unwrap_err().to_string();
        assert!(e.contains("plusieurs") && e.contains("2 Fretless") && e.contains("3 Fretless 5"), "{e}");
        let e = find(&v, "guitare").unwrap_err().to_string();
        assert!(e.contains("1 BB 735A"), "{e}");
        let e = find(&[], "x").unwrap_err().to_string();
        assert!(e.contains("calibrate --instrument"), "{e}");
    }

    #[test]
    fn a_new_measure_replaces_played_without() {
        let mut i = create(&[], "Active");
        i.calibration.insert("mtk".into(), Calibration { skipped: true, ..Default::default() });
        i.set_calibration("mtk", -12.0, -30.0, "2026-11-25").unwrap();
        let c = i.on_card("mtk").unwrap();
        assert_eq!(c.wav.as_deref(), Some("active.mtk.wav"));
        assert!(!c.skipped);
        assert_eq!(i.latest().unwrap().0, "mtk");
    }

    #[test]
    fn latest_is_the_most_recent_measure() {
        let mut i = create(&[], "X");
        i.set_calibration("a", -1.0, -30.0, "2026-11-20").unwrap();
        i.set_calibration("b", -2.0, -30.0, "2026-11-24").unwrap();
        i.calibration.insert("c".into(), Calibration { skipped: true, ..Default::default() });
        assert_eq!(i.latest().unwrap().0, "b");
    }

    #[test]
    fn make_default_moves_the_mark() {
        let v = three();
        let changed = make_default(&v, "fretless");
        assert_eq!(changed.len(), 2);
        assert!(changed.iter().any(|i| i.id == "bb-735a" && !i.default));
        assert!(changed.iter().any(|i| i.id == "fretless" && i.default));
    }
}
