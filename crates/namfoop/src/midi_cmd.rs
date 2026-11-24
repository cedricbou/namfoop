//! `namfoop midi` : les ports MIDI d'entrée de la machine, puis **ce qui en sort**, en clair.
//!
//! C'est la commande qui répond à « est-ce que mon pédalier parle ? » avant un concert, sans
//! ouvrir la carte son ni l'assistant : on appuie sur une pédale, et la ligne s'affiche. Les
//! messages sont écrits dans le même format que celui que lit un port simulé (`midi_io::text`) :
//! ce qu'on voit se recopie dans un tube pour le rejouer.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cli_midi::MidiArgs;
use crate::config::LoadedConfig;

/// Tops d'horloge par noire (protocole MIDI) : une ligne de tempo par noire reçue.
const TOPS_PAR_NOIRE: u32 = 24;

#[derive(Clone, Debug, Default, clap::Args)]
pub struct MidiCmdArgs {
    /// Liste les ports et s'arrête, sans écouter
    #[arg(long)]
    pub list: bool,
    /// Arrêt automatique après N secondes (tests) [défaut : Ctrl+C]
    #[arg(long)]
    pub seconds: Option<f64>,
}

pub fn run(args: &MidiCmdArgs, midi: &MidiArgs, conf: &LoadedConfig) -> Result<()> {
    let demande = midi.resolve(&conf.config);
    let ports = midi_io::ports().unwrap_or_default();
    if ports.is_empty() {
        println!(
            "Aucun port MIDI d'entrée. Branche le pédalier (ou lance Ableton), puis relance \
             `namfoop midi`.\nSans matériel, un port simulé fait l'affaire :\n  \
             mkfifo /tmp/m && namfoop --midi-in sim:/tmp/m midi\n  echo 'pc 3' > /tmp/m   (depuis \
             un autre terminal)"
        );
    } else {
        println!("Ports d'entrée MIDI ({}) :", ports.len());
        for p in &ports {
            println!("  {}", p.label());
        }
    }
    if args.list {
        if demande.is_none() {
            println!("\nL'écoute est coupée (--no-midi ou [midi] off).");
        }
        return Ok(());
    }
    let Some(cfg) = demande else {
        println!("\nL'écoute est coupée (--no-midi ou [midi] off) : rien à montrer.");
        return Ok(());
    };
    // Un port nommé et introuvable fait **échouer** cette commande-là : on l'a lancée pour
    // vérifier un pédalier, et son message dit déjà quoi faire. Pendant une session de jeu, la
    // même erreur n'est qu'un avertissement (`cli_midi::MidiArgs::open`).
    let mut input = midi_io::open(&cfg)?;
    println!(
        "\nÉcoute {} — {} :",
        input.description(),
        match args.seconds {
            Some(s) => format!("{s:.0} s"),
            None => "Ctrl+C pour sortir".into(),
        }
    );

    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    ctrlc::set_handler(move || s.store(true, Ordering::Relaxed)).ok();
    let debut = Instant::now();
    let fin = args.seconds.map(|s| debut + Duration::from_secs_f64(s));
    let mut horloge = Horloge::default();
    let cc_tuner = conf.config.midi.clone().unwrap_or_default().cc_tuner;
    let mut vus = 0u64;
    while !stop.load(Ordering::Relaxed) && fin.is_none_or(|f| Instant::now() < f) {
        while let Some(m) = input.try_pop() {
            vus += 1;
            if let Some(ligne) = horloge.ligne(&m) {
                println!(
                    "  {:>7.2} s  {ligne}{}",
                    (m.at - debut).as_secs_f64(),
                    en_clair(&m.event, cc_tuner)
                );
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let perdus = input.dropped();
    println!("\n{vus} message(s) reçu(s) en {:.1} s.", debut.elapsed().as_secs_f64());
    if perdus > 0 {
        println!("⚠ {perdus} message(s) perdus : la file était pleine.");
    }
    if vus == 0 {
        println!(
            "Rien n'est arrivé. Vérifie que le pédalier envoie sur ce port (`aconnect -l`), et \
             qu'il est bien branché."
        );
    }
    Ok(())
}

/// Ce que namfoop fait d'un message, quand la ligne brute ne le dit pas : le CC de l'accordeur.
/// Écrit **après** le message, que la ligne garde recopiable dans un port simulé.
fn en_clair(event: &midi_io::Event, cc_tuner: u8) -> String {
    match event {
        midi_io::Event::Control { cc, value } if *cc == cc_tuner => {
            format!("   (accordeur : {})", if *value >= 64 { "basse coupée" } else { "son rendu" })
        }
        _ => String::new(),
    }
}

/// Les tops d'horloge ne s'affichent pas un par un : une ligne par noire, avec le tempo mesuré.
/// Un top isolé ne dit rien, et 24 lignes par noire noieraient tout le reste.
#[derive(Default)]
struct Horloge {
    tops: u32,
    debut_noire: Option<Instant>,
}

impl Horloge {
    /// La ligne à afficher pour ce message, s'il y en a une.
    fn ligne(&mut self, m: &midi_io::Message) -> Option<String> {
        if m.event != midi_io::Event::ClockTick {
            if matches!(m.event, midi_io::Event::ClockStart | midi_io::Event::ClockStop) {
                *self = Self::default();
            }
            return Some(midi_io::describe(&m.event, m.channel));
        }
        self.tops += 1;
        if self.tops < TOPS_PAR_NOIRE {
            return None;
        }
        self.tops = 0;
        let precedent = self.debut_noire.replace(m.at)?;
        let noire = (m.at - precedent).as_secs_f64();
        (noire > 0.0).then(|| format!("horloge  {:.1} BPM", 60.0 / noire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midi_io::{Event, Message};

    fn msg(event: Event, at: Instant) -> Message {
        Message { at, event, clock_seq: 0, channel: None }
    }

    #[test]
    fn le_cc_de_l_accordeur_se_lit_en_clair() {
        let on = Event::Control { cc: 15, value: 127 };
        assert_eq!(en_clair(&on, 15), "   (accordeur : basse coupée)");
        assert_eq!(en_clair(&Event::Control { cc: 15, value: 0 }, 15), "   (accordeur : son rendu)");
        assert_eq!(en_clair(&on, 20), "", "un autre numéro réglé : le 15 n'est plus l'accordeur");
        assert_eq!(en_clair(&Event::Control { cc: 34, value: 1 }, 15), "");
    }

    #[test]
    fn un_message_ordinaire_s_affiche_tel_quel() {
        let mut h = Horloge::default();
        let t = Instant::now();
        assert_eq!(h.ligne(&msg(Event::ProgramChange(3), t)).as_deref(), Some("PC 3"));
        assert_eq!(h.ligne(&msg(Event::Control { cc: 34, value: 2 }, t)).as_deref(), Some("CC 34 2"));
    }

    /// Une noire = 24 tops : rien ne s'affiche avant, et le tempo tombe juste ensuite.
    #[test]
    fn l_horloge_ne_parle_qu_une_fois_par_noire() {
        let mut h = Horloge::default();
        let t0 = Instant::now();
        let noire = Duration::from_millis(500); // 120 BPM
        let top = noire / TOPS_PAR_NOIRE;
        let mut lignes = Vec::new();
        for i in 0..(TOPS_PAR_NOIRE * 3) {
            if let Some(l) = h.ligne(&msg(Event::ClockTick, t0 + top * i)) {
                lignes.push(l);
            }
        }
        // 3 noires de tops : la 1re pose le repère, les deux suivantes donnent le tempo.
        assert_eq!(lignes.len(), 2, "une ligne par noire, la première servant de repère : {lignes:?}");
        for l in &lignes {
            assert!(l.starts_with("horloge  120.0 BPM"), "{l}");
        }
    }
}
