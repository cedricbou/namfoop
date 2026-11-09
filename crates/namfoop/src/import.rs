//! `namfoop import` : ajoute des packs (zip, dossiers, fichiers) à la bibliothèque, en vérifiant
//! chaque fichier et en devinant ce que les métadonnées ne disent pas (ou disent mal).
//!
//! 1. Collecte : zips (lus en mémoire, chemins hors archive refusés), dossiers (les zips qu'ils
//!    contiennent sont importés comme packs), fichiers. Fichiers parasites ignorés.
//! 2. Rangement : `library/nam/<pack>/…`, `library/ir/<pack>/…`, `library/docs/<pack>/…`
//!    (sous-dossiers du pack conservés). Les fichiers d'origine ne sont pas modifiés.
//! 3. Doublons : même contenu déjà dans la bibliothèque (ou plus haut dans l'import) -> ignoré.
//! 4. Validation : modèle chargé et joué (NaN, silence), IR lue (vide, silence, durée).
//! 5. Type de capture (baffle inclus ou non) : nom du fichier, puis dossiers, puis métadonnées.
//!    S'il contredit les métadonnées, la correction va dans `library/overrides.toml` ; s'il reste
//!    inconnu : « à vérifier » (`namfoop meta`).

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::library::{self, bytes_hash, tokens, Library};
use crate::overrides::{ModelOverride, Overrides};
use crate::recommend::{needs_ir, recommend, NeedsIr};

#[derive(Clone, Debug, clap::Args)]
pub struct ImportArgs {
    /// Zips, dossiers ou fichiers .nam / .wav à importer (un dossier de zips importe chaque zip)
    #[arg(required = true)]
    pub sources: Vec<PathBuf>,
    /// Nom du pack (dossier dans la bibliothèque) [défaut : nom du zip ou du dossier]
    #[arg(long)]
    pub pack: Option<String>,
    /// Vérifie et affiche le résultat sans rien écrire
    #[arg(long)]
    pub dry_run: bool,
    /// N'importe pas un fichier identique (octet par octet) à un fichier déjà dans la bibliothèque
    /// ou plus haut dans l'import. Par défaut il est importé quand même, pour que chaque pack
    /// reste complet, et signalé
    #[arg(long)]
    pub skip_duplicates: bool,
    /// Ouvre ensuite le navigateur sur les modèles importés, éditeur ouvert : écoute, type de
    /// capture, gain d'entrée, IR préférée (prend la carte son)
    #[arg(long)]
    pub review: bool,
    /// Dossier de la bibliothèque
    #[arg(long, default_value = "library")]
    pub library: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileKind {
    Model,
    Ir,
    Doc,
}

impl FileKind {
    fn of(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if name.starts_with('.')
            || name == "desktop.ini"
            || name == "thumbs.db"
            || path.components().any(|c| c.as_os_str() == "__MACOSX")
        {
            return None;
        }
        match path.extension()?.to_string_lossy().to_lowercase().as_str() {
            "nam" => Some(Self::Model),
            "wav" => Some(Self::Ir),
            "txt" | "pdf" | "md" | "rtf" | "html" => Some(Self::Doc),
            _ => None,
        }
    }

    fn dir(self) -> &'static str {
        match self {
            Self::Model => "nam",
            Self::Ir => "ir",
            Self::Doc => "docs",
        }
    }
}

/// Un fichier à importer : pack, chemin dans le pack, contenu.
struct Incoming {
    pack: String,
    sub: PathBuf,
    kind: FileKind,
    data: Vec<u8>,
}

fn clean_pack(name: &str) -> String {
    let s: String = name.trim().replace(['/', '\\'], "-");
    if s.is_empty() || s == "." || s == ".." {
        "import".into()
    } else {
        s
    }
}

/// Retire le dossier racine commun (zip qui contient « Pack/… »).
fn strip_common_root(files: &mut [Incoming]) {
    loop {
        let first = |f: &Incoming| f.sub.components().next().map(|c| c.as_os_str().to_owned());
        let Some(root) = files.first().and_then(first) else { return };
        if !files.iter().all(|f| f.sub.components().count() > 1 && first(f).as_ref() == Some(&root)) {
            return;
        }
        for f in files.iter_mut() {
            f.sub = f.sub.components().skip(1).collect();
        }
    }
}

fn from_zip(path: &Path, pack: &str, out: &mut Vec<Incoming>, skipped: &mut Vec<String>) -> Result<()> {
    let file = std::fs::File::open(path).with_context(|| format!("ouverture {}", path.display()))?;
    let mut zip =
        zip::ZipArchive::new(file).with_context(|| format!("{} : zip illisible", path.display()))?;
    let mut files = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let Some(sub) = f.enclosed_name() else {
            skipped.push(format!("{} (chemin hors de l'archive)", f.name_raw().escape_ascii()));
            continue;
        };
        let Some(kind) = FileKind::of(&sub) else { continue };
        let mut data = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut data).with_context(|| format!("{} : {}", path.display(), sub.display()))?;
        files.push(Incoming { pack: pack.to_string(), sub, kind, data });
    }
    strip_common_root(&mut files);
    out.extend(files);
    Ok(())
}

fn walk_all(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk_all(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn is_zip(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

fn collect(src: &Path, pack: Option<&str>, out: &mut Vec<Incoming>, skipped: &mut Vec<String>) -> Result<()> {
    let stem = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if src.is_dir() {
        let mut all = Vec::new();
        walk_all(src, &mut all);
        let name = clean_pack(pack.unwrap_or(&src.file_name().unwrap_or_default().to_string_lossy()));
        let mut files = Vec::new();
        for p in all {
            if is_zip(&p) {
                // un dossier de zips : chaque zip est un pack
                from_zip(&p, &clean_pack(pack.unwrap_or(&stem(&p))), out, skipped)?;
            } else if let Some(kind) = FileKind::of(&p) {
                let sub = p.strip_prefix(src).unwrap_or(&p).to_path_buf();
                files.push(Incoming { pack: name.clone(), sub, kind, data: std::fs::read(&p)? });
            }
        }
        out.extend(files);
    } else if is_zip(src) {
        from_zip(src, &clean_pack(pack.unwrap_or(&stem(src))), out, skipped)?;
    } else if src.is_file() {
        let Some(kind) = FileKind::of(src) else { bail!("{} : ni .nam, ni .wav, ni .zip", src.display()) };
        let parent = src.parent().and_then(|d| d.file_name()).map(|s| s.to_string_lossy().into_owned());
        out.push(Incoming {
            pack: clean_pack(pack.or(parent.as_deref()).unwrap_or("import")),
            sub: PathBuf::from(src.file_name().unwrap_or_default()),
            kind,
            data: std::fs::read(src)?,
        });
    } else {
        bail!("{} introuvable", src.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Type de capture

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// Baffle inclus (amp_cab, full rig) : pas d'IR.
    Cab,
    /// Sans baffle (ampli seul, DI, pédale) : IR nécessaire.
    NoCab,
}

impl Capture {
    fn label(self) -> &'static str {
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
    fn cli(self) -> &'static str {
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

const CAB_WORDS: &[&str] = &["fullrig", "full rig", "amp cab", "ampcab", "with cab", "w cab", "amp and cab"];
const NOCAB_WORDS: &[&str] =
    &["di", "nocab", "no cab", "amp only", "amponly", "without cab", "cabless", "no ir", "noir"];

fn words_hint(text: &str) -> Option<(Capture, &'static str)> {
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

// ---------------------------------------------------------------------------------------------
// Validation

const RATE: u32 = 48000;

struct ModelCheck {
    expected_rate: f64,
}

/// Charge le modèle et lui fait jouer un bruit à -30 dBFS RMS (niveau de jeu calibré).
fn check_model(path: &Path) -> Result<ModelCheck> {
    let mut m = nam_sys::Model::load(path)?;
    m.set_slimmable_size(1.0);
    let block = 512;
    m.reset(RATE as f64, block)?;
    let warm = RATE as usize / 4;
    let total = warm + RATE as usize * 3 / 2;
    let mut seed = 0x2545_f491u32;
    let mut uniform = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    // somme de 4 uniformes ~ gaussienne, écart-type 0,577 ; ramenée à -30 dBFS RMS
    let g = dsp::db_to_gain(-30.0) / 0.577;
    let input: Vec<f32> = (0..total).map(|_| (uniform() + uniform() + uniform() + uniform()) * g).collect();
    let mut out = vec![0.0f32; total];
    for (i, o) in input.chunks(block).zip(out.chunks_mut(block)) {
        m.process(i, o);
    }
    let tail = &out[warm..];
    if !tail.iter().all(|x| x.is_finite()) {
        bail!("sortie invalide (NaN/inf)");
    }
    if tail.iter().fold(0.0f32, |a, x| a.max(x.abs())) < 1e-5 {
        bail!("sortie muette");
    }
    Ok(ModelCheck { expected_rate: m.expected_sample_rate() })
}

fn check_ir(path: &Path) -> Result<String> {
    let (x, sr) = dsp::wav::read_mono(path)?;
    if x.is_empty() {
        bail!("fichier vide");
    }
    if !x.iter().all(|v| v.is_finite()) {
        bail!("échantillons invalides");
    }
    if x.iter().fold(0.0f32, |a, v| a.max(v.abs())) < 1e-5 {
        bail!("silence");
    }
    let secs = x.len() as f32 / sr as f32;
    if secs > 10.0 {
        bail!("{secs:.0} s : pas une IR (démo ?)");
    }
    Ok(format!("{sr} Hz, {:.0} ms", secs * 1000.0))
}

// ---------------------------------------------------------------------------------------------
// Commande

struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct PackReport {
    models: Vec<ModelLine>,
    irs: usize,
    docs: usize,
    /// Ignorés (`--skip-duplicates`) : identiques à un fichier existant.
    duplicates: Vec<String>,
    /// Importés bien qu'identiques à un autre fichier : (ce fichier, l'autre).
    twins: Vec<(String, String)>,
    /// Déjà à leur place avec le même contenu (réimport).
    already: usize,
    refused: Vec<String>,
    /// IR du pack (nouvelles ou déjà présentes), chemin dans la bibliothèque.
    bundled_irs: Vec<String>,
}

/// IR fournie par l'éditeur avec ses modèles : un mélange (« Mix ») de préférence, sinon la première.
fn bundled_choice(irs: &[String]) -> Option<&String> {
    let mut sorted: Vec<&String> = irs.iter().collect();
    sorted.sort();
    let stem = |r: &str| r.rsplit('/').next().unwrap_or(r).to_string();
    sorted.iter().find(|r| tokens(&stem(r)).iter().any(|t| t.starts_with("mix"))).or(sorted.first()).copied()
}

struct ModelLine {
    rel: String,
    hash: String,
    verdict: Verdict,
    note: Option<String>,
}

/// Importe ; renvoie les modèles ajoutés (chemins relatifs à la bibliothèque).
pub fn run(a: &ImportArgs, conf: &crate::config::LoadedConfig) -> Result<Vec<String>> {
    let mut incoming = Vec::new();
    let mut skipped = Vec::new();
    for s in &a.sources {
        collect(s, a.pack.as_deref(), &mut incoming, &mut skipped)?;
    }
    if incoming.is_empty() {
        bail!("rien à importer (ni .nam, ni .wav)");
    }

    // contenus connus : empreinte -> (chemin dans la bibliothèque, fichier de cet import s'il en vient).
    // L'empreinte trouve les candidats, la comparaison octet par octet confirme.
    let mut known: HashMap<String, (String, Option<usize>)> = HashMap::new();
    let mut existing = Vec::new();
    library::walk(&a.library, "nam", &mut existing);
    library::walk(&a.library, "wav", &mut existing);
    for p in &existing {
        if let Ok(h) = library::content_hash(p) {
            let rel = p.strip_prefix(&a.library).unwrap_or(p).to_string_lossy().replace('\\', "/");
            known.insert(h, (rel, None));
        }
    }
    let same_bytes = |data: &[u8], (rel, src): &(String, Option<usize>)| match src {
        Some(i) => incoming[*i].data == data,
        None => std::fs::read(a.library.join(rel)).is_ok_and(|b| b == data),
    };

    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let stage = Staging(std::env::temp_dir().join(format!(
        "namfoop-import-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    let mut overrides = Overrides::load(&a.library)?;
    let mut reports: Vec<(String, PackReport)> = Vec::new();
    let mut written = 0usize;

    for (index, f) in incoming.iter().enumerate() {
        if !reports.iter().any(|(p, _)| p == &f.pack) {
            reports.push((f.pack.clone(), PackReport::default()));
        }
        let rep = &mut reports.iter_mut().find(|(p, _)| p == &f.pack).expect("présent").1;
        let rel_path: PathBuf =
            [Path::new(f.kind.dir()), Path::new(&f.pack), &f.sub].iter().collect::<PathBuf>();
        let rel = rel_path.to_string_lossy().replace('\\', "/");
        if rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
            rep.refused.push(format!("{rel} : chemin invalide"));
            continue;
        }
        let dest = a.library.join(&rel_path);
        let hash = bytes_hash(&f.data);
        if f.kind != FileKind::Doc {
            if dest.exists() {
                if std::fs::read(&dest).is_ok_and(|b| b == f.data) {
                    rep.already += 1;
                    if f.kind == FileKind::Ir {
                        rep.bundled_irs.push(rel);
                    }
                } else {
                    rep.refused.push(format!("{rel} : un autre fichier porte déjà ce nom"));
                }
                continue;
            }
            if let Some(other) = known.get(&hash).filter(|k| same_bytes(&f.data, k)) {
                if a.skip_duplicates {
                    if f.kind == FileKind::Ir {
                        rep.bundled_irs.push(other.0.clone());
                    }
                    rep.duplicates.push(rel);
                    continue;
                }
                rep.twins.push((rel.clone(), other.0.clone()));
            }
        } else if dest.exists() {
            continue;
        }
        let staged = stage.0.join(&rel_path);
        std::fs::create_dir_all(staged.parent().expect("parent"))?;
        std::fs::write(&staged, &f.data)?;
        let mut note = None;
        match f.kind {
            FileKind::Model => match check_model(&staged) {
                Ok(c) => {
                    if (c.expected_rate - RATE as f64).abs() > 1.0 && c.expected_rate > 0.0 {
                        note = Some(format!("modèle entraîné à {} Hz", c.expected_rate));
                    }
                    let meta = library::read_meta(&staged)?;
                    let stem = f.sub.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    let mut dirs: Vec<String> = vec![f.pack.clone()];
                    if let Some(parent) = f.sub.parent() {
                        dirs.extend(parent.iter().map(|c| c.to_string_lossy().into_owned()));
                    }
                    let dirs_ref: Vec<&str> = dirs.iter().map(String::as_str).collect();
                    let verdict = decide(&meta.gear_type, name_hint(&stem, &dirs_ref));
                    rep.models.push(ModelLine { rel: rel.clone(), hash: hash.clone(), verdict, note });
                }
                Err(e) => {
                    rep.refused.push(format!("{rel} : {e:#}"));
                    continue;
                }
            },
            FileKind::Ir => match check_ir(&staged) {
                Ok(_) => {
                    rep.irs += 1;
                    rep.bundled_irs.push(rel.clone());
                }
                Err(e) => {
                    rep.refused.push(format!("{rel} : {e:#}"));
                    continue;
                }
            },
            FileKind::Doc => rep.docs += 1,
        }
        if f.kind != FileKind::Doc {
            known.entry(hash).or_insert((rel.clone(), Some(index)));
        }
        if !a.dry_run {
            std::fs::create_dir_all(dest.parent().expect("parent"))?;
            std::fs::copy(&staged, &dest).with_context(|| format!("copie vers {}", dest.display()))?;
            written += 1;
        }
    }

    // corrections (sans écraser celles de l'utilisateur) : type de capture, IR fournie par le pack
    let mut corrected = 0;
    for (_, rep) in &reports {
        let bundled = bundled_choice(&rep.bundled_irs);
        for m in &rep.models {
            if let (Some(ir), Some(Capture::NoCab)) = (bundled, m.verdict.capture) {
                if !overrides.model.get(&m.hash).is_some_and(|o| o.ir.is_some()) {
                    overrides.merge(
                        &m.hash,
                        ModelOverride {
                            file: Some(m.rel.clone()),
                            ir: Some(ir.clone()),
                            note: Some("import : IR fournie avec le pack".into()),
                            ..Default::default()
                        },
                    );
                }
            }
            if let Some(t) = m.verdict.correct {
                if overrides.model.get(&m.hash).is_some_and(|o| o.gear_type.is_some()) {
                    continue;
                }
                overrides.merge(
                    &m.hash,
                    ModelOverride {
                        file: Some(m.rel.clone()),
                        gear_type: Some(t.to_string()),
                        note: Some(format!("import : {}", m.verdict.source)),
                        ..Default::default()
                    },
                );
                corrected += 1;
            }
        }
    }
    if !a.dry_run && (corrected > 0 || reports.iter().any(|(_, r)| !r.bundled_irs.is_empty())) {
        overrides.save(&a.library)?;
    }

    print_report(a, conf, &reports, &skipped, corrected, written)?;
    Ok(reports.iter().flat_map(|(_, r)| r.models.iter().map(|m| m.rel.clone())).collect())
}

fn print_report(
    a: &ImportArgs,
    conf: &crate::config::LoadedConfig,
    reports: &[(String, PackReport)],
    skipped: &[String],
    corrected: usize,
    written: usize,
) -> Result<()> {
    let lib = (!a.dry_run).then(|| Library::scan(&a.library));
    // profils (IR puis modèles, avec ta basse) : calculés ici pour les nouveaux fichiers
    let profiles = lib.as_ref().map(|l| crate::profile::Profiles::load(l, |_| {}));
    let model_profiles = match (&lib, &profiles) {
        (Some(l), Some(p)) => Some(crate::model_profile::ModelProfiles::load(l, conf, p, |n| {
            if n >= 3 {
                eprintln!("profilage de {n} modèles avec ton enregistrement de calibration…")
            }
        })),
        _ => None,
    };
    let mut doubts = Vec::new();
    for (pack, r) in reports {
        println!(
            "[{pack}] modèles : {} nouveaux · IR : {} nouvelles · docs : {} · déjà présents : {} · refusés : {}",
            r.models.len(),
            r.irs,
            r.docs,
            r.already,
            r.refused.len()
        );
        for m in &r.models {
            let entry = lib.as_ref().and_then(|l| l.entries.iter().find(|e| e.rel == m.rel));
            let id = entry.map_or("-".to_string(), |e| e.id.clone());
            let name = m.rel.split_once(&format!("{pack}/")).map_or(m.rel.as_str(), |(_, s)| s);
            let v = &m.verdict;
            let mut what = match v.capture {
                Some(c) => format!("{} ({})", c.label(), v.source),
                None => "type ?".to_string(),
            };
            if v.correct.is_some() {
                what.push_str(" → corrigé");
            }
            if v.capture == Some(Capture::NoCab) {
                if let (Some(l), Some(e)) = (&lib, entry) {
                    if let Ok(meta) = l.meta(&e.path) {
                        if let Some(s) = recommend(l, e, &meta, None).first() {
                            what.push_str(&format!(" · IR conseillée {} {}", s.entry.id, s.entry.stem));
                        }
                    }
                }
            }
            if let Some(g) =
                model_profiles.as_ref().and_then(|p| p.by_rel.get(&m.rel)).and_then(|d| d.grain.as_ref())
            {
                what.push_str(&format!(" · {g}"));
            }
            if let Some(n) = &m.note {
                what.push_str(&format!(" · {n}"));
            }
            let mark = if v.doubt.is_some() { "⚠" } else { " " };
            println!("  {mark} {id:<6} {name}  — {what}");
            if let Some(d) = &v.doubt {
                let fix = match v.capture {
                    Some(c) => format!(
                        "--type {} (ou {})",
                        c.cli(),
                        if c == Capture::Cab { Capture::NoCab.cli() } else { Capture::Cab.cli() }
                    ),
                    None => "--type amp-cab (ou amp)".into(),
                };
                doubts.push(format!("{id} {name} : {d}\n      → namfoop meta {id} {fix}"));
            }
        }
        let short = |p: &str| p.split_once(&format!("{pack}/")).map_or(p, |(_, x)| x).to_string();
        if !r.twins.is_empty() {
            println!(
                "    {} fichier(s) identique(s) octet par octet à un autre, importé(s) pour garder le pack complet :",
                r.twins.len()
            );
            for (f, other) in r.twins.iter().take(5) {
                println!("      {}  =  {other}", short(f));
            }
            if r.twins.len() > 5 {
                println!("      … (+{})", r.twins.len() - 5);
            }
        }
        if !r.duplicates.is_empty() {
            let shown: Vec<String> = r.duplicates.iter().take(3).map(|d| short(d)).collect();
            let more = r.duplicates.len().saturating_sub(3);
            println!(
                "    ignorés (--skip-duplicates, contenu déjà présent) : {}{}",
                shown.join(", "),
                if more > 0 { format!(" … (+{more})") } else { String::new() }
            );
        }
        for d in &r.refused {
            println!("  ✗ refusé : {d}");
        }
    }
    for s in skipped {
        println!("  ✗ ignoré : {s}");
    }
    println!();
    if a.dry_run {
        println!("--dry-run : rien n'a été écrit.");
    } else {
        println!(
            "{written} fichier(s) ajouté(s) à {} ; {corrected} type(s) de capture corrigé(s) dans {}.",
            a.library.display(),
            Overrides::path(&a.library).display()
        );
    }
    if !doubts.is_empty() {
        println!(
            "\nÀ vérifier ({}) — écoute et corrige à l'écran (`namfoop browse`, touche e), ou :",
            doubts.len()
        );
        for d in doubts {
            println!("  {d}");
        }
    }
    if lib.is_some() {
        if let Some(p) = profiles.as_ref().filter(|p| p.computed > 0) {
            println!(
                "{} IR profilées (mini-spectre et étiquettes : `namfoop models`, `namfoop browse`).",
                p.computed
            );
        }
        if let Some(p) = model_profiles.as_ref().filter(|p| p.computed > 0) {
            println!(
                "{} modèles profilés {} (grain, mini-spectre, étiquettes).",
                p.computed,
                if p.synthetic {
                    "sur un signal de synthèse — lance `namfoop calibrate` pour les mesurer avec ta basse"
                } else {
                    "avec ta basse"
                }
            );
        }
        println!("\nSuite : `namfoop models` (identifiants, IR conseillées) ou `namfoop browse` (écoute).");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Kind;

    #[test]
    fn name_hints() {
        assert_eq!(name_hint("PPB_ChunkyBass_FullRig", &["DI"]).unwrap().0, Capture::Cab, "le nom prime");
        assert_eq!(name_hint("PPB_ChunkyBass_DI", &[]).unwrap().0, Capture::NoCab);
        assert_eq!(name_hint("ZABD_Wooley", &["ZeroAmps", "Full-Rig Profiles"]).unwrap().0, Capture::Cab);
        assert_eq!(name_hint("ZABD_Wooley DI", &["ZeroAmps", "DI Profiles"]).unwrap().0, Capture::NoCab);
        assert!(name_hint("Ampeg SVT - Diode", &["Ampeg"]).is_none(), "mot entier seulement");
        assert!(name_hint("MESA BASS 800D - MODERN", &["MESA _ BOOGIE BASS 800D"]).is_none());
    }

    #[test]
    fn verdicts() {
        let cab = |s: &str| Some((Capture::Cab, s.to_string()));
        // komposition101 : métadonnées « amp » sur un FullRig -> correction
        let v = decide("amp", cab("nom"));
        assert_eq!((v.capture, v.correct, v.doubt), (Some(Capture::Cab), Some("amp_cab"), None));
        // métadonnées cohérentes : rien à écrire
        let v = decide("amp_cab", None);
        assert_eq!((v.capture, v.correct, v.doubt), (Some(Capture::Cab), None, None));
        let v = decide("amp_cab", cab("nom"));
        assert_eq!(v.correct, None);
        // rien de connu -> à vérifier
        assert!(decide("", None).doubt.is_some());
    }

    fn zip_with(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        use std::io::Write;
        let p = dir.join(name);
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        for (n, data) in files {
            z.start_file(
                *n,
                zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
        p
    }

    fn ir_wav(dir: &Path, len: usize) -> Vec<u8> {
        let p = dir.join(format!("tmp-{len}.wav"));
        let ir: Vec<f32> = (0..len).map(|i| if i == 0 { 1.0 } else { 0.5 / i as f32 }).collect();
        dsp::wav::write_mono(&p, &ir, 48000).unwrap();
        std::fs::read(&p).unwrap()
    }

    #[test]
    fn imports_irs_skips_junk_duplicates_and_refuses_bad_files() {
        let lib = crate::library::tests::fake_library("import", &[]);
        let src = lib.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let a = ir_wav(&src, 2400);
        let b = ir_wav(&src, 4800);
        let z = zip_with(
            &src,
            "Pack K.zip",
            &[
                ("Pack K/Mixes/A.wav", &a),
                ("Pack K/Mixes/B.wav", &b),
                ("Pack K/Other/A copy.wav", &a),
                ("Pack K/desktop.ini", b"x"),
                ("Pack K/README.txt", b"lisez-moi"),
                ("Pack K/broken.nam", b"pas du json"),
            ],
        );
        let root = lib.join("library");
        let args = |dry| ImportArgs {
            sources: vec![z.clone()],
            pack: None,
            dry_run: dry,
            skip_duplicates: false,
            review: false,
            library: root.clone(),
        };
        let conf = crate::config::LoadedConfig::load(None, true).unwrap();
        run(&args(true), &conf).unwrap();
        assert!(!root.exists(), "--dry-run n'écrit rien");
        run(&args(false), &conf).unwrap();
        assert!(root.join("ir/Pack K/Mixes/A.wav").is_file(), "racine commune retirée, sous-dossiers gardés");
        assert!(root.join("ir/Pack K/Mixes/B.wav").is_file());
        assert!(root.join("ir/Pack K/Other/A copy.wav").is_file(), "identique mais importé : pack complet");
        assert!(root.join("docs/Pack K/README.txt").is_file());
        assert!(!root.join("nam/Pack K/broken.nam").exists(), "modèle illisible refusé");
        // réimport : tout est déjà en place, rien ne change
        run(&args(false), &conf).unwrap();
        let l = Library::scan(&root);
        assert_eq!(l.of_kind(Kind::Ir).count(), 3);
        assert_eq!(l.of_kind(Kind::Ir).next().unwrap().pack, "Pack K/Mixes");
        // --skip-duplicates : la copie identique n'est pas importée
        let root2 = lib.join("library2");
        let mut skip = args(false);
        skip.library = root2.clone();
        skip.skip_duplicates = true;
        run(&skip, &conf).unwrap();
        assert!(root2.join("ir/Pack K/Mixes/A.wav").is_file());
        assert!(!root2.join("ir/Pack K/Other/A copy.wav").exists());
    }

    #[test]
    fn real_models_are_validated_and_classified() {
        // modèle de démo de NeuralAmpModelerCore (pas de bibliothèque en CI)
        let model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models/wavenet.nam");
        assert!(model.is_file(), "sous-module NeuralAmpModelerCore absent");
        check_model(&model).unwrap();
        let lib = crate::library::tests::fake_library("import-nam", &[]);
        let src = lib.join("Mon Pack/Full-Rig");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::copy(&model, src.join("Tone_FullRig.nam")).unwrap();
        let root = lib.join("library");
        run(
            &ImportArgs {
                sources: vec![lib.join("Mon Pack")],
                pack: None,
                dry_run: false,
                skip_duplicates: false,
                review: false,
                library: root.clone(),
            },
            &crate::config::LoadedConfig::load(None, true).unwrap(),
        )
        .unwrap();
        let dest = root.join("nam/Mon Pack/Full-Rig/Tone_FullRig.nam");
        assert!(dest.is_file());
        let m = Library::scan(&root).meta(&dest).unwrap();
        assert_eq!((m.gear_type.as_str(), m.corrected.as_slice()), ("amp_cab", ["type"].as_slice()));
    }
}
