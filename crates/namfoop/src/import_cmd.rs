//! `namfoop import` : options de la ligne de commande et bilan affiché. L'import lui-même est dans
//! `library::import`.

use anyhow::Result;
use std::path::PathBuf;

use crate::config::LoadedConfig;
use library::import::{Capture, Outcome};
use library::overrides::Overrides;
use library::recommend::recommend;
use library::Library;

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

impl From<&ImportArgs> for library::import::ImportArgs {
    fn from(a: &ImportArgs) -> Self {
        Self {
            sources: a.sources.clone(),
            pack: a.pack.clone(),
            dry_run: a.dry_run,
            skip_duplicates: a.skip_duplicates,
            library: a.library.clone(),
        }
    }
}

/// Importe, affiche le bilan ; renvoie les modèles ajoutés (chemins relatifs à la bibliothèque).
pub fn run(a: &ImportArgs, conf: &LoadedConfig) -> Result<Vec<String>> {
    let out = library::import::run(&a.into())?;
    print_report(a, conf, &out)?;
    Ok(out.imported_models())
}

fn print_report(a: &ImportArgs, conf: &LoadedConfig, out: &Outcome) -> Result<()> {
    let (reports, skipped, corrected, written) = (&out.reports, &out.skipped, out.corrected, out.written);
    let lib = (!a.dry_run).then(|| Library::scan(&a.library));
    // profils (IR puis modèles, avec ta basse) : calculés ici pour les nouveaux fichiers
    let profiles = lib.as_ref().map(|l| library::profile::Profiles::load(l, |_| {}));
    let model_profiles = match (&lib, &profiles) {
        (Some(l), Some(p)) => Some(library::model_profile::ModelProfiles::load(l, conf, p, |n| {
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
