//! Collecte : zips (lus en mémoire), dossiers, fichiers ; fichiers parasites ignorés.

use crate::error::{bail, Context, Result};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileKind {
    Model,
    Ir,
    Doc,
}

impl FileKind {
    pub(super) fn of(path: &Path) -> Option<Self> {
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

    pub(super) fn dir(self) -> &'static str {
        match self {
            Self::Model => "nam",
            Self::Ir => "ir",
            Self::Doc => "docs",
        }
    }
}

/// Un fichier à importer : pack, chemin dans le pack, contenu.
pub(super) struct Incoming {
    pub(super) pack: String,
    pub(super) sub: PathBuf,
    pub(super) kind: FileKind,
    pub(super) data: Vec<u8>,
}

pub(super) fn clean_pack(name: &str) -> String {
    let s: String = name.trim().replace(['/', '\\'], "-");
    if s.is_empty() || s == "." || s == ".." {
        "import".into()
    } else {
        s
    }
}

/// Retire le dossier racine commun (zip qui contient « Pack/… »).
pub(super) fn strip_common_root(files: &mut [Incoming]) {
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

pub(super) fn from_zip(
    path: &Path,
    pack: &str,
    out: &mut Vec<Incoming>,
    skipped: &mut Vec<String>,
) -> Result<()> {
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

pub(super) fn walk_all(dir: &Path, out: &mut Vec<PathBuf>) {
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

pub(super) fn is_zip(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

pub(super) fn collect(
    src: &Path,
    pack: Option<&str>,
    out: &mut Vec<Incoming>,
    skipped: &mut Vec<String>,
) -> Result<()> {
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
