//! Contrôles mixer ALSA réglés à l'ouverture de la carte (profil de carte `[[mixer]]`), hors temps
//! réel. Un contrôle absent ou une valeur refusée donne un avertissement, jamais un échec : la
//! carte doit s'ouvrir quand même.

use alsa::mixer::{MilliBel, Mixer, Selem, SelemChannelId, SelemId};
use alsa::Round;

/// Valeur demandée pour un contrôle.
#[derive(Clone, Debug, PartialEq)]
pub enum MixerValue {
    /// « 80% » : part de la plage du contrôle.
    Percent(f32),
    /// « -6dB ».
    Db(f32),
    /// « on » / « off ».
    Switch(bool),
    /// Entier brut du pilote.
    Raw(i64),
    /// Nom d'un choix (contrôle énuméré, ex. sélection d'entrée).
    Item(String),
}

pub fn parse_value(v: &str) -> MixerValue {
    let t = v.trim();
    let l = t.to_lowercase();
    if let Some(p) = l.strip_suffix('%').and_then(|n| n.trim().parse::<f32>().ok()) {
        return MixerValue::Percent((p / 100.0).clamp(0.0, 1.0));
    }
    if let Some(d) = l.strip_suffix("db").and_then(|n| n.trim().parse::<f32>().ok()) {
        return MixerValue::Db(d);
    }
    match l.as_str() {
        "on" | "true" | "yes" => return MixerValue::Switch(true),
        "off" | "false" | "no" => return MixerValue::Switch(false),
        _ => {}
    }
    match t.parse::<i64>() {
        Ok(n) => MixerValue::Raw(n),
        Err(_) => MixerValue::Item(t.to_string()),
    }
}

/// Carte ALSA d'un device : « hw:iTwo,0 » -> « hw:iTwo » ; « plughw:1 » -> « hw:1 ».
pub fn card_of(device: &str) -> Option<String> {
    let rest = device.split_once(':')?.1;
    let id = rest.strip_prefix("CARD=").unwrap_or(rest).split(',').next()?;
    (!id.is_empty()).then(|| format!("hw:{id}"))
}

fn set(s: &Selem, v: &MixerValue) -> Result<(), String> {
    let e = |e: alsa::Error| e.to_string();
    let capture = s.has_capture_volume();
    match v {
        MixerValue::Percent(p) if capture || s.has_playback_volume() => {
            let (min, max) =
                if capture { s.get_capture_volume_range() } else { s.get_playback_volume_range() };
            let raw = min + ((max - min) as f32 * p).round() as i64;
            if capture { s.set_capture_volume_all(raw) } else { s.set_playback_volume_all(raw) }.map_err(e)
        }
        MixerValue::Db(d) if capture || s.has_playback_volume() => {
            let mb = MilliBel::from_db(*d);
            if capture {
                s.set_capture_db_all(mb, Round::Floor)
            } else {
                s.set_playback_db_all(mb, Round::Floor)
            }
            .map_err(e)
        }
        MixerValue::Raw(n) if capture || s.has_playback_volume() => {
            if capture { s.set_capture_volume_all(*n) } else { s.set_playback_volume_all(*n) }.map_err(e)
        }
        MixerValue::Switch(on) if s.has_capture_switch() || s.has_playback_switch() => {
            let x = *on as i32;
            if s.has_capture_switch() { s.set_capture_switch_all(x) } else { s.set_playback_switch_all(x) }
                .map_err(e)
        }
        MixerValue::Item(name) if s.is_enumerated() => {
            let names: Vec<String> = s.iter_enum().map_err(e)?.filter_map(|n| n.ok()).collect();
            let idx = names
                .iter()
                .position(|n| n.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("choix « {name} » inconnu ({})", names.join(", ")))?;
            s.set_enum_item(SelemChannelId::mono(), idx as u32).map_err(e)
        }
        _ => Err("valeur non applicable à ce contrôle".into()),
    }
}

/// Règle les contrôles `settings` (nom, valeur) de la carte de `device`.
pub fn apply(device: &str, settings: &[(String, String)]) {
    if settings.is_empty() {
        return;
    }
    let Some(card) = card_of(device) else {
        log::warn!("contrôles mixer ignorés : carte inconnue pour « {device} »");
        return;
    };
    let mixer = match Mixer::new(&card, false) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("contrôles mixer ignorés : mixer de {card} inaccessible ({e})");
            return;
        }
    };
    for (control, value) in settings {
        let Some(s) = mixer.find_selem(&SelemId::new(control, 0)) else {
            log::warn!("mixer {card} : contrôle « {control} » introuvable (amixer -c … scontrols)");
            continue;
        };
        match set(&s, &parse_value(value)) {
            Ok(()) => log::info!("mixer {card} : {control} = {value}"),
            Err(e) => log::warn!("mixer {card} : {control} = {value} refusé : {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        assert_eq!(parse_value("80%"), MixerValue::Percent(0.8));
        assert_eq!(parse_value(" 150 % "), MixerValue::Percent(1.0));
        assert_eq!(parse_value("-6dB"), MixerValue::Db(-6.0));
        assert_eq!(parse_value("+3 db"), MixerValue::Db(3.0));
        assert_eq!(parse_value("On"), MixerValue::Switch(true));
        assert_eq!(parse_value("off"), MixerValue::Switch(false));
        assert_eq!(parse_value("42"), MixerValue::Raw(42));
        assert_eq!(parse_value("Line In"), MixerValue::Item("Line In".into()));
    }

    #[test]
    fn cards() {
        assert_eq!(card_of("hw:iTwo").as_deref(), Some("hw:iTwo"));
        assert_eq!(card_of("hw:CARD=sndrpihifiberry,DEV=0").as_deref(), Some("hw:sndrpihifiberry"));
        assert_eq!(card_of("plughw:1,0").as_deref(), Some("hw:1"));
        assert_eq!(card_of("default"), None);
    }

    /// Carte absente : avertissement seulement, pas de panique.
    #[test]
    fn missing_card_is_not_fatal() {
        apply("hw:NamfoopNexistePas", &[("Capture".into(), "80%".into())]);
    }
}
