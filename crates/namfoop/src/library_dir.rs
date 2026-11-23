//! Où est la bibliothèque : le premier qui répond gagne.
//!
//! 1. `--library` ;
//! 2. la variable `NAMFOOP_LIBRARY` ;
//! 3. la clé `library` de la config (relative au dossier de la config) ;
//! 4. `./library` s'il existe — lancé depuis le dossier du projet, rien ne change ;
//! 5. le dossier de données : `~/.local/share/namfoop/library` (`$XDG_DATA_HOME`).
//!
//! Le choix se fait une fois, dans `main`, après le chargement de la config : les commandes
//! reçoivent un chemin et ne savent pas d'où il vient.

use anyhow::{bail, Result};
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::LoadedConfig;

pub const ENV: &str = "NAMFOOP_LIBRARY";

/// D'où vient le dossier retenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Option,
    Env,
    Config,
    CurrentDir,
    DataDir,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Origin::Option => "option --library",
            Origin::Env => "variable NAMFOOP_LIBRARY",
            Origin::Config => "clé library de la config",
            Origin::CurrentDir => "dossier courant",
            Origin::DataDir => "emplacement par défaut",
        })
    }
}

/// Le dossier retenu, d'où il vient, et ce qui a été regardé avant lui.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub path: PathBuf,
    pub origin: Origin,
    /// Les dossiers regardés, dans l'ordre, celui retenu compris (pour le message d'erreur).
    pub tried: Vec<PathBuf>,
}

/// Ce que la résolution regarde ; rempli par `locate`, à la main dans les tests.
pub struct Inputs {
    pub option: Option<PathBuf>,
    pub env: Option<OsString>,
    pub config: Option<PathBuf>,
    pub current: PathBuf,
    pub current_exists: bool,
    pub data_dir: PathBuf,
}

/// La résolution elle-même, sans accès au disque ni à l'environnement.
pub fn resolve(i: Inputs) -> Found {
    let found = |path: PathBuf, origin, mut tried: Vec<PathBuf>| {
        tried.push(path.clone());
        Found { path, origin, tried }
    };
    if let Some(p) = i.option {
        return found(p, Origin::Option, vec![]);
    }
    if let Some(p) = i.env.filter(|v| !v.is_empty()) {
        return found(PathBuf::from(p), Origin::Env, vec![]);
    }
    if let Some(p) = i.config {
        return found(p, Origin::Config, vec![]);
    }
    if i.current_exists {
        return found(i.current, Origin::CurrentDir, vec![]);
    }
    found(i.data_dir.join("library"), Origin::DataDir, vec![i.current])
}

/// La bibliothèque de cette commande : `option` est la valeur de `--library`.
pub fn locate(option: Option<&Path>, conf: &LoadedConfig) -> Found {
    let current = PathBuf::from("library");
    resolve(Inputs {
        option: option.map(Path::to_path_buf),
        env: std::env::var_os(ENV),
        config: if conf.ignored { None } else { conf.config.library_path(&conf.path) },
        current_exists: current.is_dir(),
        current,
        data_dir: model::config::data_dir(),
    })
}

impl Found {
    /// Le chemin, s'il existe ; sinon une erreur qui dit où namfoop a cherché et comment lui
    /// indiquer le bon dossier.
    pub fn require(&self) -> Result<&Path> {
        if self.path.is_dir() {
            return Ok(&self.path);
        }
        let tried: Vec<String> = self.tried.iter().map(|p| format!("  {}", p.display())).collect();
        bail!(
            "bibliothèque introuvable ({}) — cherchée dans :\n{}\n\
             Indique-la avec --library <dossier>, la variable {ENV}, ou `library = \"<dossier>\"` \
             dans la config. Pour la ranger à l'emplacement par défaut :\n  \
             mkdir -p {data} && mv <ancien dossier>/library {data}/",
            self.origin,
            tried.join("\n"),
            data = model::config::data_dir().display(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> Inputs {
        Inputs {
            option: None,
            env: None,
            config: None,
            current: PathBuf::from("library"),
            current_exists: false,
            data_dir: PathBuf::from("/home/b/.local/share/namfoop"),
        }
    }

    #[test]
    fn option_wins_over_everything() {
        let f = resolve(Inputs {
            option: Some("/opt".into()),
            env: Some("/env".into()),
            config: Some("/conf".into()),
            current_exists: true,
            ..inputs()
        });
        assert_eq!((f.path, f.origin), (PathBuf::from("/opt"), Origin::Option));
    }

    #[test]
    fn env_then_config_then_current() {
        let f = resolve(Inputs { env: Some("/env".into()), config: Some("/conf".into()), ..inputs() });
        assert_eq!((f.path, f.origin), (PathBuf::from("/env"), Origin::Env));
        // une variable vide ne compte pas
        let f = resolve(Inputs { env: Some("".into()), config: Some("/conf".into()), ..inputs() });
        assert_eq!((f.path, f.origin), (PathBuf::from("/conf"), Origin::Config));
        let f = resolve(Inputs { config: Some("/conf".into()), current_exists: true, ..inputs() });
        assert_eq!(f.origin, Origin::Config, "la config prime sur le dossier courant");
        let f = resolve(Inputs { current_exists: true, ..inputs() });
        assert_eq!((f.path, f.origin), (PathBuf::from("library"), Origin::CurrentDir));
    }

    #[test]
    fn data_dir_last_and_lists_what_was_tried() {
        let f = resolve(inputs());
        assert_eq!(f.path, PathBuf::from("/home/b/.local/share/namfoop/library"));
        assert_eq!(f.origin, Origin::DataDir);
        assert_eq!(f.tried, [PathBuf::from("library"), f.path.clone()]);
    }

    #[test]
    fn missing_library_says_where_and_how() {
        let f = resolve(Inputs { option: Some("/nulle/part/namfoop".into()), ..inputs() });
        let e = f.require().unwrap_err().to_string();
        assert!(e.contains("/nulle/part/namfoop") && e.contains("option --library"), "{e}");
        assert!(e.contains("NAMFOOP_LIBRARY") && e.contains("library = "), "{e}");
    }
}
