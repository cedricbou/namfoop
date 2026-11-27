//! `namfoop midi` : devant un terminal, **l'écran de réglage** (`crate::midi_screen`) ; sinon, ou
//! avec `--lines`, les ports MIDI d'entrée de la machine, puis **ce qui en sort**, en clair.
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
    /// Arrêt automatique après N secondes (tests ; affiche ligne à ligne) [défaut : Ctrl+C]
    #[arg(long)]
    pub seconds: Option<f64>,
    /// Affiche les messages ligne à ligne au lieu de l'écran de réglage (imposé quand la sortie
    /// n'est pas un terminal)
    #[arg(long)]
    pub lines: bool,
    /// Dossier de la bibliothèque, pour lire le set du moment (ce qu'un Program Change appelle)
    /// [défaut : ./library, sinon ~/.local/share/namfoop/library]
    #[arg(long, value_name = "DOSSIER")]
    pub library: Option<std::path::PathBuf>,
    /// Le set dont parlent l'écran et la fiche (identifiant ou morceau de nom, comme `browse
    /// --set`) [défaut : le set du moment, `[set] default`]
    #[arg(long, value_name = "SET")]
    pub set: Option<String>,
    /// Imprime la fiche à envoyer au groupe pour ce canal (sans valeur : le premier canal réglé),
    /// puis s'arrête, sans ouvrir de port ni l'écran
    #[arg(long, value_name = "CANAL", num_args = 0..=1, default_missing_value = "0",
          value_parser = clap::value_parser!(u8).range(0..=16))]
    pub fiche: Option<u8>,
}

pub fn run(
    args: &MidiCmdArgs,
    midi: &MidiArgs,
    conf: &LoadedConfig,
    library: &std::path::Path,
) -> Result<()> {
    use std::io::IsTerminal;
    if let Some(c) = args.fiche {
        return fiche(c, args.set.as_deref(), conf, library);
    }
    let demande = midi.resolve(&conf.config);
    // **L'écran de réglage** (`docs/design/midi.md`), quand on est devant un terminal. Le port
    // s'ouvre sans appareil décoché ni horloge imposée : l'écran applique son brouillon lui-même.
    let ecran = !args.list
        && !args.lines
        && args.seconds.is_none()
        && std::io::stdout().is_terminal()
        && std::io::stdin().is_terminal();
    if ecran {
        let Some(mut cfg) = demande else {
            println!("L'écoute est coupée (--no-midi ou [midi] off) : rien à régler.");
            return Ok(());
        };
        cfg.ignore.clear();
        cfg.clock_from = None;
        // un port imposé (sauf le port simulé, qui n'a qu'une source) : l'écran dit par qui
        let pinned = cfg
            .port
            .clone()
            .filter(|p| !p.starts_with("sim:"))
            .map(|asked| crate::midi_screen::Pinned { asked, by_option: midi.midi_in.is_some() });
        return crate::midi_screen::run(midi_io::open(&cfg)?, conf, library, args.set.as_deref(), pinned);
    }
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
    // le filtre du canal n'est **pas** appliqué ici : cette commande montre tout ce qui arrive, et
    // dit de ce qu'un écran ignorerait pourquoi il l'ignore
    let listen = midi.listen(&conf.config);
    println!(
        "\nÉcoute {}{} — {} :",
        input.description(),
        listen
            .label()
            .map(|l| format!(" · {} {l}", if listen.channels.len() > 1 { "canaux" } else { "canal" }))
            .unwrap_or_default(),
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
    let mut vus = 0u64;
    while !stop.load(Ordering::Relaxed) && fin.is_none_or(|f| Instant::now() < f) {
        while let Some(m) = input.try_pop() {
            vus += 1;
            if let Some(ligne) = horloge.ligne(&m, &input.device_name(m.device)) {
                let effet = match listen.why_not(m.channel) {
                    Some(raison) => format!("   ({raison})"),
                    None => en_clair(&m.event, &listen.numbers(m.channel)),
                };
                println!("  {:>7.2} s  {ligne}{effet}", (m.at - debut).as_secs_f64());
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let perdus = input.dropped();
    println!("\n{vus} message(s) reçu(s) en {:.1} s.", debut.elapsed().as_secs_f64());
    // une seule horloge est comptée : celle des autres appareils est dite, jamais additionnée
    for d in input.devices().iter().filter(|d| d.ignored_clock > 0) {
        println!(
            "⚠ horloge de [{}] ignorée ({} tops) : un autre appareil donne l'horloge.",
            d.name, d.ignored_clock
        );
    }
    for d in input.devices().iter().filter(|d| !d.listened) {
        println!("[{}] décoché : {} message(s) ignoré(s).", d.name, d.ignored);
    }
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

/// `--fiche` : la fiche du canal `c` (0 : le premier canal réglé), imprimée telle qu'elle part.
fn fiche(c: u8, set: Option<&str>, conf: &LoadedConfig, library: &std::path::Path) -> Result<()> {
    let midi = conf.config.midi.clone().unwrap_or_default();
    let tuned: Vec<u8> = midi.channels.iter().map(|x| x.channel).chain(midi.channel).collect();
    let canal = match c {
        0 => tuned.first().copied(),
        c if tuned.is_empty() || tuned.contains(&c) => Some(c),
        c => anyhow::bail!(
            "canal {c} : il n'est pas réglé (canaux réglés : {}) — `namfoop midi` pour l'ajouter",
            tuned.iter().map(u8::to_string).collect::<Vec<_>>().join(", ")
        ),
    };
    let set = crate::midi_screen::load_set(conf, library, set);
    let set = set.as_ref().map(|f| (&f.spec, f.stem.as_str()));
    for l in crate::midi_screen::fiche::lines(&midi, canal, set) {
        println!("{l}");
    }
    Ok(())
}

/// Ce que namfoop fait d'un message, quand la ligne brute ne le dit pas : le CC de l'accordeur et
/// ceux du morceau suivant / précédent, sauf s'ils sont coupés. Écrit **après** le message, que la
/// ligne garde recopiable dans un port simulé.
fn en_clair(event: &midi_io::Event, n: &crate::cli_midi::Numbers) -> String {
    let midi_io::Event::Control { cc, value } = *event else { return String::new() };
    if cc == n.cc_tuner && !n.muted.tuner {
        return format!("   (accordeur : {})", if value >= 64 { "basse coupée" } else { "son rendu" });
    }
    if cc == n.cc_instrument && !n.muted.instrument {
        return format!("   (instrument {})", value as u32 + 1);
    }
    let song = if cc == n.cc_next_song && !n.muted.next_song {
        "morceau suivant"
    } else if cc == n.cc_prev_song && !n.muted.prev_song {
        "morceau précédent"
    } else {
        return String::new();
    };
    if value >= 64 {
        format!("   ({song})")
    } else {
        format!("   ({song} : relâché, ignoré)")
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
    /// La ligne à afficher pour ce message de l'appareil `device`, s'il y en a une.
    fn ligne(&mut self, m: &midi_io::Message, device: &str) -> Option<String> {
        if m.event != midi_io::Event::ClockTick {
            if matches!(m.event, midi_io::Event::ClockStart | midi_io::Event::ClockStop) {
                *self = Self::default();
            }
            return Some(midi_io::describe_from(device, &m.event, m.channel));
        }
        self.tops += 1;
        if self.tops < TOPS_PAR_NOIRE {
            return None;
        }
        self.tops = 0;
        let precedent = self.debut_noire.replace(m.at)?;
        let noire = (m.at - precedent).as_secs_f64();
        (noire > 0.0).then(|| format!("[{device}] horloge  {:.1} BPM", 60.0 / noire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midi_io::{Event, Message};

    fn msg(event: Event, at: Instant) -> Message {
        Message { at, event, clock_seq: 0, channel: None, device: 0 }
    }

    #[test]
    fn le_cc_de_l_accordeur_se_lit_en_clair() {
        let on = Event::Control { cc: 15, value: 127 };
        let n = crate::cli_midi::Numbers::default();
        assert_eq!(en_clair(&on, &n), "   (accordeur : basse coupée)");
        assert_eq!(en_clair(&Event::Control { cc: 15, value: 0 }, &n), "   (accordeur : son rendu)");
        let other = crate::cli_midi::Numbers { cc_tuner: 20, ..n };
        assert_eq!(en_clair(&on, &other), "", "un autre numéro réglé : le 15 n'est plus l'accordeur");
        assert_eq!(en_clair(&Event::Control { cc: 34, value: 1 }, &n), "");
        let muted =
            crate::cli_midi::Numbers { muted: crate::cli_midi::Muted { tuner: true, ..n.muted }, ..n };
        assert_eq!(en_clair(&on, &muted), "", "l'accordeur coupé ne se traduit pas");
    }

    #[test]
    fn les_cc_de_morceau_se_lisent_en_clair() {
        let n = crate::cli_midi::Numbers::default();
        assert_eq!(en_clair(&Event::Control { cc: 104, value: 127 }, &n), "   (morceau suivant)");
        assert_eq!(en_clair(&Event::Control { cc: 103, value: 127 }, &n), "   (morceau précédent)");
        assert_eq!(en_clair(&Event::Control { cc: 102, value: 1 }, &n), "   (instrument 2)");
        assert_eq!(
            en_clair(&Event::Control { cc: 104, value: 0 }, &n),
            "   (morceau suivant : relâché, ignoré)"
        );
    }

    #[test]
    fn un_message_ordinaire_s_affiche_tel_quel() {
        let mut h = Horloge::default();
        let t = Instant::now();
        assert_eq!(h.ligne(&msg(Event::ProgramChange(3), t), "MC6 Pro").as_deref(), Some("[MC6 Pro] PC 3"));
        assert_eq!(
            h.ligne(&msg(Event::Control { cc: 34, value: 2 }, t), "sim").as_deref(),
            Some("[sim] CC 34 2")
        );
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
            if let Some(l) = h.ligne(&msg(Event::ClockTick, t0 + top * i), "UM-ONE") {
                lignes.push(l);
            }
        }
        // 3 noires de tops : la 1re pose le repère, les deux suivantes donnent le tempo.
        assert_eq!(lignes.len(), 2, "une ligne par noire, la première servant de repère : {lignes:?}");
        for l in &lignes {
            assert!(l.starts_with("[UM-ONE] horloge  120.0 BPM"), "{l}");
        }
    }
}
