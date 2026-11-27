//! Ce que `setup` écrit pour une carte ALSA (`docs/design/instruments.md` § 12) : **tout ce qui
//! tient à la carte va dans son profil** — device, buffer, périodes, entrées, rôle de chaque
//! sortie —, et la config ne garde que la carte préférée. Régler une seconde carte ne touche pas
//! au profil de la première.
//!
//! Une config d'avant, qui porte elle-même `[audio] device` et `[routing]`, est rangée au passage
//! dans le profil de sa carte : ses réglages ne sont pas perdus quand une autre carte est réglée.

use anyhow::Result;
use std::path::PathBuf;

use crate::cards;
use crate::config::{Config, LoadedConfig};
use model::{AudioConfig, CardProfile, Routing};

/// Le device réglé dans l'assistant : son identifiant ALSA (`hw:MTK`) et son nom lisible.
pub struct Device<'a> {
    pub id: &'a str,
    pub label: &'a str,
}

/// Ce qui a été écrit, pour le dire en sortant.
#[derive(Debug, PartialEq)]
pub struct Saved {
    pub id: String,
    pub path: PathBuf,
    /// La carte de la config d'avant, rangée dans son profil.
    pub moved: Option<PathBuf>,
    pub preferred: String,
}

/// Range le profil de la carte réglée, et la config d'avant dans le profil de sa carte ; `new`
/// (la config que l'assistant a remplie) ne garde que backend, carte préférée et cœur réservé.
/// `ask` pose la question de la carte préférée quand une autre l'est déjà.
pub fn save(
    conf: &LoadedConfig,
    new: &mut Config,
    dev: &Device,
    ask: impl Fn(&str) -> bool,
) -> Result<Saved> {
    let dir = model::card::user_dir(&conf.path);
    let all = model::card::all_profiles(Some(&dir))?;
    let set = cards::configured(conf)?;
    let preferred = cards::preferred(conf, &set).map(str::to_string);

    // la carte de la config d'avant, si ce n'est pas celle qu'on règle : ses réglages vont dans
    // son profil avant que la config ne les perde
    let mut moved = None;
    if let Some(old) = set.iter().find(|c| c.of_config && c.profile.device.as_deref() != Some(dev.id)) {
        let p = with_config(old.profile.clone(), &conf.audio, &conf.routing);
        let path = dir.join(format!("{}.toml", old.id));
        p.save(&path)?;
        moved = Some(path);
    }

    let (id, base) = match crate::cli_audio::detect(&all, &[(dev.id.to_string(), dev.label.to_string())]) {
        Some(found) => found,
        None => {
            let alsa = dev.id.trim_start_matches("hw:").split(',').next().unwrap_or(dev.id);
            let profile =
                CardProfile { name: dev.label.into(), match_name: alsa.into(), ..Default::default() };
            (cards::system_id(dev.id, cards::alsa_name), profile)
        }
    };
    let mut p = base;
    p.device = Some(dev.id.to_string());
    p.buffer = new.audio.buffer;
    p.periods = new.audio.periods;
    if conf.audio.device.as_deref() == Some(dev.id) && p.capture_device.is_none() {
        p.capture_device = conf.audio.capture_device.clone();
    }
    p.bass_in = Some(new.routing.bass_in);
    p.mic_in = new.routing.mic_in;
    p.outputs = new.routing.outputs.clone();
    let path = dir.join(format!("{id}.toml"));
    p.save(&path)?;

    let preferred = match preferred {
        Some(other) if other != id && !ask(&p.name) => other,
        _ => id.clone(),
    };
    new.audio = AudioConfig {
        backend: new.audio.backend,
        profile: Some(preferred.clone()),
        cpu: new.audio.cpu,
        ..Default::default()
    };
    new.routing = Routing::default();
    Ok(Saved { id, path, moved, preferred })
}

/// Le profil d'une carte, complété des réglages que la config d'avant portait pour elle.
fn with_config(mut p: CardProfile, a: &AudioConfig, r: &Routing) -> CardProfile {
    p.device = a.device.clone().or(p.device);
    p.capture_device = a.capture_device.clone().or(p.capture_device);
    p.buffer = a.buffer.or(p.buffer);
    p.periods = a.periods.or(p.periods);
    if *r != Routing::default() || p.bass_in.is_none() {
        p.bass_in = Some(r.bass_in);
        p.mic_in = r.mic_in;
        p.outputs = r.outputs.clone();
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{OutputRoute, Source};

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("namfoop-setup-card-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn filled(new_out: usize) -> Config {
        let mut c = Config::default();
        c.audio.backend = Some(model::BackendKind::Alsa);
        c.audio.device = Some("hw:MTK".into());
        c.audio.buffer = Some(48);
        c.audio.periods = Some(3);
        c.routing.bass_in = 1;
        c.routing.outputs = vec![OutputRoute { channel: new_out, source: Source::Processed, click: true }];
        c
    }

    const MTK: Device = Device { id: "hw:MTK", label: "Soundcraft Signature 12 MTK (hw:MTK)" };

    /// La config d'avant (l'iTwo, son routage) est rangée dans le profil de l'iTwo, la MTK reçoit
    /// le sien, la config ne garde que la préférée — et régler à nouveau la MTK ne touche pas à
    /// l'iTwo.
    #[test]
    fn setting_a_second_card_keeps_the_first_one() {
        let d = tmp("second");
        let path = d.join("config.toml");
        std::fs::write(
            &path,
            "trim_db = -6.0\n[audio]\nbackend = \"alsa\"\nprofile = \"itwo\"\ndevice = \"hw:iTwo\"\nbuffer = 64\n\
             [routing]\nbass_in = 1\nmic_in = 2\n[[routing.outputs]]\nchannel = 2\nsource = \"di\"\n",
        )
        .unwrap();
        let conf = LoadedConfig::load(Some(path.clone()), false).unwrap();
        let mut new = filled(6);
        let saved = save(&conf, &mut new, &MTK, |_| false).unwrap();
        assert_eq!(
            (saved.id.as_str(), saved.preferred.as_str()),
            ("mtk", "itwo"),
            "« non » : l'iTwo reste préférée"
        );
        let itwo = CardProfile::load(saved.moved.as_ref().unwrap()).unwrap();
        assert_eq!((itwo.device.as_deref(), itwo.buffer, itwo.mic_in), (Some("hw:iTwo"), Some(64), Some(2)));
        assert_eq!(itwo.outputs, vec![OutputRoute { channel: 2, source: Source::Di, click: false }]);
        assert_eq!(itwo.name, "PreSonus AudioBox iTwo", "le profil livré, complété");
        let mtk = CardProfile::load(&saved.path).unwrap();
        assert_eq!(
            (mtk.device.as_deref(), mtk.buffer, mtk.outputs[0].channel),
            (Some("hw:MTK"), Some(48), 6)
        );
        assert_eq!(
            new.audio,
            AudioConfig {
                backend: Some(model::BackendKind::Alsa),
                profile: Some("itwo".into()),
                ..Default::default()
            }
        );
        assert_eq!(new.routing, Routing::default());
        assert_eq!(new.trim_db, 0.0, "le reste de la config est celui qu'on lui donne");

        // la config écrite, régler la MTK une seconde fois ne touche pas au profil de l'iTwo
        new.save(&path).unwrap();
        let before = std::fs::read(saved.moved.unwrap()).unwrap();
        let conf = LoadedConfig::load(Some(path), false).unwrap();
        let mut again = filled(5);
        let saved = save(&conf, &mut again, &MTK, |_| true).unwrap();
        assert_eq!(
            (saved.moved, saved.preferred.as_str()),
            (None, "mtk"),
            "« oui » : la MTK devient préférée"
        );
        assert_eq!(std::fs::read(d.join("profiles/itwo.toml")).unwrap(), before);
        assert_eq!(CardProfile::load(&saved.path).unwrap().outputs[0].channel, 5);
        let _ = std::fs::remove_dir_all(d);
    }

    /// La première carte réglée devient la préférée, sans question.
    #[test]
    fn the_first_set_card_is_preferred_without_asking() {
        let d = tmp("first");
        let conf = LoadedConfig::load(Some(d.join("config.toml")), false).unwrap();
        let mut new = filled(1);
        let saved = save(&conf, &mut new, &MTK, |_| panic!("pas de question")).unwrap();
        assert_eq!((saved.preferred.as_str(), saved.moved), ("mtk", None));
        let _ = std::fs::remove_dir_all(d);
    }
}
