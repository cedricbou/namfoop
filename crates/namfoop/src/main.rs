mod amp;
mod backend;
mod browse;
mod calibrate;
mod config;
mod import;
mod latency;
mod library;
mod live;
mod offline;
mod overrides;
mod recommend;
mod router;
mod setup;
mod tui;

#[cfg(all(test, debug_assertions))]
#[global_allocator]
static TEST_ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::config::LoadedConfig;

#[derive(Parser)]
#[command(
    name = "namfoop",
    version,
    about = "Multi-effet basse temps réel (NAM + IR) — JACK, ALSA direct ou carte simulée",
    after_help = "Exemples :\n  \
        namfoop setup              (une fois : carte, entrées, sorties, buffer)\n  \
        namfoop calibrate          (une fois : règle le niveau de ta basse)\n  \
        namfoop browse             (choisir modèle / IR / mélange en jouant)\n  \
        namfoop run -m b7k\n  \
        namfoop run -m \"800d modern\" -i \"dg212 sm57\" --route 2=di\n  \
        namfoop render -m b7k -o /tmp/out.wav\n  \
        namfoop bench --all-sizes\n  \
        namfoop import ~/Téléchargements/pack.zip\n  \
        namfoop models\n\n\
        Réglages enregistrés dans ~/.config/namfoop/config.toml ; les options priment."
)]
struct Cli {
    #[command(flatten)]
    audio: backend::AudioArgs,

    /// Fichier de config [défaut : ~/.config/namfoop/config.toml]
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Ignore la config : réglages par défaut (tests reproductibles, comparaisons)
    #[arg(long, global = true)]
    no_config: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Joue en temps réel : entrée basse -> [NAM -> IR] -> sorties selon leur rôle
    Run {
        #[command(flatten)]
        amp: amp::AmpArgs,
        #[command(flatten)]
        routing: router::RoutingArgs,
        /// Arrêt automatique après N secondes (tests)
        #[arg(long)]
        seconds: Option<f64>,
        /// Pas de vumètre (logs seulement)
        #[arg(short, long)]
        quiet: bool,
    },
    /// Traite un WAV hors ligne avec la même chaîne que `run`
    Render {
        #[command(flatten)]
        amp: amp::AmpArgs,
        /// WAV d'entrée (DI). Par défaut : signal de test de NeuralAmpModelerCore
        #[arg(long = "in")]
        input: Option<PathBuf>,
        /// WAV de sortie (flottant 32 bits). Sans : stats seulement
        #[arg(short, long = "out")]
        output: Option<PathBuf>,
        /// Taille de bloc
        #[arg(long, default_value_t = 64)]
        block: usize,
    },
    /// Mesure le coût CPU des modèles NAM (fichiers, dossiers ou mots-clés ; défaut : bibliothèque)
    Bench {
        /// Fichiers .nam, dossiers ou mots-clés (défaut : toute la bibliothèque)
        paths: Vec<PathBuf>,
        /// Taille de bloc
        #[arg(long, default_value_t = 64)]
        block: usize,
        /// Secondes d'audio simulées par modèle
        #[arg(long, default_value_t = 5.0)]
        seconds: f64,
        /// Mesure chaque taille des modèles slimmable
        #[arg(long)]
        all_sizes: bool,
        /// Dossier de la bibliothèque
        #[arg(long, default_value = "library")]
        library: PathBuf,
    },
    /// Importe des packs (zip, dossier, .nam, .wav) : rangement, vérification, doublons, type de capture
    Import {
        #[command(flatten)]
        args: import::ImportArgs,
    },
    /// Affiche ou corrige les métadonnées d'un modèle (type de capture, entrée, IR préférée…)
    Meta {
        #[command(flatten)]
        args: overrides::MetaArgs,
    },
    /// Liste les modèles et IR avec leur identifiant, et l'IR conseillée pour chaque modèle
    Models {
        /// Dossier de la bibliothèque
        #[arg(default_value = "library")]
        library: PathBuf,
    },
    /// Mesure le niveau de ta basse (joue fort ~15 s) et règle le trim d'entrée + le volume
    Calibrate {
        /// Durée d'enregistrement (s)
        #[arg(long, default_value_t = 15.0)]
        seconds: f64,
        /// Crête robuste visée à l'entrée des modèles (dBFS)
        #[arg(long, default_value_t = calibrate::DEFAULT_TARGET_DBFS, allow_hyphen_values = true)]
        target: f32,
        /// Entrée de la carte où est branchée la basse [défaut : config (setup), sinon 1]
        #[arg(long)]
        channel: Option<usize>,
        /// Analyse seulement, ne sauve rien
        #[arg(long)]
        dry_run: bool,
    },
    /// Navigateur plein écran qui joue en direct : modèles, IR conseillées, mélanges d'IR à l'oreille
    Browse {
        /// Dossier de la bibliothèque
        #[arg(long, default_value = "library")]
        library: PathBuf,
        /// Taille des modèles slimmable : 0 = le plus léger, 1 = complet
        #[arg(long, default_value_t = 1.0)]
        size: f64,
        /// Filtre initial de la liste (ex: "PopPunk")
        #[arg(long)]
        filter: Option<String>,
        #[command(flatten)]
        routing: router::RoutingArgs,
    },
    /// Assistant plein écran : backend, carte, entrées (vumètres), sorties (bip), buffer
    Setup {
        /// Dossier de la bibliothèque (un modèle sert de charge réaliste au test des buffers)
        #[arg(long, default_value = "library")]
        library: PathBuf,
    },
    /// Liste les ports JACK (audio + MIDI)
    Ports,
    /// Mesure la latence aller-retour (câble sortie -> entrée nécessaire)
    Latency {
        /// Canal de sortie qui émet l'impulsion
        #[arg(long, default_value_t = 1)]
        out_ch: usize,
        /// Canal d'entrée où revient le signal
        #[arg(long, default_value_t = 2)]
        in_ch: usize,
        /// Nombre de mesures
        #[arg(long, default_value_t = 10)]
        count: u32,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    if std::env::var_os("NAMFOOP_CHECK_ARGS").is_some() {
        return check_args(&cli);
    }
    let conf = LoadedConfig::load(cli.config.clone(), cli.no_config)?;
    let audio = cli.audio.resolve(&conf);
    match cli.cmd {
        Cmd::Run { amp, routing, seconds, quiet } => {
            let routing = routing.apply(&conf.routing);
            live::run(&audio, &conf, &amp, &routing, seconds, quiet)
        }
        Cmd::Render { amp, input, output, block } => {
            let input = input.unwrap_or_else(offline::default_input);
            let stats = offline::render(&amp, &conf, &input, output.as_deref(), block)?;
            offline::print_render(&stats);
            if !stats.finite {
                bail!("sortie invalide (NaN/inf)");
            }
            Ok(())
        }
        Cmd::Bench { paths, block, seconds, all_sizes, library } => {
            offline::bench(&paths, &library, audio.rate, block, seconds, all_sizes)
        }
        Cmd::Import { args } => {
            let imported = import::run(&args)?;
            if args.review && !args.dry_run {
                if imported.is_empty() {
                    println!("--review : aucun nouveau modèle à revoir");
                    return Ok(());
                }
                let start = browse::Start { only: Some(&imported), edit: true, ..Default::default() };
                browse::run(&audio, &conf, &args.library, &conf.routing, 1.0, start)?;
            }
            Ok(())
        }
        Cmd::Meta { args } => overrides::run(&args, conf.default_ir.as_deref()),
        Cmd::Models { library } => library::list(&library, conf.default_ir.as_deref()),
        Cmd::Calibrate { seconds, target, channel, dry_run } => {
            let mut routing = conf.routing.clone();
            if let Some(c) = channel {
                routing.bass_in = c;
            }
            let save = !dry_run && !conf.ignored;
            if !dry_run && conf.ignored {
                log::warn!("--no-config : rien ne sera sauvé");
            }
            calibrate::run(&audio, &conf.path, seconds, target, &routing, save)
        }
        Cmd::Browse { library, size, filter, routing } => {
            let routing = routing.apply(&conf.routing);
            let start = browse::Start { filter: filter.as_deref(), ..Default::default() };
            browse::run(&audio, &conf, &library, &routing, size, start)
        }
        Cmd::Setup { library } => setup::run(&audio, cli.audio.backend, &conf, &library),
        Cmd::Ports => backend::jack::list_ports(&audio.client_name),
        Cmd::Latency { out_ch, in_ch, count } => latency::run(&audio, out_ch, in_ch, count),
    }
}

/// Mode « vérification seule » (variable NAMFOOP_CHECK_ARGS) : valide la ligne de commande
/// sans rien exécuter. Utilisé par scripts/check-docs.sh pour tester les exemples des docs.
/// Si la bibliothèque existe, les mots-clés -m / -i doivent désigner un fichier unique.
fn check_args(cli: &Cli) -> Result<()> {
    let amp = match &cli.cmd {
        Cmd::Run { amp, .. } | Cmd::Render { amp, .. } => Some(amp),
        _ => None,
    };
    if let Some(a) = amp {
        if a.library.is_dir() {
            if let Some(q) = &a.model {
                library::resolve(q, &a.library, "nam")?;
            }
            if let Some(q) = a.ir.as_deref().filter(|q| *q != "auto") {
                library::resolve(q, &a.library, "wav")?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn walk(cmd: &clap::Command, path: &str, missing: &mut Vec<String>) {
        for a in cmd.get_arguments() {
            if a.is_hide_set() || ["help", "version"].contains(&a.get_id().as_str()) {
                continue;
            }
            if a.get_help().is_none() {
                missing.push(format!("{path} --{}", a.get_id()));
            }
        }
        for sub in cmd.get_subcommands() {
            if sub.get_name() == "help" {
                continue;
            }
            if sub.get_about().is_none() {
                missing.push(format!("{path} {} (sous-commande)", sub.get_name()));
            }
            walk(sub, &format!("{path} {}", sub.get_name()), missing);
        }
    }

    /// Toute option et toute sous-commande doit avoir un texte d'aide (c'est la source de docs/cli.md).
    #[test]
    fn every_argument_has_help() {
        let cmd = Cli::command();
        cmd.clone().debug_assert();
        let mut missing = Vec::new();
        walk(&cmd, "namfoop", &mut missing);
        assert!(missing.is_empty(), "aide manquante :\n{}", missing.join("\n"));
    }
}
