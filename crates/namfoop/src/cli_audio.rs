//! Options audio de la ligne de commande -> `audio_io::Settings` (priorité CLI > config > défaut).
//! La crate `audio-io` ne connaît ni clap ni la config : la traduction se fait ici.

use audio_io::{Kind, Settings};

use crate::cards::Card;
use crate::config::{Config as FileConfig, LoadedConfig};
use model::{BackendKind, CardProfile};

/// Backend audio choisi en ligne de commande.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum BackendArg {
    /// ALSA direct : latence minimale et stable, accès exclusif à la carte
    Alsa,
    /// Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel
    Sim,
}

impl From<BackendArg> for Kind {
    fn from(b: BackendArg) -> Self {
        to_audio(b.into())
    }
}

impl From<BackendArg> for BackendKind {
    fn from(b: BackendArg) -> Self {
        match b {
            BackendArg::Alsa => BackendKind::Alsa,
            BackendArg::Sim => BackendKind::Sim,
        }
    }
}

/// Backend de la config (`model`) -> backend d'`audio-io`.
pub fn to_audio(b: BackendKind) -> Kind {
    match b {
        BackendKind::Alsa => Kind::Alsa,
        BackendKind::Sim => Kind::Sim,
    }
}

/// Options audio globales de la ligne de commande. Sans option : valeur de la config
/// (`namfoop setup`), sinon valeur par défaut.
#[derive(Clone, Debug, Default, clap::Args)]
pub struct AudioArgs {
    /// Backend audio [défaut : config, sinon alsa]
    #[arg(long, value_enum, global = true)]
    pub backend: Option<BackendArg>,
    /// Carte réglée à prendre pour cette session, par son identifiant (ex: "mtk") [défaut : la
    /// préférée si elle est branchée, sinon l'unique autre carte réglée branchée]
    #[arg(long, global = true)]
    pub card: Option<String>,
    /// Carte : device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6")
    /// [défaut : config, sinon profil de carte ; sinon aucune, lancer « namfoop setup »]
    #[arg(long, global = true)]
    pub device: Option<String>,
    /// Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la
    /// sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]
    #[arg(long, global = true)]
    pub capture_device: Option<String>,
    /// Taille de buffer / période en frames [défaut : config, sinon 64]
    #[arg(long, global = true)]
    pub buffer: Option<u32>,
    /// Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]
    #[arg(long, global = true)]
    pub periods: Option<u32>,
    /// Cœur du processeur réservé au thread audio (ALSA, sim ; ex: 3). Utile avec isolcpus sur le Pi
    /// [défaut : config, sinon aucun]
    #[arg(long, global = true)]
    pub cpu: Option<usize>,
    /// Fréquence d'échantillonnage [défaut : 48000]
    #[arg(long, global = true)]
    pub rate: Option<u32>,
}

impl AudioArgs {
    /// Réglages sans profil de carte : CLI > config > défaut.
    pub fn resolve(&self, file: &FileConfig) -> Settings {
        self.resolve_with(file, None, true)
    }

    /// CLI > config > profil de carte > défaut. Le profil (device, buffer, périodes, fréquence,
    /// contrôles mixer) ne s'applique qu'au backend ALSA : la carte simulée n'a pas de device
    /// matériel. `of_config` : la carte jouée est celle que la config porte elle-même ; sinon ses
    /// `[audio] device / buffer / …` ne valent pas pour elle.
    pub fn resolve_with(
        &self,
        file: &FileConfig,
        profile: Option<&CardProfile>,
        of_config: bool,
    ) -> Settings {
        let none =
            model::AudioConfig { backend: file.audio.backend, cpu: file.audio.cpu, ..Default::default() };
        let a = if of_config { &file.audio } else { &none };
        let d = Settings::default();
        let backend = self.backend.map(Kind::from).or(a.backend.map(to_audio)).unwrap_or(d.backend);
        let p = profile.filter(|_| backend == Kind::Alsa);
        Settings {
            backend,
            device: self
                .device
                .clone()
                .or_else(|| a.device.clone())
                .or_else(|| p.and_then(|p| p.device.clone())),
            capture_device: self
                .capture_device
                .clone()
                .or_else(|| a.capture_device.clone())
                .or_else(|| p.and_then(|p| p.capture_device.clone())),
            mixer: p.map_or(Vec::new(), |p| {
                p.mixer.iter().map(|m| (m.control.clone(), m.value.clone())).collect()
            }),
            buffer: self.buffer.or(a.buffer).or(p.and_then(|p| p.buffer)).unwrap_or(d.buffer),
            periods: self.periods.or(a.periods).or(p.and_then(|p| p.periods)).unwrap_or(d.periods),
            rate: self.rate.or(a.rate).or(p.and_then(|p| p.rate)).unwrap_or(d.rate),
            cpu: self.cpu.or(a.cpu),
        }
    }

    /// Backend retenu (CLI > config > défaut).
    pub fn backend_kind(&self, file: &FileConfig) -> Kind {
        self.backend
            .map(Kind::from)
            .or(file.audio.backend.map(to_audio))
            .unwrap_or(Settings::default().backend)
    }
}

/// Profil de carte d'avant les cartes réglées : celui de la config (`[audio] profile`), sinon
/// celui qui reconnaît une carte ALSA branchée. Aucun avec `--no-config` (réglages
/// reproductibles) ni en carte simulée.
pub fn card_profile(conf: &LoadedConfig, backend: Kind) -> anyhow::Result<Option<(String, CardProfile)>> {
    if conf.ignored || backend == Kind::Sim {
        return Ok(None);
    }
    let dir = model::card::user_dir(&conf.path);
    let all = model::card::all_profiles(Some(&dir))?;
    if let Some(id) = &conf.audio.profile {
        return Ok(Some((id.clone(), model::card::find(&all, id, &dir)?)));
    }
    Ok(detect(&all, &audio_io::alsa::cards()))
}

/// Premier profil qui reconnaît une des cartes (identifiant « hw:… », nom).
pub fn detect(all: &[(String, CardProfile)], cards: &[(String, String)]) -> Option<(String, CardProfile)> {
    all.iter().find(|(_, p)| cards.iter().any(|(id, name)| p.matches(id) || p.matches(name))).cloned()
}

/// La carte jouée (§ 12 de `docs/design/instruments.md`) : parmi les cartes réglées, celle que
/// `cards::choose` retient, annoncée par une ligne. `--device` en ligne de commande, la carte
/// simulée, `--no-config` ou un ordinateur sans carte réglée gardent la règle d'avant.
pub fn card(args: &AudioArgs, conf: &LoadedConfig) -> anyhow::Result<Option<Card>> {
    let backend = args.backend_kind(conf);
    if backend == Kind::Sim || conf.ignored {
        return Ok(None);
    }
    let set = crate::cards::configured(conf)?;
    if args.device.is_some() || set.is_empty() {
        if args.card.is_some() {
            anyhow::bail!("--card : aucune carte réglée sur cet ordinateur ; `namfoop setup` en règle une");
        }
        return Ok(card_profile(conf, backend)?.map(|(id, profile)| Card { id, profile, of_config: true }));
    }
    let preferred = crate::cards::preferred(conf, &set);
    let choice = crate::cards::choose(&set, &audio_io::alsa::cards(), preferred, args.card.as_deref())
        .map_err(anyhow::Error::msg)?;
    eprintln!("{}", choice.line);
    Ok(Some(choice.card))
}

/// Réglages audio d'une commande qui ouvre la carte, avec la carte retenue.
pub fn settings(args: &AudioArgs, conf: &LoadedConfig) -> anyhow::Result<(Settings, Option<Card>)> {
    let card = card(args, conf)?;
    if let Some(c) = &card {
        log::info!("profil de carte « {} » ({})", c.id, c.profile.name);
    }
    let of_config = card.as_ref().is_none_or(|c| c.of_config);
    Ok((args.resolve_with(conf, card.as_ref().map(|c| &c.profile), of_config), card))
}

/// Routage de base. La carte de la config d'avant garde le `[routing]` de la config ; une carte
/// réglée par `setup` porte le sien dans son profil ; sinon le routage de la config, avec
/// l'entrée basse du profil si la config n'en dit rien.
pub fn base_routing(conf: &FileConfig, card: Option<&Card>) -> model::Routing {
    let mut r = conf.routing.clone();
    let Some(c) = card else { return r };
    let p = &c.profile;
    let own = !p.outputs.is_empty() || p.mic_in.is_some();
    if own && !(c.of_config && r != model::Routing::default()) {
        return model::Routing {
            bass_in: p.bass_in.unwrap_or(1),
            mic_in: p.mic_in,
            outputs: p.outputs.clone(),
        };
    }
    if r == model::Routing::default() {
        if let Some(b) = p.bass_in {
            r.bass_in = b;
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sans option ni config : mêmes valeurs que `Settings::default()` (64 frames, 3 périodes...).
    #[test]
    fn defaults_match_settings_default() {
        let s = AudioArgs { ..Default::default() }.resolve(&FileConfig::default());
        let d = Settings::default();
        assert_eq!(
            (s.backend, s.device, s.buffer, s.periods, s.rate),
            (d.backend, d.device, d.buffer, d.periods, d.rate)
        );
        assert_eq!((d.backend, d.buffer, d.periods, d.rate), (Kind::Alsa, 64, 3, 48000));
    }

    fn profile() -> CardProfile {
        CardProfile {
            name: "Carte".into(),
            match_name: "Carte".into(),
            device: Some("hw:Carte".into()),
            buffer: Some(48),
            periods: Some(2),
            bass_in: Some(2),
            mixer: vec![model::MixerSetting { control: "Capture".into(), value: "80%".into() }],
            ..Default::default()
        }
    }

    /// CLI > config > profil > défaut, et le profil ne vaut que pour ALSA.
    #[test]
    fn priority_cli_config_profile_default() {
        let p = profile();
        let mut file = FileConfig::default();
        file.audio.backend = Some(BackendKind::Alsa);
        let args = AudioArgs { ..Default::default() };
        let s = args.resolve_with(&file, Some(&p), true);
        assert_eq!((s.device.as_deref(), s.buffer, s.periods, s.rate), (Some("hw:Carte"), 48, 2, 48000));
        assert_eq!(s.mixer, vec![("Capture".to_string(), "80%".to_string())]);
        file.audio.buffer = Some(128);
        assert_eq!(args.resolve_with(&file, Some(&p), true).buffer, 128, "la config prime sur le profil");
        let cli = AudioArgs { buffer: Some(32), ..args.clone() };
        assert_eq!(cli.resolve_with(&file, Some(&p), true).buffer, 32, "la CLI prime sur tout");
        let sim = AudioArgs { backend: Some(BackendArg::Sim), ..args.clone() };
        let s = sim.resolve_with(&FileConfig::default(), Some(&p), true);
        assert_eq!((s.device, s.buffer, s.mixer.len()), (None, 64, 0), "hors ALSA : profil ignoré");
    }

    #[test]
    fn detection_and_bass_input() {
        let all = vec![("carte".to_string(), profile())];
        let cards =
            vec![("hw:MTK".to_string(), "Soundcraft".to_string()), ("hw:Carte".into(), "Ma Carte".into())];
        assert_eq!(detect(&all, &cards).map(|(id, _)| id).as_deref(), Some("carte"));
        assert!(detect(&all, &cards[..1]).is_none());
        let c = Card { id: "carte".into(), profile: profile(), of_config: true };
        assert_eq!(base_routing(&FileConfig::default(), Some(&c)).bass_in, 2);
        let mut file = FileConfig::default();
        file.routing.bass_in = 1;
        file.routing.mic_in = Some(2);
        assert_eq!(base_routing(&file, Some(&c)).bass_in, 1, "routage de la config gardé");
    }

    /// Une carte réglée par `setup` joue avec son propre routage et ses propres réglages : ceux
    /// que la config d'avant porte pour une autre carte ne valent pas pour elle.
    #[test]
    fn a_set_card_plays_with_its_own_routing_not_the_older_config_one() {
        use model::{OutputRoute, Source};
        let mut file = FileConfig::default();
        file.audio.backend = Some(BackendKind::Alsa);
        file.audio.device = Some("hw:iTwo".into());
        file.audio.buffer = Some(128);
        file.routing.mic_in = Some(2);
        file.routing.outputs = vec![OutputRoute { channel: 1, source: Source::Di, click: false }];
        let mut p = profile();
        p.outputs = vec![OutputRoute { channel: 6, source: Source::Processed, click: true }];
        let mtk = Card { id: "mtk".into(), profile: p.clone(), of_config: false };
        let r = base_routing(&file, Some(&mtk));
        assert_eq!((r.bass_in, r.mic_in, r.outputs.clone()), (2, None, p.outputs.clone()));
        let s = AudioArgs::default().resolve_with(&file, Some(&p), false);
        assert_eq!((s.device.as_deref(), s.buffer), (Some("hw:Carte"), 48), "pas le device de l'iTwo");
        let itwo = Card { of_config: true, ..mtk };
        assert_eq!(
            base_routing(&file, Some(&itwo)).mic_in,
            Some(2),
            "la carte de la config garde son routage"
        );
    }

    #[test]
    fn no_profile_without_config_and_unknown_profile_is_explained() {
        let conf = LoadedConfig::load(None, true).unwrap();
        assert!(card_profile(&conf, Kind::Alsa).unwrap().is_none(), "--no-config : pas de profil");
        let d = std::env::temp_dir().join(format!("namfoop-cardprof-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("config.toml"), "[audio]\nprofile = \"hat\"\n").unwrap();
        let conf = LoadedConfig::load(Some(d.join("config.toml")), false).unwrap();
        let err = card_profile(&conf, Kind::Alsa).unwrap_err().to_string();
        assert!(err.contains("profil de carte inconnu « hat »") && err.contains("itwo"), "{err}");
        let _ = std::fs::remove_dir_all(d);
    }
}
