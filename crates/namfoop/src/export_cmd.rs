//! `namfoop export` : met toute la bibliothèque dans la valise de la clé, ne recopie que ce qui a
//! changé, et dit quoi, famille par famille. « Tu peux retirer la clé » ne s'affiche qu'une fois
//! tout écrit pour de bon. L'écriture elle-même est dans `library::exchange::suitcase`.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::config::LoadedConfig;
use crate::exchange::{self as xchg, lib_err};
use library::exchange::label::Stamp;
use library::exchange::memory;
use library::exchange::suitcase::{apply_export, plan_export, Suitcase};
use library::exchange::{inventory, new_id, portable_names};

#[derive(Clone, Debug, clap::Args)]
pub struct ExportArgs {
    /// Dossier où poser la valise : la racine de la clé USB, ou tout autre dossier
    /// [défaut : celui du dernier échange]
    #[arg(value_name = "DOSSIER")]
    pub folder: Option<PathBuf>,
    /// Montre ce qui partirait, sans rien écrire
    #[arg(long)]
    pub dry_run: bool,
    /// Dossier de la mémoire des échanges de cette machine (son nom, ses derniers échanges)
    /// [défaut : echanges/ à côté du fichier de config ; obligatoire avec --no-config]
    #[arg(long, value_name = "DOSSIER")]
    pub exchanges: Option<PathBuf>,
    /// Dossier de la bibliothèque [défaut : ./library, sinon ~/.local/share/namfoop/library]
    #[arg(long = "library", value_name = "DOSSIER")]
    pub library_arg: Option<PathBuf>,
    /// La bibliothèque retenue (`library_dir`), posée par `main`.
    #[arg(skip)]
    pub library: PathBuf,
}

/// La commande, telle que `main` l'appelle : la bibliothèque doit exister.
pub fn start(mut a: ExportArgs, conf: &LoadedConfig) -> Result<()> {
    a.library = crate::library_dir::locate(a.library_arg.as_deref(), conf).require()?.to_path_buf();
    run(&a, conf, &mut xchg::Terminal)
}

pub fn run(a: &ExportArgs, conf: &LoadedConfig, ui: &mut dyn xchg::Ui) -> Result<()> {
    let mem = xchg::memory_dir(a.exchanges.as_deref(), conf)?;
    let mut machine = xchg::machine(&mem, ui)?;
    let folder = xchg::folder(a.folder.as_deref(), &mem, &machine)?;
    let started = std::time::Instant::now();
    let local = inventory(&a.library).map_err(lib_err)?;
    log::debug!(
        "inventaire de {} : {} fichiers en {:?}",
        a.library.display(),
        local.files.len(),
        started.elapsed()
    );
    portable_names(local.files.keys()).map_err(lib_err)?;
    let suitcase = Suitcase::open(&folder).map_err(lib_err)?;
    let held = suitcase.state().map_err(lib_err)?;
    let id = suitcase.label.as_ref().map_or_else(|| new_id(xchg::seed()), |l| l.id.clone());
    let last = memory::load_exchange(&mem, &id).map_err(lib_err)?;
    let date = xchg::now();
    let stamp = Stamp { machine: machine.name.clone(), date, complete: true };
    let plan = plan_export(&local, &held, &suitcase, &id, stamp);

    match (&suitcase.label, &last) {
        (None, _) => println!("valise      nouvelle, dans {}", folder.display()),
        (Some(_), Some(x)) => {
            println!(
                "valise      {} · vue ici le {}",
                suitcase.dir.display(),
                xchg::short_date(&x.date, false)
            )
        }
        (Some(l), None) => println!(
            "valise      {} · de {}, {}",
            suitcase.dir.display(),
            l.export.machine,
            xchg::short_date(&l.export.date, true)
        ),
    }
    println!("machine     {}", machine.name);
    println!();
    for line in xchg::report(&plan.changes, &local) {
        println!("{line}");
    }
    println!();

    let elements: Vec<_> = local.elements.values().cloned().collect();
    let remember =
        |m: &mut memory::Machine| xchg::remember(&mem, m, &id, suitcase.folder(), elements.clone());
    if plan.up_to_date {
        println!(" La valise est déjà à jour avec {}.", machine.name);
        if !a.dry_run {
            remember(&mut machine)?;
        }
        return Ok(());
    }
    if a.dry_run {
        println!(" --dry-run : {} à copier, rien n'est écrit.", xchg::size(plan.bytes));
        return Ok(());
    }
    check_room(&folder, plan.bytes)?;
    let t = std::time::Instant::now();
    apply_export(&a.library, &suitcase, &plan, &mut |_| {}).map_err(lib_err)?;
    let took = t.elapsed().as_secs_f64();
    remember(&mut machine)?;
    println!(
        " {} copiés en {}. La valise est complète : {}.",
        xchg::size(plan.bytes),
        xchg::seconds(took),
        xchg::totals(&local)
    );
    println!(" {}", crate::theme::paint("Tu peux retirer la clé.", crate::theme::ok()));
    Ok(())
}

/// La place libre sur la clé, vérifiée avant la première écriture.
fn check_room(folder: &Path, need: u64) -> Result<()> {
    let Some(free) = free_bytes(folder) else { return Ok(()) };
    // une marge pour les dossiers et l'étiquette
    let need = need + 1_000_000;
    if need > free {
        bail!("il manque {} sur la clé : rien n'a changé dans la valise", xchg::size(need - free));
    }
    Ok(())
}

fn free_bytes(folder: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(folder.as_os_str().as_bytes()).ok()?;
    // SAFETY: `st` est rempli par statvfs, `c` est une chaîne C valide.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    (r == 0).then(|| st.f_bavail as u64 * st.f_frsize as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("namfoop-export-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Une bibliothèque jetable (jamais celle de l'utilisateur).
    fn library(root: &Path) -> PathBuf {
        let lib = root.join("library");
        for (rel, body) in [
            ("nam/Pack/B7K.nam", "{}"),
            ("presets/Été à Lyon.preset.toml", "name = \"Été à Lyon\"\n"),
            ("instruments/bb.toml", "name = \"BB\"\nnumber = 1\n"),
        ] {
            let p = lib.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        lib
    }

    fn args(root: &Path, folder: Option<&Path>) -> ExportArgs {
        ExportArgs {
            folder: folder.map(Path::to_path_buf),
            dry_run: false,
            exchanges: None,
            library_arg: None,
            library: root.join("library"),
        }
    }

    /// La config de l'ordinateur reste identique à l'octet ; la mémoire s'écrit à côté d'elle.
    #[test]
    fn export_leaves_the_config_byte_identical_and_remembers_the_key() {
        let root = tmp("conf");
        library(&root);
        let cle = root.join("NAMFOOP");
        std::fs::create_dir_all(&cle).unwrap();
        let config = root.join("config").join("config.toml");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        let text = b"# ma config\n[audio]\ndevice = \"hw:iTwo\"\nlevel = \"match\"\n";
        std::fs::write(&config, text).unwrap();
        let conf = LoadedConfig::load(Some(config.clone()), false).unwrap();
        let mut ask = xchg::Script::new(&["studio\n"]);
        run(&args(&root, Some(&cle)), &conf, &mut ask).unwrap();
        assert_eq!(std::fs::read(&config).unwrap(), text);
        let mem = root.join("config").join(xchg::DIR);
        let m = memory::load_machine(&mem).unwrap().unwrap();
        assert_eq!(m.name, "studio");
        assert!(cle.join("valise-namfoop/library/presets/Été à Lyon.preset.toml").is_file());
        // sans dossier : celui du dernier échange ; le nom n'est plus demandé
        std::fs::write(root.join("library/presets/Neuf.preset.toml"), "name = \"Neuf\"\n").unwrap();
        run(&args(&root, None), &conf, &mut ask).unwrap();
        assert_eq!(memory::load_machine(&mem).unwrap().unwrap().name, "studio", "pas redemandé");
        assert!(cle.join("valise-namfoop/library/presets/Neuf.preset.toml").is_file());
        assert_eq!(std::fs::read(&config).unwrap(), text);
        // la clé retirée : le message la nomme
        std::fs::remove_dir_all(&cle).unwrap();
        let e = run(&args(&root, None), &conf, &mut ask).unwrap_err().to_string();
        assert!(e.contains("clé NAMFOOP absente"), "{e}");
    }

    #[test]
    fn dry_run_writes_nothing_anywhere() {
        let root = tmp("dry");
        library(&root);
        let cle = root.join("cle");
        std::fs::create_dir_all(&cle).unwrap();
        let conf = LoadedConfig::load(None, true).unwrap();
        let mut a = args(&root, Some(&cle));
        a.dry_run = true;
        a.exchanges = Some(root.join("mem"));
        run(&a, &conf, &mut xchg::Script::new(&["pi"])).unwrap();
        assert_eq!(std::fs::read_dir(&cle).unwrap().count(), 0);
        assert!(!root.join("mem").exists());
    }

    #[test]
    fn a_name_a_usb_key_refuses_stops_before_writing() {
        let root = tmp("fat");
        library(&root);
        std::fs::write(root.join("library/presets/A: B.preset.toml"), "x").unwrap();
        let cle = root.join("cle");
        std::fs::create_dir_all(&cle).unwrap();
        let conf = LoadedConfig::load(None, true).unwrap();
        let mut a = args(&root, Some(&cle));
        a.exchanges = Some(root.join("mem"));
        let e = run(&a, &conf, &mut xchg::Script::new(&["pi"])).unwrap_err().to_string();
        assert!(e.contains("A: B.preset.toml"), "{e}");
        assert_eq!(std::fs::read_dir(&cle).unwrap().count(), 0);
    }
}
