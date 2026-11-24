//! Le **format texte** d'un message : une ligne, un message. C'est à la fois ce que lit le port
//! simulé et ce qu'affiche `namfoop midi` — un seul vocabulaire, donc ce qu'on voit à l'écran se
//! recopie dans un tube pour le reproduire.
//!
//! ```text
//! pc 3        Program Change 3 (de 1 à 128, la numérotation de namfoop)
//! cc 34 2     Control Change 34, valeur 2
//! pc 3 ch 5   le même, sur le canal 5 (de 1 à 16) ; sans « ch », le message n'a pas de canal
//! clock       un top d'horloge (24 par noire)
//! start       Start          stop   Stop          continue   Continue
//! spp 64      Song Position Pointer, 64 doubles-croches
//! # ...       commentaire, ligne vide ignorée
//! ```
//!
//! Seuls `pc` et `cc` ont un canal : l'horloge, le transport et la position n'en ont pas sur le
//! câble, et une ligne qui leur en donne un est refusée plutôt que de laisser croire qu'il compte.

use crate::error::{bail, Result};
use crate::Event;

/// Lit une ligne du format texte : le message et son canal (`None` : écrit sans canal).
/// `Ok(None)` : ligne vide ou commentaire, à sauter.
pub fn parse_line(line: &str) -> Result<Option<(Event, Option<u8>)>> {
    let line = line.split('#').next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    let mut mots = line.split_whitespace();
    let verbe = mots.next().unwrap_or("").to_ascii_lowercase();
    let mut reste: Vec<&str> = mots.collect();
    let mut channel = None;
    if let [.., ch, n] = reste.as_slice() {
        if ch.eq_ignore_ascii_case("ch") {
            channel = match n.parse::<u8>() {
                Ok(v) if (1..=16).contains(&v) => Some(v),
                _ => bail!("« {line} » : ch « {n} » : un canal de 1 à 16"),
            };
            reste.truncate(reste.len() - 2);
        }
    }
    if channel.is_some() && !matches!(verbe.as_str(), "pc" | "cc") {
        bail!("« {line} » : {verbe} n'a pas de canal (seuls pc et cc en ont un)");
    }
    let event = match (verbe.as_str(), reste.as_slice()) {
        ("pc", [n]) => match n.parse::<u8>() {
            Ok(v) if (1..=128).contains(&v) => Event::ProgramChange(v),
            _ => bail!("pc « {n} » : un Program Change de 1 à 128"),
        },
        ("cc", [n, v]) => Event::Control { cc: sept_bits("cc", n)?, value: sept_bits("cc", v)? },
        ("clock", []) => Event::ClockTick,
        ("start", []) => Event::ClockStart,
        ("stop", []) => Event::ClockStop,
        ("continue", []) => Event::ClockContinue,
        ("spp", [n]) => match n.parse::<u16>() {
            Ok(v) if v <= 16383 => Event::SongPosition(v),
            _ => bail!("spp « {n} » : une position de 0 à 16383 (en doubles-croches)"),
        },
        ("pc" | "spp", _) => bail!("« {line} » : {verbe} attend une valeur (ex. « {verbe} 3 »)"),
        ("cc", _) => bail!("« {line} » : cc attend un numéro et une valeur (ex. « cc 34 2 »)"),
        ("clock" | "start" | "stop" | "continue", _) => {
            bail!("« {line} » : {verbe} ne prend pas de valeur")
        }
        _ => bail!("« {line} » : message inconnu (pc, cc, clock, start, stop, continue, spp)"),
    };
    Ok(Some((event, channel)))
}

fn sept_bits(quoi: &str, mot: &str) -> Result<u8> {
    match mot.parse::<u8>() {
        Ok(v) if v <= 127 => Ok(v),
        _ => bail!("{quoi} « {mot} » : une valeur MIDI de 0 à 127"),
    }
}

/// Ce qu'on affiche pour un message reçu, et son canal s'il en a un (`PC 3 ch 5`). Se relit par
/// [`parse_line`], au cas près : ce qu'on voit se recopie dans un port simulé.
pub fn describe(event: &Event, channel: Option<u8>) -> String {
    let texte = describe_event(event);
    match channel {
        Some(c) => format!("{texte} ch {c}"),
        None => texte,
    }
}

fn describe_event(event: &Event) -> String {
    match event {
        Event::ProgramChange(n) => format!("PC {n}"),
        Event::Control { cc, value } => format!("CC {cc} {value}"),
        Event::ClockTick => "clock".into(),
        Event::ClockStart => "start".into(),
        Event::ClockStop => "stop".into(),
        Event::ClockContinue => "continue".into(),
        Event::SongPosition(n) => format!("SPP {n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_message_se_lit() {
        let cas = [
            ("pc 1", Event::ProgramChange(1)),
            ("PC 128", Event::ProgramChange(128)),
            ("cc 34 2", Event::Control { cc: 34, value: 2 }),
            ("  cc 14 127  ", Event::Control { cc: 14, value: 127 }),
            ("clock", Event::ClockTick),
            ("start", Event::ClockStart),
            ("stop", Event::ClockStop),
            ("continue", Event::ClockContinue),
            ("spp 16383", Event::SongPosition(16383)),
            ("pc 3 # le morceau 4", Event::ProgramChange(3)),
        ];
        for (ligne, attendu) in cas {
            assert_eq!(parse_line(ligne).expect(ligne), Some((attendu, None)), "{ligne}");
        }
    }

    #[test]
    fn un_canal_se_lit_en_suffixe() {
        let cas = [
            ("pc 3 ch 5", Event::ProgramChange(3), 5),
            ("cc 34 2 ch 1", Event::Control { cc: 34, value: 2 }, 1),
            ("CC 20 1 CH 16 # Ableton", Event::Control { cc: 20, value: 1 }, 16),
        ];
        for (ligne, attendu, canal) in cas {
            assert_eq!(parse_line(ligne).expect(ligne), Some((attendu, Some(canal))), "{ligne}");
        }
    }

    #[test]
    fn un_canal_faux_ou_sans_objet_est_refuse() {
        for ligne in ["pc 3 ch 0", "pc 3 ch 17", "cc 34 2 ch x", "clock ch 1", "start ch 2", "spp 64 ch 3"] {
            let e = parse_line(ligne).expect_err(ligne).to_string();
            assert!(e.contains("canal"), "{ligne} -> {e}");
        }
    }

    #[test]
    fn une_ligne_vide_ou_commentee_ne_donne_rien() {
        for ligne in ["", "   ", "# rien", "\t# pc 3"] {
            assert_eq!(parse_line(ligne).expect(ligne), None, "{ligne}");
        }
    }

    #[test]
    fn une_ligne_fausse_dit_ce_qu_elle_attendait() {
        for ligne in
            ["pc", "pc 0", "pc 129", "pc -1", "cc 34", "cc 34 999", "clock 24", "spp 16384", "bidule"]
        {
            let e = parse_line(ligne).expect_err(ligne).to_string();
            assert!(!e.is_empty(), "{ligne}");
            assert!(e.contains(ligne.split_whitespace().next().unwrap_or("")), "{ligne} -> {e}");
        }
    }

    #[test]
    fn ce_qu_on_affiche_se_relit() {
        let tous = [
            Event::ProgramChange(3),
            Event::Control { cc: 34, value: 2 },
            Event::ClockTick,
            Event::ClockStart,
            Event::ClockStop,
            Event::ClockContinue,
            Event::SongPosition(64),
        ];
        for e in tous {
            let texte = describe(&e, None);
            assert_eq!(parse_line(&texte).expect(&texte), Some((e, None)), "{texte}");
        }
        for e in [Event::ProgramChange(3), Event::Control { cc: 34, value: 2 }] {
            for canal in [1, 5, 16] {
                let texte = describe(&e, Some(canal));
                assert_eq!(parse_line(&texte).expect(&texte), Some((e, Some(canal))), "{texte}");
            }
        }
    }
}
