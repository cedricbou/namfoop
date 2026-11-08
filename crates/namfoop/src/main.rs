mod amp;
mod backend;
mod calibrate;
mod config;
mod latency;
mod library;
mod live;
mod offline;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "namfoop",
    version,
    about = "Multi-effet basse temps réel (NAM + IR) — JACK ou ALSA direct",
    after_help = "Exemples :\n  \
        pw-jack namfoop run -m b7k -i \"svt 8x10\"\n  \
        namfoop --backend alsa --buffer 48 run -m \"svt classic\"\n  \
        namfoop calibrate          (une fois : règle le niveau de ta basse)\n  \
        namfoop render -m b7k -o /tmp/out.wav\n  \
        namfoop bench --all-sizes\n  \
        namfoop models"
)]
struct Cli {
    #[command(flatten)]
    audio: backend::Config,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Joue en temps réel : entrée 1 -> [NAM -> IR] -> sorties
    Run {
        #[command(flatten)]
        amp: amp::AmpArgs,
        /// Contenu de la sortie 2
        #[arg(long, value_enum, default_value_t = live::Out2::Same)]
        out2: live::Out2,
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
    /// Liste les modèles et IR de la bibliothèque
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
        /// Entrée de la carte où est branchée la basse
        #[arg(long, default_value_t = 1)]
        channel: usize,
        /// Analyse seulement, ne sauve rien
        #[arg(long)]
        dry_run: bool,
        /// Fichier de config à écrire (défaut : ~/.config/namfoop/config.toml)
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Liste les ports JACK (audio + MIDI)
    Ports,
    /// Mesure la latence aller-retour (câble sortie -> entrée nécessaire)
    Latency {
        /// Canal de sortie qui émet l'impulsion (1 ou 2)
        #[arg(long, default_value_t = 1)]
        out_ch: usize,
        /// Canal d'entrée où revient le signal (1 ou 2)
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
    match cli.cmd {
        Cmd::Run { amp, out2, seconds, quiet } => live::run(&cli.audio, &amp, out2, seconds, quiet),
        Cmd::Render { amp, input, output, block } => {
            let input = input.unwrap_or_else(offline::default_input);
            let stats = offline::render(&amp, &input, output.as_deref(), block)?;
            offline::print_render(&stats);
            if !stats.finite {
                bail!("sortie invalide (NaN/inf)");
            }
            Ok(())
        }
        Cmd::Bench { paths, block, seconds, all_sizes, library } => {
            offline::bench(&paths, &library, cli.audio.rate, block, seconds, all_sizes)
        }
        Cmd::Models { library } => library::list(&library),
        Cmd::Calibrate { seconds, target, channel, dry_run, config } => {
            let path = config.unwrap_or_else(config::default_path);
            calibrate::run(&cli.audio, &path, seconds, target, channel, !dry_run)
        }
        Cmd::Ports => backend::jack::list_ports(&cli.audio.client_name),
        Cmd::Latency { out_ch, in_ch, count } => latency::run(&cli.audio, out_ch, in_ch, count),
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
            if let Some(q) = &a.ir {
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
