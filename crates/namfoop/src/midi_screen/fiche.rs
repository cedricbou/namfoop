//! **La fiche** (`docs/design/midi.md` § 7) : le texte à envoyer au groupe pour un canal, calculé
//! depuis les réglages MIDI et le set du moment — même non enregistrés. Fonction pure : l'onglet
//! Fiche, le fichier `fiche-midi.txt` et `namfoop midi --fiche` l'appellent tous les trois.
//!
//! **Du texte nu**, fait pour être collé dans un message et lu sur un téléphone : lignes de
//! [`WIDTH`] colonnes au plus, ni tableau ni cadre, aucune mise en forme propre à une messagerie.

use library::set::SetSpec;
use model::{Gesture, Midi};

use crate::cli_midi::Listen;

/// La largeur maximale d'une ligne : ce qu'un téléphone montre sans couper.
pub const WIDTH: usize = 46;

/// Le nom du fichier que l'onglet écrit dans le dossier courant.
pub const FILE: &str = "fiche-midi.txt";

fn pad(s: &str, n: usize) -> String {
    let len = s.chars().count();
    if len > n {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        format!("{s}{}", " ".repeat(n - len))
    }
}

fn cut(s: String) -> String {
    if s.chars().count() <= WIDTH {
        s
    } else {
        pad(&s, WIDTH)
    }
}

/// La fiche du canal `channel` (`None` : aucun canal réglé, les numéros valent pour tous). Le set
/// est celui du moment ; `set_name` le nomme s'il n'a pas de nom.
pub fn lines(midi: &Midi, channel: Option<u8>, set: Option<(&SetSpec, &str)>) -> Vec<String> {
    let n = Listen::from_config(midi, None).numbers(channel);
    let mut out = vec!["Basse (namfoop) · ce que j'attends en MIDI".to_string(), String::new()];
    out.push(match channel {
        Some(c) => format!("Canal {c}. Les autres sont ignorés."),
        None => "Canal : à choisir. Lequel tu me réserves ?".into(),
    });
    if !n.muted.has(Gesture::Pc) {
        out.push("Morceau : Program Change, compté de 1 à 128".into());
        out.push("  (logiciel qui compte de 0 à 127 : retire 1)".into());
    }
    if !n.muted.has(Gesture::Sound) {
        out.push(format!("Son du morceau : CC {}, valeur 0 = 1er son", n.cc_sound));
    }
    if !n.muted.has(Gesture::Tap) {
        out.push(format!("Tap tempo : CC {}", n.cc_tap));
    }
    out.push("Tempo : horloge MIDI, Start, Stop, position".into());
    if !n.muted.has(Gesture::Tuner) {
        out.push(format!("Accordeur : CC {} = 127 coupe ma basse", n.cc_tuner));
        out.push(format!("  CC {} = 0, un PC ou Start la rendent", n.cc_tuner));
    }
    out.push("Je n'envoie rien. Le reste est ignoré.".into());
    if let Some((spec, stem)) = set {
        out.push(String::new());
        let name = if spec.name.trim().is_empty() { stem } else { spec.name.as_str() };
        out.push(cut(format!("Set « {name} »")));
        for (i, song) in spec.songs.iter().enumerate() {
            let pc = song.pc.map_or("PC ?".to_string(), |p| format!("PC {p}"));
            let tempo = song.tempo_bpm.map(|b| format!("♩{b:.0}")).unwrap_or_default();
            let reste = if song.pc.is_none() { "  à convenir" } else { "" };
            let titre = crate::midi_rules::title(spec, i);
            out.push(cut(format!("{}{}{tempo}{reste}", pad(&pc, 7), pad(&titre, 18)).trim_end().to_string()));
            if !n.muted.has(Gesture::Sound) {
                for (k, s) in song.sounds.iter().enumerate() {
                    out.push(cut(format!("       CC {} = {k}  {}", n.cc_sound, s.preset)));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use library::set::{Song, Sound};

    fn set() -> SetSpec {
        let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
        SetSpec {
            name: "Groupe 1 · Live Mairie".into(),
            songs: vec![
                Song {
                    title: Some("Le Phare".into()),
                    pc: Some(2),
                    tempo_bpm: Some(124.0),
                    sounds: vec![sound("fuzz"), sound("clean")],
                    ..Default::default()
                },
                Song {
                    title: Some("Un titre beaucoup trop long pour un téléphone".into()),
                    pc: None,
                    tempo_bpm: Some(110.0),
                    sounds: vec![sound("un preset au nom démesurément long, vraiment")],
                    ..Default::default()
                },
            ],
            base: Default::default(),
        }
    }

    fn midi() -> Midi {
        Midi {
            channels: vec![model::MidiChannel {
                channel: 5,
                cc_sound: Some(20),
                cc_tap: None,
                cc_tuner: None,
                muted: vec![],
            }],
            ..Default::default()
        }
    }

    /// Le texte de référence du § 7 de la spécification, pour le canal 5.
    #[test]
    fn la_fiche_du_canal_5() {
        let s = set();
        let l = lines(&midi(), Some(5), Some((&s, "groupe-1")));
        assert_eq!(
            &l[..12],
            [
                "Basse (namfoop) · ce que j'attends en MIDI",
                "",
                "Canal 5. Les autres sont ignorés.",
                "Morceau : Program Change, compté de 1 à 128",
                "  (logiciel qui compte de 0 à 127 : retire 1)",
                "Son du morceau : CC 20, valeur 0 = 1er son",
                "Tap tempo : CC 14",
                "Tempo : horloge MIDI, Start, Stop, position",
                "Accordeur : CC 15 = 127 coupe ma basse",
                "  CC 15 = 0, un PC ou Start la rendent",
                "Je n'envoie rien. Le reste est ignoré.",
                "",
            ]
        );
        assert!(l.contains(&"Set « Groupe 1 · Live Mairie »".to_string()));
        assert!(l.contains(&"PC 2   Le Phare          ♩124".to_string()), "{l:#?}");
        assert!(l.contains(&"       CC 20 = 1  clean".to_string()), "{l:#?}");
        assert!(l.iter().any(|x| x.starts_with("PC ?") && x.ends_with("à convenir")), "{l:#?}");
    }

    #[test]
    fn aucune_ligne_ne_depasse_la_largeur_d_un_telephone() {
        let s = set();
        for l in lines(&midi(), Some(5), Some((&s, "x"))) {
            assert!(l.chars().count() <= WIDTH, "{} colonnes : {l}", l.chars().count());
        }
    }

    #[test]
    fn un_geste_coupe_n_y_figure_pas_et_sans_canal_on_demande_lequel() {
        let m = Midi { muted: vec![Gesture::Tap, Gesture::Tuner], ..Default::default() };
        let l = lines(&m, None, None);
        assert!(l.contains(&"Canal : à choisir. Lequel tu me réserves ?".to_string()));
        assert!(!l.iter().any(|x| x.starts_with("Tap tempo") || x.starts_with("Accordeur")), "{l:#?}");
        assert!(!l.iter().any(|x| x.starts_with("Set")), "sans set, pas de liste");
    }
}
