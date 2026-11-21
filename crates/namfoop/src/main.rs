mod amp;
mod browse;
mod calibrate;
mod cli_audio;
mod cli_midi;
mod cli_routing;
mod config;
mod fx_cmd;
mod import_cmd;
mod latency;
mod live;
mod meta_cmd;
mod midi_cmd;
mod models_cmd;
mod offline;
mod presets_cmd;
mod sets_cmd;
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
    about = "Multi-effet basse temps réel (NAM + IR) — ALSA direct ou carte simulée",
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
    audio: cli_audio::AudioArgs,

    #[command(flatten)]
    midi: cli_midi::MidiArgs,

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
        routing: cli_routing::RoutingArgs,
        /// Arrêt automatique après N secondes (tests)
        #[arg(long)]
        seconds: Option<f64>,
        /// Pas de vumètre (logs seulement)
        #[arg(short, long)]
        quiet: bool,
        /// Coupe la queue de preset : la réverbe et le delay du preset quitté s'arrêtent net au
        /// changement, mais une seule chaîne tourne (moins de charge DSP)
        #[arg(long)]
        no_trails: bool,
        /// Clic de métronome au tempo, dans les sorties déclarées « in-ear » par `namfoop setup`
        /// (jamais dans le sondier). Niveau et temps par mesure : [metronome] de la config
        #[arg(long)]
        click: bool,
        /// Jouer un SET de concert, écran de scène VERROUILLÉ : rien ne s'édite, seuls les sons
        /// changent. Identifiant, nom ou mots-clés (`namfoop sets`) ; sans valeur, le set retenu
        /// (`[set] default` de la config). S'exclut avec --preset
        #[arg(long, value_name = "SET", num_args = 0..=1, default_missing_value = "")]
        set: Option<String>,
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
        /// Mesure les effets du catalogue (aux réglages par défaut) au lieu des modèles
        #[arg(long)]
        fx: bool,
        /// Dossier de la bibliothèque
        #[arg(long, default_value = "library")]
        library: PathBuf,
    },
    /// Importe des packs (zip, dossier, .nam, .wav) : rangement, vérification, fichiers identiques, type de capture
    Import {
        #[command(flatten)]
        args: import_cmd::ImportArgs,
    },
    /// Affiche ou corrige les métadonnées d'un modèle (type de capture, entrée, IR préférée…)
    Meta {
        #[command(flatten)]
        args: meta_cmd::MetaArgs,
    },
    /// Liste les effets (--pre / --post) ou, avec un nom, les paramètres d'un effet
    Fx {
        /// Nom d'un effet (ex: gain) : ses paramètres, unités, plages, défauts
        name: Option<String>,
    },
    /// Liste les modèles et IR avec leur identifiant, et l'IR conseillée pour chaque modèle
    Models {
        /// Dossier de la bibliothèque
        #[arg(default_value = "library")]
        library: PathBuf,
    },
    /// Liste les presets enregistrés (identifiant pour `run --preset`, nom, chaîne)
    Presets {
        /// Dossier de la bibliothèque
        #[arg(default_value = "library")]
        library: PathBuf,
    },
    /// Liste les sets de concert (identifiant, nom, morceaux et sons, avertissements)
    Sets {
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
        /// Set de concert à ouvrir : identifiant (`namfoop sets`), nom ou mots-clés. PgDn / PgUp
        /// suivent alors ses morceaux au lieu de la banque entière
        #[arg(long, value_name = "SET")]
        set: Option<String>,
        /// Coupe la queue de preset : la réverbe et le delay du preset quitté s'arrêtent net au
        /// changement, mais une seule chaîne tourne (moins de charge DSP)
        #[arg(long)]
        no_trails: bool,
        #[command(flatten)]
        routing: cli_routing::RoutingArgs,
    },
    /// Liste les ports MIDI d'entrée, puis affiche en clair ce qui en arrive (pédalier, Ableton)
    Midi {
        #[command(flatten)]
        args: midi_cmd::MidiCmdArgs,
    },
    /// Assistant plein écran : backend, carte, entrées (vumètres), sorties (bip), buffer
    Setup {
        /// Dossier de la bibliothèque (un modèle sert de charge réaliste au test des buffers)
        #[arg(long, default_value = "library")]
        library: PathBuf,
    },
    /// Mesure la latence aller-retour (câble sortie -> entrée nécessaire)
    Latency {
        /// Canal de sortie qui émet l'impulsion
        #[arg(long, default_value_t = 1)]
        out_ch: usize,
        /// Canal d'entrée où revient le signal
        #[arg(long, default_value_t = 2)]
        in_ch: usize,
        /// Nombre de mesures par passe
        #[arg(long, default_value_t = 10)]
        count: u32,
        /// Mesure aussi chaque chaîne de référence : l'impulsion la traverse, puis elle tourne à
        /// côté (charge DSP et xruns par passe)
        #[arg(long)]
        chains: bool,
        /// Une seule chaîne de référence, par son nom (un nom inconnu les liste)
        #[arg(long, value_name = "NOM", conflicts_with = "chains")]
        chain: Option<String>,
        /// Modèle qui remplit les chaînes de référence (défaut : le modèle d'exemple du moteur NAM)
        #[arg(short = 'm', long)]
        model: Option<String>,
        /// IR qui remplit les chaînes de référence (défaut : une IR de synthèse)
        #[arg(short = 'i', long)]
        ir: Option<String>,
        /// Dossier de la bibliothèque, où -m et -i sont cherchés
        #[arg(long, default_value = "library")]
        library: PathBuf,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    if std::env::var_os("NAMFOOP_CHECK_ARGS").is_some() {
        return check_args(&cli);
    }
    let conf = LoadedConfig::load(cli.config.clone(), cli.no_config)?;
    // réglages audio (et profil de carte) : seulement pour les commandes qui ouvrent la carte
    let audio = || cli_audio::settings(&cli.audio, &conf);
    match cli.cmd {
        Cmd::Run { amp, routing, seconds, quiet, no_trails, click, set } => {
            // `run --set` : la scène verrouillée (§ 9 de docs/design/set.md). C'est l'écran de
            // `browse`, privé de tout ce qui édite — plutôt qu'un second écran à tenir à jour.
            if let Some(query) = set {
                if amp.preset.is_some() {
                    anyhow::bail!(
                        "--set et --preset s'excluent : un set choisit déjà ses presets, morceau \
                         par morceau. Donne l'un ou l'autre."
                    );
                }
                // sans valeur (`run --set`), c'est le set retenu. On le vérifie **avant** de
                // prendre la carte son : un service de scène mal configuré doit échouer tout de
                // suite, pas après avoir ouvert l'audio et scanné la bibliothèque.
                let kept = conf.config.set.as_ref().and_then(|x| x.default.as_deref());
                if query.is_empty() && kept.is_none() {
                    anyhow::bail!(
                        "run --set : aucun set indiqué et aucun set retenu. Donne-le \
                         (`run --set <id>`, cf. `namfoop sets`), ou ouvre-le une fois dans \
                         `browse` (touche L) pour qu'il devienne le set par défaut."
                    );
                }
                let (audio, profile) = audio()?;
                let routing = routing.apply(&cli_audio::base_routing(&conf, profile.as_ref()));
                let start = browse::Start {
                    set: (!query.is_empty()).then_some(query.as_str()),
                    locked: true,
                    midi: cli.midi.open(&conf.config),
                    ..Default::default()
                };
                // la bibliothèque est celle de `--library`, que `AmpArgs` porte déjà
                return browse::run(&audio, &conf, &amp.library, &routing, 1.0, !no_trails, start);
            }
            let (audio, profile) = audio()?;
            let routing = routing.apply(&cli_audio::base_routing(&conf, profile.as_ref()));
            let session = live::Session { seconds, quiet, trails: !no_trails, click };
            live::run(&audio, &conf, &amp, &routing, cli.midi.open(&conf.config), session)
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
        Cmd::Bench { paths, block, seconds, all_sizes, fx, library } => {
            let rate = cli.audio.resolve(&conf).rate;
            if fx {
                offline::bench_fx(rate, block, seconds)
            } else {
                offline::bench(&paths, &library, rate, block, seconds, all_sizes)
            }
        }
        Cmd::Import { args } => {
            let imported = import_cmd::run(&args, &conf)?;
            if args.review && !args.dry_run {
                if imported.is_empty() {
                    println!("--review : aucun nouveau modèle à revoir");
                    return Ok(());
                }
                let start = browse::Start {
                    only: Some(&imported),
                    edit: true,
                    midi: cli.midi.open(&conf.config),
                    ..Default::default()
                };
                let (audio, profile) = audio()?;
                let routing = cli_audio::base_routing(&conf, profile.as_ref());
                browse::run(&audio, &conf, &args.library, &routing, 1.0, true, start)?;
            }
            Ok(())
        }
        Cmd::Meta { args } => meta_cmd::run(&args, &conf),
        Cmd::Models { library } => models_cmd::list(&library, &conf),
        Cmd::Presets { library } => presets_cmd::list(&library, &conf),
        Cmd::Sets { library } => sets_cmd::list(&library, &conf),
        Cmd::Fx { name } => fx_cmd::run(name.as_deref()),
        Cmd::Calibrate { seconds, target, channel, dry_run } => {
            let (audio, profile) = audio()?;
            let mut routing = cli_audio::base_routing(&conf, profile.as_ref());
            if let Some(c) = channel {
                routing.bass_in = c;
            }
            let save = !dry_run && !conf.ignored;
            if !dry_run && conf.ignored {
                log::warn!("--no-config : rien ne sera sauvé");
            }
            calibrate::run(&audio, &conf.path, seconds, target, &routing, save)
        }
        Cmd::Browse { library, size, filter, set, no_trails, routing } => {
            let (audio, profile) = audio()?;
            let routing = routing.apply(&cli_audio::base_routing(&conf, profile.as_ref()));
            let start = browse::Start {
                filter: filter.as_deref(),
                set: set.as_deref(),
                midi: cli.midi.open(&conf.config),
                ..Default::default()
            };
            browse::run(&audio, &conf, &library, &routing, size, !no_trails, start)
        }
        Cmd::Midi { args } => midi_cmd::run(&args, &cli.midi, &conf),
        Cmd::Setup { library } => setup::run(&audio()?.0, cli.audio.backend.map(Into::into), &conf, &library),
        Cmd::Latency { out_ch, in_ch, count, chains, chain, model, ir, library } => {
            let args = latency::Args { out_ch, in_ch, count, chains, chain, model, ir, library };
            latency::run(&audio()?.0, &args)
        }
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

    /// Erreurs de syntaxe des effets : chacune dit quoi corriger.
    #[test]
    fn fx_syntax_errors_help() {
        let err = |fx: &str| match Cli::try_parse_from(["namfoop", "render", "--post", fx]) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("{fx} accepté"),
        };
        assert!(err("fuzz").contains("effet « fuzz » inconnu (gain"), "{}", err("fuzz"));
        assert!(err("gain:vol=3").contains("paramètre « vol » inconnu (db)"));
        assert!(err("gain:db=99").contains("hors plage (-60 … 24 dB)"));
        assert!(err("gain:db=99").contains("namfoop fx"));
        // le parallèle est un bloc de la chaîne, pas un effet (ADR 0006)
        for fx in ["split", "split:freq=200", "parallele"] {
            assert!(err(fx).contains("bloc de la chaîne"), "{}", err(fx));
            assert!(err(fx).contains("dans un preset") && err(fx).contains("--crossover"), "{}", err(fx));
        }
        let ok = Cli::try_parse_from([
            "namfoop",
            "render",
            "--pre",
            "gain:db=-6",
            "--post",
            "gain",
            "--post",
            "gain:db=3",
        ])
        .unwrap();
        let Cmd::Render { amp, .. } = ok.cmd else { panic!() };
        assert_eq!(amp.pre.len(), 1);
        assert_eq!(amp.post.iter().map(|f| f.to_string()).collect::<Vec<_>>(), ["gain", "gain:db=3"]);
    }

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

    /// Un preset décrit toute la chaîne : les options de chaîne sont refusées, avec un message qui
    /// dit quoi faire. Et il se résout comme `-m` / `-i` (bibliothèque jetable).
    #[test]
    fn a_preset_refuses_to_mix_with_the_chain_options() {
        // bibliothèque jetable (jamais celle de l'utilisateur)
        let d = std::env::temp_dir().join(format!("namfoop-preset-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for (rel, body) in [
            ("nam/Ampeg SVT/SVT Ultra Lo SM57.nam", "{}"),
            (
                "presets/Chill verse.preset.toml",
                "name = \"Chill verse\"\n[[block]]\nkind = \"fx\"\n[block.fx]\ntype = \"reverb\"\n",
            ),
        ] {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().expect("dossier")).expect("création");
            std::fs::write(p, body).expect("écriture");
        }
        let conf = config::LoadedConfig::load(None, true).expect("config ignorée");
        let args = |extra: &[&str]| -> amp::AmpArgs {
            let mut v =
                vec!["namfoop", "render", "--preset", "chill", "--library", d.to_str().expect("chemin")];
            v.extend_from_slice(extra);
            match Cli::try_parse_from(v).expect("options valides").cmd {
                Cmd::Render { amp, .. } => amp,
                _ => panic!("render attendu"),
            }
        };
        // seul, le preset donne sa chaîne
        let chain = args(&[]).chain_spec(&conf, None).expect("la chaîne du preset");
        assert_eq!(chain.blocks.len(), 1, "{}", chain.describe());
        assert!(chain.describe().contains("reverb"), "{}", chain.describe());
        // mélangé, il refuse en nommant l'option fautive
        for (extra, nom) in [
            (vec!["-m", "svt"], "-m"),
            (vec!["-i", "auto"], "-i"),
            (vec!["--pre", "gate"], "--pre"),
            (vec!["--post", "reverb"], "--post"),
            (vec!["--crossover", "150"], "--crossover"),
        ] {
            let e = args(&extra).chain_spec(&conf, None).expect_err("refus attendu").to_string();
            assert!(e.contains("--preset décrit déjà toute la chaîne"), "{extra:?} : {e}");
            assert!(e.contains(nom), "{extra:?} : {e}");
        }
        // introuvable : le message renvoie vers `namfoop presets`
        let mut a = args(&[]);
        a.preset = Some("inconnu".into());
        let e = a.chain_spec(&conf, None).expect_err("introuvable").to_string();
        assert!(e.contains("aucun preset") && e.contains("namfoop presets"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
