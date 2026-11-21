//! Le **format texte** d'un message : une ligne, un message. C'est à la fois ce que lit le port
//! simulé et ce qu'affiche `namfoop midi` — un seul vocabulaire, donc ce qu'on voit à l'écran se
//! recopie dans un tube pour le reproduire.
//!
//! ```text
//! pc 3        Program Change 3 (de 1 à 128, la numérotation de namfoop)
//! cc 34 2     Control Change 34, valeur 2
//! clock       un top d'horloge (24 par noire)
//! start       Start          stop   Stop          continue   Continue
//! spp 64      Song Position Pointer, 64 doubles-croches
//! # ...       commentaire, ligne vide ignorée
//! ```

use crate::error::{bail, Result};
use crate::Event;

/// Lit une ligne du format texte. `Ok(None)` : ligne vide ou commentaire, à sauter.
pub fn parse_line(line: &str) -> Result<Option<Event>> {
    let line = line.split('#').next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    let mut mots = line.split_whitespace();
    let verbe = mots.next().unwrap_or("").to_ascii_lowercase();
    let reste: Vec<&str> = mots.collect();
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
    Ok(Some(event))
}

fn sept_bits(quoi: &str, mot: &str) -> Result<u8> {
    match mot.parse::<u8>() {
        Ok(v) if v <= 127 => Ok(v),
        _ => bail!("{quoi} « {mot} » : une valeur MIDI de 0 à 127"),
    }
}

/// Ce qu'on affiche pour un message reçu. Se relit par [`parse_line`], au cas près.
pub fn describe(event: &Event) -> String {
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
            assert_eq!(parse_line(ligne).expect(ligne), Some(attendu), "{ligne}");
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
            let texte = describe(&e);
            assert_eq!(parse_line(&texte).expect(&texte), Some(e), "{texte}");
        }
    }
}
