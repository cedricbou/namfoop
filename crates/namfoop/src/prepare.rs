//! **La préparation** d'une session qui joue (`browse`, `run`, `run --set`, `import --review`),
//! § 4 et § 5 de `docs/design/instruments.md` : si l'instrument joué n'a jamais été calibré sur
//! la carte branchée, namfoop le dit **avant d'ouvrir la carte** et attend une réponse —
//! `Entrée` calibre (la mesure de `calibrate`, puis la session s'ouvre), `Échap` joue sans et le
//! retient dans le fichier de l'instrument. **Au pied**, le geste de cet instrument calibre et le
//! CC du son joue sans : le port MIDI, déjà ouvert, est lu pendant la question. Rien ne sort tant qu'on n'a pas répondu, et jouer une
//! note ne répond pas : la carte n'est pas encore ouverte.
//!
//! La décision est pure (`decide`) ; l'écran et la mesure ne servent qu'au cas `Ask`. Hors
//! carte (`offline`), aucune question : la calibration de l'instrument se résout seule.

use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use engine::Routing;
use library::instrument as lib_instrument;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::amp::LevelArg;
use crate::config::LoadedConfig;
use crate::instrument::{self, Calibration, State};
use crate::theme;
use crate::tui::{self, Key};

/// Ce que fait la préparation.
#[derive(Debug, PartialEq)]
pub enum Decision {
    /// Calibrée, « jouée sans » retenue, carte simulée, hors carte, ou sans instrument : la
    /// session s'ouvre, aucune question.
    Play,
    /// Jamais calibrée sur cette carte, et quelqu'un au clavier : la question.
    Ask,
    /// Jamais calibrée, et personne pour répondre (service de scène du Pi) : jouer sans, le
    /// dire, ne rien retenir — la question reviendra au prochain lancement au clavier.
    Alone,
}

/// La décision, en fonction de l'état de la calibration et de la présence d'un clavier. La
/// carte simulée n'a jamais l'état `Uncalibrated` (elle emprunte une mesure ou n'en a aucune) :
/// elle ne pose donc jamais la question.
pub fn decide(cal: &Calibration, keyboard: bool) -> Decision {
    match (&cal.state, &cal.card) {
        (State::Uncalibrated, Some(_)) if keyboard => Decision::Ask,
        (State::Uncalibrated, Some(_)) => Decision::Alone,
        _ => Decision::Play,
    }
}

/// De quoi calibrer : la carte, le routage (l'entrée de la basse, les sorties où l'on s'entend),
/// la bibliothèque et l'instrument demandé.
pub struct Prep<'a> {
    pub audio: &'a audio_io::Settings,
    /// La carte jouée, celle des calibrations.
    pub card: instrument::Card,
    pub routing: &'a Routing,
    pub library: &'a Path,
    pub query: Option<&'a str>,
    /// Le pédalier, qui répond aussi à la question (§ 5) ; `None` : le clavier seul.
    pub midi: Option<&'a std::cell::RefCell<crate::cli_midi::MidiIn>>,
}

/// Sur la carte : résout la calibration jouée, pose la question s'il le faut, puis écrit la
/// ligne de démarrage avec le niveau retenu (`level` : `--level` de la commande).
pub fn on_card(conf: &LoadedConfig, prep: &Prep, level: Option<LevelArg>) -> Result<Calibration> {
    let card = &prep.card;
    let calib =
        || instrument::resolve(conf, &instrument::load(prep.library)?, prep.query, card, prep.library);
    let c = run(calib()?, prep, calib)?;
    if let Some(line) = c.startup_line(level.map(Into::into).unwrap_or_else(|| c.level(conf.config.level))) {
        eprintln!("{line}");
    }
    Ok(c)
}

/// Hors carte (`render`, `models`, `meta`, `import`) : aucune question, la ligne va au journal.
pub fn offline(
    conf: &LoadedConfig,
    library: &Path,
    query: Option<&str>,
    level: Option<LevelArg>,
) -> Result<Calibration> {
    let list = instrument::load(library)?;
    let c = instrument::resolve(conf, &list, query, &instrument::Card::Offline, library)?;
    if let Some(line) = c.startup_line(level.map(Into::into).unwrap_or_else(|| c.level(conf.config.level))) {
        log::info!("{line}");
    }
    Ok(c)
}

/// **Recalibrer** l'instrument demandé sur la carte (le même geste fait deux fois, § 8) : la
/// mesure de `calibrate`, puis le temps de lire le trim.
pub fn recalibrate(prep: &Prep) -> Result<()> {
    let card = match &prep.card {
        instrument::Card::Real(id) => id.clone(),
        instrument::Card::Sim => "sim".into(),
        instrument::Card::Offline => anyhow::bail!("aucune carte pour calibrer"),
    };
    measure(prep, &card)?;
    std::thread::sleep(Duration::from_secs(2));
    Ok(())
}

/// La réponse à la question.
#[derive(Debug, PartialEq)]
enum Answer {
    Calibrate,
    Skip,
    Quit,
}

fn answer_of(k: Key) -> Option<Answer> {
    match k {
        Key::Enter => Some(Answer::Calibrate),
        Key::Esc => Some(Answer::Skip),
        Key::Char('q') => Some(Answer::Quit),
        _ => None,
    }
}

/// Prépare la session : rend la calibration à jouer, mesurée si l'on a choisi de calibrer.
/// `again` relit la calibration après une mesure.
pub fn run(cal: Calibration, prep: &Prep, again: impl Fn() -> Result<Calibration>) -> Result<Calibration> {
    let keyboard = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let (Some(who), Some(card)) = (cal.instrument.clone(), cal.card.clone()) else {
        return Ok(cal);
    };
    match decide(&cal, keyboard) {
        Decision::Play => Ok(cal),
        Decision::Alone => {
            eprintln!("instrument  personne pour répondre : {who} joue sans calibration sur {card}");
            Ok(cal)
        }
        Decision::Ask => {
            let mut refused: Option<String> = None;
            let number = instrument_of(prep).ok().map(|i| i.number);
            loop {
                match question(&who, &card, number, prep.midi, &mut refused)? {
                    Answer::Quit => {
                        anyhow::bail!("lancement abandonné : {who} n'est pas calibrée sur {card}")
                    }
                    Answer::Skip => return Ok(skip(cal, prep, &card)),
                    Answer::Calibrate => match measure(prep, &card) {
                        Ok(()) => {
                            // le temps de lire le trim avant que l'écran ne s'ouvre
                            std::thread::sleep(Duration::from_secs(2));
                            return again();
                        }
                        // une mesure refusée (saturation, signal trop faible) dit quoi corriger,
                        // et la question revient
                        Err(e) => refused = Some(format!("Mesure refusée : {e:#}")),
                    },
                }
            }
        }
    }
}

/// L'instrument joué, tel qu'il est sur le disque.
fn instrument_of(prep: &Prep) -> Result<lib_instrument::Instrument> {
    let list = instrument::load(prep.library)?;
    let found = instrument::to_calibrate(&list, prep.query)?;
    found.map(|(i, _)| i).context("aucun instrument à calibrer")
}

fn measure(prep: &Prep, card: &str) -> Result<()> {
    let dest = crate::calibrate::Dest::Instrument {
        library: prep.library.to_path_buf(),
        instrument: instrument_of(prep)?,
        new: false,
        card: card.into(),
    };
    crate::calibrate::run(
        prep.audio,
        dest,
        crate::calibrate::DEFAULT_SECONDS,
        crate::calibrate::DEFAULT_TARGET_DBFS,
        prep.routing,
    )
}

/// « Jouer sans » : retenu dans le fichier de l'instrument. Une bibliothèque protégée en
/// écriture (le Pi de scène) n'empêche pas de jouer : la réponse vaut pour la session.
fn skip(cal: Calibration, prep: &Prep, card: &str) -> Calibration {
    let saved = instrument_of(prep).and_then(|mut i| {
        i.skip(card);
        lib_instrument::save(prep.library, &i).map_err(|e| anyhow::anyhow!("{e:#}"))
    });
    if let Err(e) = saved {
        log::debug!("« jouée sans » non enregistrée : {e:#}");
        eprintln!("bibliothèque protégée en écriture : retenu pour cette session seulement");
    }
    Calibration { state: State::Skipped, ..cal }
}

/// La question, en plein écran, jusqu'à une réponse au clavier ou au pied. `note` : la ligne
/// d'avertissement (mesure refusée, geste d'un autre instrument).
fn question(
    who: &str,
    card: &str,
    number: Option<u32>,
    midi: Option<&std::cell::RefCell<crate::cli_midi::MidiIn>>,
    note: &mut Option<String>,
) -> Result<Answer> {
    let mut session = tui::Session::start()?;
    loop {
        session.terminal.draw(|f| screen(f, who, card, note.as_deref()))?;
        if let Some(a) = tui::poll_key(Duration::from_millis(50))?.and_then(answer_of) {
            return Ok(a);
        }
        let Some(m) = midi else { continue };
        let mut m = m.borrow_mut();
        while let Some(msg) = m.try_pop() {
            let n = m.listen().numbers(msg.channel);
            match foot_answer(&msg.event, &n, number) {
                Some(Ok(a)) => return Ok(a),
                Some(Err(other)) => {
                    *note = Some(format!("Réponds d'abord pour {who} (geste de l'instrument {other})"))
                }
                None => {}
            }
        }
    }
}

/// Ce que vaut un message pendant la question : le geste de **cet** instrument calibre, le CC du
/// son joue sans, le geste d'un autre instrument ne répond pas (son numéro, pour le dire). Une
/// note jouée n'arrive pas ici : seul un geste répond.
fn foot_answer(
    event: &midi_io::Event,
    n: &crate::cli_midi::Numbers,
    number: Option<u32>,
) -> Option<std::result::Result<Answer, u32>> {
    let midi_io::Event::Control { cc, value } = *event else { return None };
    if cc == n.cc_instrument && !n.muted.instrument {
        let asked = value as u32 + 1;
        return Some(if Some(asked) == number { Ok(Answer::Calibrate) } else { Err(asked) });
    }
    (cc == n.cc_sound && !n.muted.sound).then_some(Ok(Answer::Skip))
}

/// L'écran de la question (§ 5), en couleurs neutres : l'ambre dit ce qui demande attention.
fn screen(f: &mut Frame, who: &str, card: &str, refused: Option<&str>) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let key = |k: &str| {
        Span::styled(format!("   {k:<10}"), Style::new().fg(theme::key()).add_modifier(Modifier::BOLD))
    };
    let mut lines = vec![
        Line::from(Span::styled(" namfoop · avant de jouer", bold)),
        Line::default(),
        Line::from(vec![
            Span::raw("   "),
            Span::styled(who.to_string(), bold),
            Span::styled(format!(" n'est pas calibrée sur {card}."), Style::new().fg(theme::warn())),
        ]),
        Line::default(),
    ];
    if let Some(why) = refused {
        lines.push(Line::from(Span::styled(format!("   {why}"), Style::new().fg(theme::error()))));
        lines.push(Line::default());
    }
    let secs = crate::calibrate::DEFAULT_SECONDS;
    lines.extend([
        Line::from(vec![
            key("Entrée"),
            Span::raw(format!(
                "calibrer maintenant : je joue fort {secs:.0} s (au pied : le geste de cet instrument)"
            )),
        ]),
        Line::from(vec![
            key("Échap"),
            Span::raw("jouer sans calibrer, la question ne reviendra pas (au pied : un bouton de son)"),
        ]),
        Line::from(vec![key("q"), Span::styled("ne pas jouer", Style::new().fg(theme::dim()))]),
        Line::default(),
        Line::from(Span::styled(
            "   Rien ne sort tant que je n'ai pas répondu.",
            Style::new().fg(theme::dim()),
        )),
    ]);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), f.area());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal(state: State, card: Option<&str>) -> Calibration {
        Calibration {
            instrument: Some("2 Fretless".into()),
            card: card.map(Into::into),
            state,
            ..Default::default()
        }
    }

    /// Tous les cas du tableau : seule une calibration jamais faite sur la carte branchée pose
    /// la question, et seulement s'il y a quelqu'un pour répondre.
    #[test]
    fn only_a_never_calibrated_instrument_on_a_real_card_asks() {
        let itwo = Some("itwo");
        for keyboard in [true, false] {
            assert_eq!(decide(&cal(State::Calibrated, itwo), keyboard), Decision::Play);
            assert_eq!(decide(&cal(State::Skipped, itwo), keyboard), Decision::Play, "réponse retenue");
            assert_eq!(decide(&cal(State::Config, None), keyboard), Decision::Play, "sans instrument");
            // la carte simulée et le hors carte n'ont jamais l'état « jamais calibrée »
            assert_eq!(decide(&cal(State::Borrowed { card: "itwo".into() }, None), keyboard), Decision::Play);
            assert_eq!(decide(&cal(State::Never, None), keyboard), Decision::Play, "carte simulée");
        }
        assert_eq!(decide(&cal(State::Uncalibrated, itwo), true), Decision::Ask);
        assert_eq!(decide(&cal(State::Uncalibrated, itwo), false), Decision::Alone, "le Pi démarre seul");
    }

    /// Une note jouée ne répond pas (la carte n'est pas ouverte) ; au clavier, seuls `Entrée`,
    /// `Échap` et `q` répondent.
    #[test]
    fn only_a_gesture_answers() {
        assert_eq!(answer_of(Key::Enter), Some(Answer::Calibrate));
        assert_eq!(answer_of(Key::Esc), Some(Answer::Skip));
        assert_eq!(answer_of(Key::Char('q')), Some(Answer::Quit));
        assert_eq!(answer_of(Key::Char(' ')), None);
        assert_eq!(answer_of(Key::Down), None);
    }

    /// **Au pied** : le geste de cet instrument calibre, le CC du son joue sans, le geste d'un
    /// autre instrument ne répond pas, et une note ou un autre CC ne disent rien.
    #[test]
    fn the_foot_answers_with_a_gesture() {
        use midi_io::Event;
        let n = crate::cli_midi::Numbers::default();
        let cc = |cc, value| Event::Control { cc, value };
        assert_eq!(foot_answer(&cc(102, 1), &n, Some(2)), Some(Ok(Answer::Calibrate)));
        assert_eq!(foot_answer(&cc(102, 0), &n, Some(2)), Some(Err(1)), "un autre instrument");
        assert_eq!(foot_answer(&cc(34, 0), &n, Some(2)), Some(Ok(Answer::Skip)));
        assert_eq!(foot_answer(&cc(104, 127), &n, Some(2)), None);
        assert_eq!(foot_answer(&Event::ProgramChange(1), &n, Some(2)), None);
    }

    #[test]
    fn the_question_names_the_instrument_the_card_and_both_answers() {
        let text = tui::screen_text(80, 25, |f| screen(f, "2 Fretless", "itwo", None));
        assert!(text.contains("2 Fretless n'est pas calibrée sur itwo."), "{text}");
        assert!(text.contains("Entrée") && text.contains("Échap"), "{text}");
        assert!(text.contains("Rien ne sort"), "{text}");
        let text = tui::screen_text(80, 25, |f| {
            screen(f, "2 Fretless", "itwo", Some("Mesure refusée : signal trop faible"))
        });
        assert!(text.contains("Mesure refusée : signal trop faible"), "{text}");
    }
}
