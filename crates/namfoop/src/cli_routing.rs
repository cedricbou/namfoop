//! Options de routage de la CLI (priment sur [routing] de la config) -> `engine::router::Routing`.

use engine::router::{default_outputs, OutputRoute, Routing, Source};

pub fn parse_out(s: &str) -> std::result::Result<OutputRoute, String> {
    let (c, src) = s.split_once('=').ok_or("format attendu : N=source (ex: 2=di)")?;
    let channel: usize = c.trim().parse().map_err(|_| format!("canal invalide : {c}"))?;
    let source = src
        .trim()
        .parse::<Source>()
        .map_err(|_| format!("source inconnue « {src} » (processed, di, mic, mute)"))?;
    // `--route` ne dit rien du clic de métronome : il se règle dans la config (#201)
    Ok(OutputRoute { channel, source, click: false })
}

/// Options de routage de `run` (priment sur la config).
#[derive(Clone, Debug, Default, clap::Args)]
pub struct RoutingArgs {
    /// Entrée de la carte où est branchée la basse [défaut : config, sinon 1]
    #[arg(long)]
    pub bass_in: Option<usize>,
    /// Entrée du micro [défaut : config]
    #[arg(long)]
    pub mic_in: Option<usize>,
    /// Rôle d'une sortie, N=source avec source = processed, di, mic ou mute (répétable,
    /// ex: --route 1=processed --route 2=di) [défaut : config, sinon 1 et 2 = processed]
    #[arg(long = "route", value_parser = parse_out, value_name = "N=SOURCE")]
    pub outputs: Vec<OutputRoute>,
}

impl RoutingArgs {
    pub fn apply(&self, base: &Routing) -> Routing {
        let mut r = base.clone();
        if let Some(b) = self.bass_in {
            r.bass_in = b;
        }
        if self.mic_in.is_some() {
            r.mic_in = self.mic_in;
        }
        if !self.outputs.is_empty() {
            if r.outputs.is_empty() {
                r.outputs = default_outputs(2);
            }
            for o in &self.outputs {
                r.outputs.retain(|x| x.channel != o.channel);
                r.outputs.push(*o);
            }
            r.outputs.sort_by_key(|o| o.channel);
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_overrides_config() {
        let base = Routing {
            bass_in: 1,
            mic_in: None,
            outputs: vec![
                OutputRoute { channel: 1, source: Source::Processed, click: false },
                OutputRoute { channel: 2, source: Source::Processed, click: false },
            ],
        };
        let args = RoutingArgs { bass_in: Some(2), mic_in: None, outputs: vec![parse_out("2=di").unwrap()] };
        let r = args.apply(&base);
        assert_eq!(r.bass_in, 2);
        assert_eq!(
            r.outputs,
            vec![
                OutputRoute { channel: 1, source: Source::Processed, click: false },
                OutputRoute { channel: 2, source: Source::Di, click: false }
            ]
        );
        assert!(parse_out("2-di").is_err() && parse_out("x=di").is_err() && parse_out("1=foo").is_err());
    }
}
