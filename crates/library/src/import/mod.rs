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

use crate::error::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::overrides::{ModelOverride, Overrides};
use crate::{bytes_hash, tokens};

/// Options d'import (les options de la ligne de commande sont dans le binaire).
#[derive(Clone, Debug)]
pub struct ImportArgs {
    /// Zips, dossiers ou fichiers .nam / .wav à importer.
    pub sources: Vec<PathBuf>,
    /// Nom du pack (dossier dans la bibliothèque) ; défaut : nom du zip ou du dossier.
    pub pack: Option<String>,
    /// Vérifie sans rien écrire.
    pub dry_run: bool,
    /// N'importe pas un fichier identique à un fichier déjà présent ou plus haut dans l'import.
    pub skip_duplicates: bool,
    /// Dossier de la bibliothèque.
    pub library: PathBuf,
}

mod classify;
mod scan;
mod validate;

pub use classify::{decide, name_hint, Capture, Verdict};
use scan::*;
use validate::*;

// ---------------------------------------------------------------------------------------------
// La commande : zone de travail jetable, bilan par pack, et le déroulé de `run`

struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Bilan d'un pack.
#[derive(Default)]
pub struct PackReport {
    pub models: Vec<ModelLine>,
    pub irs: usize,
    pub docs: usize,
    /// Ignorés (`--skip-duplicates`) : identiques à un fichier existant.
    pub duplicates: Vec<String>,
    /// Importés bien qu'identiques à un autre fichier : (ce fichier, l'autre).
    pub twins: Vec<(String, String)>,
    /// Déjà à leur place avec le même contenu (réimport).
    pub already: usize,
    pub refused: Vec<String>,
    /// IR du pack (nouvelles ou déjà présentes), chemin dans la bibliothèque.
    pub bundled_irs: Vec<String>,
}

/// IR fournie par l'éditeur avec ses modèles : un mélange (« Mix ») de préférence, sinon la première.
fn bundled_choice(irs: &[String]) -> Option<&String> {
    let mut sorted: Vec<&String> = irs.iter().collect();
    sorted.sort();
    let stem = |r: &str| r.rsplit('/').next().unwrap_or(r).to_string();
    sorted.iter().find(|r| tokens(&stem(r)).iter().any(|t| t.starts_with("mix"))).or(sorted.first()).copied()
}

/// Modèle importé.
pub struct ModelLine {
    pub rel: String,
    pub hash: String,
    pub verdict: Verdict,
    pub note: Option<String>,
}

/// Résultat d'un import, affiché par le binaire.
pub struct Outcome {
    /// (pack, bilan), dans l'ordre de l'import.
    pub reports: Vec<(String, PackReport)>,
    /// Sources ignorées (fichiers parasites...).
    pub skipped: Vec<String>,
    /// Types de capture corrigés dans `overrides.toml`.
    pub corrected: usize,
    /// Fichiers écrits dans la bibliothèque.
    pub written: usize,
}

impl Outcome {
    /// Modèles ajoutés (chemins relatifs à la bibliothèque).
    pub fn imported_models(&self) -> Vec<String> {
        self.reports.iter().flat_map(|(_, r)| r.models.iter().map(|m| m.rel.clone())).collect()
    }
}

/// Importe et renvoie le bilan (les profils sont calculés à l'affichage du bilan, par le binaire).
pub fn run(a: &ImportArgs) -> Result<Outcome> {
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
    crate::walk(&a.library, "nam", &mut existing);
    crate::walk(&a.library, "wav", &mut existing);
    for p in &existing {
        if let Ok(h) = crate::content_hash(p) {
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
                    let meta = crate::read_meta(&staged)?;
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

    Ok(Outcome { reports, skipped, corrected, written })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, Library};

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
        let lib = crate::tests::fake_library("import", &[]);
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
            library: root.clone(),
        };
        run(&args(true)).unwrap();
        assert!(!root.exists(), "--dry-run n'écrit rien");
        run(&args(false)).unwrap();
        assert!(root.join("ir/Pack K/Mixes/A.wav").is_file(), "racine commune retirée, sous-dossiers gardés");
        assert!(root.join("ir/Pack K/Mixes/B.wav").is_file());
        assert!(root.join("ir/Pack K/Other/A copy.wav").is_file(), "identique mais importé : pack complet");
        assert!(root.join("docs/Pack K/README.txt").is_file());
        assert!(!root.join("nam/Pack K/broken.nam").exists(), "modèle illisible refusé");
        // réimport : tout est déjà en place, rien ne change
        run(&args(false)).unwrap();
        let l = Library::scan(&root);
        assert_eq!(l.of_kind(Kind::Ir).count(), 3);
        assert_eq!(l.of_kind(Kind::Ir).next().unwrap().pack, "Pack K/Mixes");
        // --skip-duplicates : la copie identique n'est pas importée
        let root2 = lib.join("library2");
        let mut skip = args(false);
        skip.library = root2.clone();
        skip.skip_duplicates = true;
        run(&skip).unwrap();
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
        let lib = crate::tests::fake_library("import-nam", &[]);
        let src = lib.join("Mon Pack/Full-Rig");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::copy(&model, src.join("Tone_FullRig.nam")).unwrap();
        let root = lib.join("library");
        run(&ImportArgs {
            sources: vec![lib.join("Mon Pack")],
            pack: None,
            dry_run: false,
            skip_duplicates: false,
            library: root.clone(),
        })
        .unwrap();
        let dest = root.join("nam/Mon Pack/Full-Rig/Tone_FullRig.nam");
        assert!(dest.is_file());
        let m = Library::scan(&root).meta(&dest).unwrap();
        assert_eq!((m.gear_type.as_str(), m.corrected.as_slice()), ("amp_cab", ["type"].as_slice()));
    }
}
