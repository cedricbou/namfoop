//! **Mes cartes** (`docs/design/instruments.md` § 12) : quelle carte réglée joue. Une carte est
//! « réglée » quand `setup` a écrit son profil sur cet ordinateur (`profiles/` à côté de la
//! config) — un profil livré ne dit pas où vont les sorties —, ou quand la config d'avant porte
//! elle-même une carte : ses réglages valent pour cette carte, qui est alors la préférée.
//!
//! Le choix (`choose`) est pur : `--card` ; la préférée si elle est branchée ; l'unique autre
//! carte réglée qui est branchée, annoncée ; sinon un refus qui liste ce qu'il voit. namfoop ne
//! joue jamais sur une carte que l'utilisateur n'a pas réglée.

use std::path::Path;

use anyhow::Result;
use model::CardProfile;

use crate::config::LoadedConfig;

/// Une carte réglée : son identifiant (celui des calibrations), son profil, et si les réglages
/// `[audio]` / `[routing]` de la config d'avant sont les siens.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: String,
    pub profile: CardProfile,
    pub of_config: bool,
}

/// L'identifiant d'une carte, tiré du nom que le système lui donne — jamais de son numéro, qui
/// change avec l'ordre de branchement : `hw:1` et `hw:iTwo` donnent `itwo` quand la carte 1
/// s'appelle `iTwo`. `name_of` donne le nom système d'une carte par son numéro.
pub fn system_id(device: &str, name_of: impl Fn(u32) -> Option<String>) -> String {
    let card = device.trim_start_matches("hw:").trim_start_matches("plughw:");
    let card = card.split(',').next().unwrap_or("");
    let card = match card.parse::<u32>() {
        Ok(n) => name_of(n).unwrap_or_else(|| card.to_string()),
        Err(_) => card.to_string(),
    };
    library::instrument::slug(&card)
}

/// Le nom système d'une carte ALSA par son numéro (`/proc/asound/cardN/id`).
pub fn alsa_name(n: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/asound/card{n}/id")).ok().map(|s| s.trim().to_string())
}

/// Les profils écrits sur cet ordinateur (pas les profils livrés), triés par identifiant.
fn written(dir: &Path) -> Result<Vec<(String, CardProfile)>> {
    let all = model::card::all_profiles(Some(dir))?;
    Ok(all.into_iter().filter(|(id, _)| dir.join(format!("{id}.toml")).is_file()).collect())
}

/// Les cartes réglées sur cet ordinateur : les profils écrits par `setup`, plus la carte que la
/// config d'avant porte elle-même (`[audio] device`, avec ou sans profil). Vide avec
/// `--no-config`.
pub fn configured(conf: &LoadedConfig) -> Result<Vec<Card>> {
    if conf.ignored {
        return Ok(vec![]);
    }
    let dir = model::card::user_dir(&conf.path);
    let a = &conf.audio;
    let mut out: Vec<Card> = written(&dir)?
        .into_iter()
        .map(|(id, profile)| Card {
            of_config: a.device.is_some() && a.profile.as_ref() == Some(&id),
            id,
            profile,
        })
        .collect();
    if let Some(device) = a.device.as_deref().filter(|_| !out.iter().any(|c| c.of_config)) {
        let card = match &a.profile {
            // le profil livré que la config nomme (l'iTwo réglée avant les profils écrits)
            Some(id) => model::card::all_profiles(Some(&dir))?
                .into_iter()
                .find(|(i, _)| i == id)
                .map(|(id, profile)| Card { id, profile, of_config: true }),
            None => None,
        };
        let card = card.unwrap_or_else(|| {
            let id = system_id(device, alsa_name);
            let name = device.trim_start_matches("hw:").split(',').next().unwrap_or(device).to_string();
            let profile = CardProfile {
                name: name.clone(),
                match_name: name,
                device: Some(device.to_string()),
                ..Default::default()
            };
            Card { id, profile, of_config: true }
        });
        out.retain(|c| c.id != card.id);
        out.push(card);
    }
    Ok(out)
}

/// La carte préférée : `[audio] profile`, sinon la carte de la config d'avant.
pub fn preferred<'a>(conf: &'a LoadedConfig, cards: &'a [Card]) -> Option<&'a str> {
    conf.audio.profile.as_deref().or_else(|| cards.iter().find(|c| c.of_config).map(|c| c.id.as_str()))
}

/// La carte choisie et la ligne de démarrage qui l'annonce.
#[derive(Debug, PartialEq)]
pub struct Choice {
    pub card: Card,
    pub line: String,
}

fn plugged(p: &CardProfile, seen: &[(String, String)]) -> bool {
    seen.iter().any(|(id, name)| p.matches(id) || p.matches(name))
}

fn names(cards: &[&Card]) -> String {
    cards.iter().map(|c| format!("{} (--card {})", c.profile.name, c.id)).collect::<Vec<_>>().join(", ")
}

/// Le choix de la carte (§ 12), pur. `seen` : les cartes branchées (`hw:<id>`, nom). L'erreur est
/// le message à montrer.
pub fn choose(
    cards: &[Card],
    seen: &[(String, String)],
    preferred: Option<&str>,
    asked: Option<&str>,
) -> std::result::Result<Choice, String> {
    let here: Vec<&Card> = cards.iter().filter(|c| plugged(&c.profile, seen)).collect();
    let line = |c: &Card, why: String| Choice {
        card: c.clone(),
        line: format!("carte       {}{why}", c.profile.name),
    };
    if let Some(q) = asked {
        let q = q.trim().to_lowercase();
        let Some(c) = cards.iter().find(|c| c.id == q) else {
            let ids: Vec<&str> = cards.iter().map(|c| c.id.as_str()).collect();
            return Err(format!(
                "--card {q} : aucune carte réglée de ce nom. Cartes réglées : {}. `namfoop setup` en règle une autre.",
                if ids.is_empty() { "aucune".to_string() } else { ids.join(", ") }
            ));
        };
        if !here.iter().any(|h| h.id == c.id) {
            return Err(format!("--card {q} : {} n'est pas branchée", c.profile.name));
        }
        return Ok(line(c, " (--card)".into()));
    }
    let pref = preferred.and_then(|p| cards.iter().find(|c| c.id == p));
    if let Some(p) = pref.filter(|p| here.iter().any(|h| h.id == p.id)) {
        let others: Vec<&Card> = here.iter().copied().filter(|h| h.id != p.id).collect();
        let why = if others.is_empty() {
            String::new()
        } else {
            format!(" — aussi branchée : {}", names(&others))
        };
        return Ok(line(p, why));
    }
    let absent = pref.map_or(String::new(), |p| format!("{} n'est pas branchée", p.profile.name));
    match here.as_slice() {
        [one] => Ok(line(one, if absent.is_empty() { absent } else { format!(" — {absent}") })),
        [] => {
            let set: Vec<&str> = cards.iter().map(|c| c.profile.name.as_str()).collect();
            let seen: Vec<String> = seen.iter().map(|(id, name)| format!("{id} ({name})")).collect();
            Err(format!(
                "aucune carte réglée n'est branchée\n  cartes réglées : {}\n  cartes vues : {}\n  \
                 branche l'une d'elles, ou lance « namfoop setup » pour régler celle qui est là",
                set.join(", "),
                if seen.is_empty() { "aucune".to_string() } else { seen.join(", ") }
            ))
        }
        several => Err(format!(
            "{}plusieurs cartes réglées sont branchées : {}. Choisis avec --card.",
            if absent.is_empty() { String::new() } else { format!("{absent}, et ") },
            names(several)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, name: &str, alsa: &str) -> Card {
        let profile = CardProfile { name: name.into(), match_name: alsa.into(), ..Default::default() };
        Card { id: id.into(), profile, of_config: false }
    }

    fn two() -> Vec<Card> {
        vec![
            card("itwo", "PreSonus AudioBox iTwo", "iTwo"),
            card("mtk", "Soundcraft Signature 12 MTK", "MTK"),
        ]
    }

    fn seen(ids: &[&str]) -> Vec<(String, String)> {
        ids.iter().map(|i| (format!("hw:{i}"), format!("carte {i}"))).collect()
    }

    #[test]
    fn the_preferred_card_wins_when_plugged_and_names_the_other() {
        let c = choose(&two(), &seen(&["iTwo", "PCH"]), Some("itwo"), None).unwrap();
        assert_eq!((c.card.id.as_str(), c.line.as_str()), ("itwo", "carte       PreSonus AudioBox iTwo"));
        let c = choose(&two(), &seen(&["MTK", "iTwo"]), Some("itwo"), None).unwrap();
        assert_eq!(c.card.id, "itwo");
        assert_eq!(
            c.line,
            "carte       PreSonus AudioBox iTwo — aussi branchée : Soundcraft Signature 12 MTK (--card mtk)"
        );
    }

    #[test]
    fn without_the_preferred_one_the_only_other_set_card_plays_and_says_so() {
        let c = choose(&two(), &seen(&["MTK", "PCH"]), Some("itwo"), None).unwrap();
        assert_eq!(c.card.id, "mtk");
        assert_eq!(
            c.line,
            "carte       Soundcraft Signature 12 MTK — PreSonus AudioBox iTwo n'est pas branchée"
        );
    }

    #[test]
    fn card_option_picks_a_set_and_plugged_card_only() {
        let c = choose(&two(), &seen(&["MTK", "iTwo"]), Some("itwo"), Some("MTK")).unwrap();
        assert_eq!(c.card.id, "mtk");
        let e = choose(&two(), &seen(&["iTwo"]), Some("itwo"), Some("mtk")).unwrap_err();
        assert!(e.contains("n'est pas branchée"), "{e}");
        let e = choose(&two(), &seen(&["iTwo"]), Some("itwo"), Some("umc")).unwrap_err();
        assert!(e.contains("aucune carte réglée de ce nom") && e.contains("itwo, mtk"), "{e}");
    }

    /// Jamais une carte que l'utilisateur n'a pas réglée : sans carte réglée branchée, un refus
    /// qui liste ce qu'il voit et renvoie vers `setup`.
    #[test]
    fn no_set_card_plugged_is_refused_with_what_is_seen() {
        let e = choose(&two(), &seen(&["PCH", "A49"]), Some("itwo"), None).unwrap_err();
        assert!(e.contains("aucune carte réglée n'est branchée") && e.contains("hw:PCH (carte PCH)"), "{e}");
        assert!(e.contains("namfoop setup"), "{e}");
    }

    #[test]
    fn several_set_cards_without_the_preferred_one_are_refused() {
        let mut cards = two();
        cards.push(card("umc", "Behringer UMC404HD", "U404HD"));
        let e = choose(&cards, &seen(&["MTK", "U404HD"]), Some("itwo"), None).unwrap_err();
        assert!(
            e.contains("iTwo n'est pas branchée") && e.contains("--card mtk") && e.contains("--card umc"),
            "{e}"
        );
    }

    /// Le numéro d'une carte change avec l'ordre de branchement ; son nom système, non.
    #[test]
    fn a_card_id_comes_from_its_system_name_never_its_number() {
        let name_of = |n| (n == 1).then(|| "iTwo".to_string());
        assert_eq!(system_id("hw:1", name_of), "itwo");
        assert_eq!(system_id("hw:iTwo", name_of), "itwo");
        assert_eq!(system_id("hw:1,0", name_of), "itwo");
        assert_eq!(system_id("hw:MTK", name_of), "mtk");
    }

    /// La config d'avant, qui porte elle-même sa carte et son routage, marche sans qu'on y touche :
    /// sa carte est réglée, et préférée.
    #[test]
    fn an_older_config_card_is_set_and_preferred() {
        let d = std::env::temp_dir().join(format!("namfoop-cards-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        let p = d.join("config.toml");
        std::fs::write(&p, "[audio]\nbackend = \"alsa\"\nprofile = \"itwo\"\ndevice = \"hw:iTwo\"\n")
            .unwrap();
        let conf = LoadedConfig::load(Some(p.clone()), false).unwrap();
        let cards = configured(&conf).unwrap();
        assert_eq!(cards.len(), 1);
        assert!(cards[0].of_config && cards[0].id == "itwo" && cards[0].profile.name.contains("iTwo"));
        assert_eq!(preferred(&conf, &cards), Some("itwo"));
        // une seconde carte réglée par setup s'ajoute, sans prendre la place de la première
        let mtk = CardProfile { name: "MTK".into(), match_name: "MTK".into(), ..Default::default() };
        mtk.save(&d.join("profiles/mtk.toml")).unwrap();
        let cards = configured(&conf).unwrap();
        let ids: Vec<(&str, bool)> = cards.iter().map(|c| (c.id.as_str(), c.of_config)).collect();
        assert_eq!(ids, [("mtk", false), ("itwo", true)]);
        // sans carte dans la config : seuls les profils écrits, pas le profil livré de l'iTwo
        std::fs::write(&p, "").unwrap();
        let conf = LoadedConfig::load(Some(p), false).unwrap();
        let ids: Vec<String> = configured(&conf).unwrap().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, ["mtk"]);
        let _ = std::fs::remove_dir_all(d);
    }
}
