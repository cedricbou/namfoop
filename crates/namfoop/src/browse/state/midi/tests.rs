use super::super::chain::{Block, Leaf};
use super::super::tests::{fx_block, with_chain};
use super::super::*;
use crate::tui::Key;
use engine::tempo::TempoSource;
use midi_io::Event;

fn nam(m: usize) -> Leaf {
    Leaf::Nam { model: Some(m), on: true }
}

/// Une banque de deux presets, et une chaîne enregistrée (rien à perdre en basculant).
fn bank() -> State {
    let mut s = with_chain(vec![Block::Leaf(nam(0))]);
    s.mark_start();
    let preset =
        |name: &str, m: usize| with_chain(vec![Block::Leaf(nam(m)), fx_block("reverb")]).to_preset(name);
    s.adopt_presets(vec![
        ("p1".into(), "Chill verse".into(), "/x/a.preset.toml".into(), preset("Chill verse", 1)),
        ("p2".into(), "Punk chorus".into(), "/x/b.preset.toml".into(), preset("Punk chorus", 2)),
    ]);
    s
}

/// **PC n installe la place n de la banque**, celle que `P` montre, par le chemin du pied sur
/// `PgDn` : la chaîne est jouée et le preset devient la référence.
#[test]
fn a_program_change_installs_the_bank_slot() {
    let mut s = bank();
    assert_eq!(s.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Chill verse", "Punk chorus"]);
    assert!(matches!(s.on_midi(Event::ProgramChange(2), 0, 0), Action::Play { .. }));
    assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Punk chorus".into()));
    assert!(s.message.as_deref().is_some_and(|m| m.contains("Punk chorus")), "{:?}", s.message);
    assert!(s.unsaved().is_empty(), "le preset chargé est la référence");
    // et le premier aussi : la place 1 est bien la première, pas la seconde
    assert!(matches!(s.on_midi(Event::ProgramChange(1), 0, 0), Action::Play { .. }));
    assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Chill verse".into()));
}

/// Une chaîne non enregistrée : la bascule **demande une fois**, exactement comme `PgDn`. Un
/// pédalier ne doit pas faire perdre un réglage en cours sans le dire.
#[test]
fn a_program_change_asks_once_when_the_chain_is_not_saved() {
    let mut s = bank();
    s.on_key(Key::Char('x')); // la chaîne change
    s.on_midi(Event::ProgramChange(2), 0, 0);
    assert!(s.confirm_switch && s.pending_switch.is_some(), "la fenêtre s'ouvre");
    assert!(matches!(s.on_key(Key::Enter), Action::Play { .. }), "Entrée charge quand même");
    assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Punk chorus".into()));
}

/// Une place qui n'existe pas **ne change pas le son** et le dit (ADR 0011) : jouer
/// silencieusement un autre preset serait pire.
#[test]
fn a_program_change_out_of_the_bank_is_ignored_and_said() {
    let mut s = bank();
    for n in [3, 128] {
        assert_eq!(s.on_midi(Event::ProgramChange(n), 0, 0), Action::None);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("2 place")), "{:?}", s.message);
        assert!(s.preset.is_none(), "aucun preset n'a été installé");
    }
    let mut vide = with_chain(vec![Block::Leaf(nam(0))]);
    assert_eq!(vide.on_midi(Event::ProgramChange(1), 0, 0), Action::None);
    assert!(vide.message.as_deref().is_some_and(|m| m.contains("aucun preset")), "{:?}", vide.message);
}

/// Un preset illisible : le message le nomme, et le son ne change pas.
#[test]
fn a_broken_preset_called_by_midi_keeps_the_sound() {
    let mut s = bank();
    s.presets[0].chain = None;
    s.presets[0].broken = Some("modèle introuvable".into());
    assert_eq!(s.on_midi(Event::ProgramChange(1), 0, 0), Action::None);
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("Chill verse") && m.contains("modèle introuvable"), "{m}");
    assert!(s.preset.is_none(), "rien n'a été installé");
}

/// Pendant une **saisie de texte**, on est au clavier et pas au pied : le PC est ignoré, et il
/// ne change pas la chaîne sous un nom qu'on tape.
#[test]
fn a_program_change_waits_for_the_end_of_a_text_entry() {
    let mut s = bank();
    s.on_key(Key::Char('S')); // nommer le preset
    assert!(s.typing());
    assert_eq!(s.on_midi(Event::ProgramChange(2), 0, 0), Action::None);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("saisie")), "{:?}", s.message);
    assert!(s.preset.is_none());
}

/// Les CC que namfoop connaît mais qui n'ont rien à désigner **sans set** : ignorés, et dits.
/// Un Bank Select n'est jamais pris pour un son.
#[test]
fn the_known_ccs_say_why_they_do_nothing_without_a_set() {
    let mut s = bank();
    assert_eq!(s.on_midi(Event::Control { cc: 34, value: 2 }, 0, 0), Action::None);
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("CC 34") && m.contains("sans set"), "{m}");

    assert_eq!(s.on_midi(Event::Control { cc: super::CC_BANK_SELECT, value: 1 }, 0, 0), Action::None);
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("Bank Select") && m.contains("set"), "{m}");
    assert!(s.preset.is_none(), "aucun de ces CC n'installe quoi que ce soit");
}

/// Le CC du son est **réglable** (`[midi] cc_sound`) : c'est lui qu'on reconnaît, pas 34.
#[test]
fn the_sound_cc_number_is_configurable() {
    let mut s = bank();
    s.midi = super::MidiKeys { cc_sound: 69, ..Default::default() };
    s.on_midi(Event::Control { cc: 69, value: 2 }, 0, 0);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("CC 69")), "{:?}", s.message);
    s.message = None;
    s.on_midi(Event::Control { cc: 34, value: 2 }, 0, 0);
    assert_eq!(s.message, None, "34 ne veut plus rien dire : ignoré sans bruit");
}

/// Une horloge de `beats` noires à `bpm`, en partant de `from_us`. Rend l'instant du dernier
/// top. 24 tops par noire (constante du protocole) : deux noires suffisent à verrouiller.
fn clock(s: &mut State, bpm: f32, from_us: u64, beats: u32) -> u64 {
    let step = (60_000_000.0 / bpm / engine::tempo::TICKS_PER_BEAT as f32) as u64;
    let mut at = from_us;
    for k in 0..(beats * engine::tempo::TICKS_PER_BEAT) {
        at += step;
        s.on_midi(Event::ClockTick, at, k as u64 + 1);
    }
    at
}

/// **L'horloge d'Ableton donne le tempo entendu**, et le suit quand il change — sans que
/// l'arbitre republie pour un écart insignifiant (ADR 0008).
#[test]
fn an_ableton_clock_gives_the_heard_tempo() {
    let mut s = bank();
    let at = clock(&mut s, 120.0, 0, 3);
    assert_eq!(s.tempo_source(), TempoSource::MidiClock);
    assert!((s.heard_tempo_bpm() - 120.0).abs() < 0.5, "{} BPM", s.heard_tempo_bpm());
    // le morceau suivant est à 92 : le tempo entendu suit
    let at = clock(&mut s, 92.0, at + 1, 4);
    assert_eq!(s.tempo_source(), TempoSource::MidiClock);
    assert!((s.heard_tempo_bpm() - 92.0).abs() < 0.5, "{} BPM", s.heard_tempo_bpm());
    // le tempo ENREGISTRÉ du preset, lui, n'a pas bougé : une horloge n'est pas un réglage
    assert_eq!(s.fx.chain.tempo_bpm, dsp::tempo::DEFAULT_BPM);
    assert!(s.unsaved().is_empty(), "une horloge ne rend pas la chaîne « non enregistrée »");
    let _ = at;
}

/// Une horloge qui **se tait** garde son dernier tempo et le dit : jamais de retour silencieux
/// au tempo du preset, qui serait un changement de son causé par une panne.
#[test]
fn a_clock_that_stops_keeps_its_tempo_and_says_so() {
    let mut s = bank();
    let at = clock(&mut s, 120.0, 0, 3);
    let bpm = s.heard_tempo_bpm();
    // 600 ms sans top : plus de verrou (CLOCK_LOST_MS = 500)
    s.tick(at / 1000 + 600);
    assert_eq!(s.tempo_source(), TempoSource::ClockLost);
    assert_eq!(s.heard_tempo_bpm(), bpm, "le tempo est gardé");
    assert_eq!(TempoSource::ClockLost.label(), "horloge perdue");
}

/// `Start` recale le **temps 1** (le clic le suit) ; `Stop` garde le tempo.
#[test]
fn start_recalls_beat_one_and_stop_keeps_the_tempo() {
    let mut s = bank();
    let at = clock(&mut s, 120.0, 0, 3);
    let (epoch, bpm) = (s.beat_epoch(), s.heard_tempo_bpm());
    s.on_midi(Event::ClockStart, at + 1, 0);
    assert_eq!(s.beat_epoch(), epoch + 1, "Start recale le temps 1");
    assert_eq!(s.heard_tempo_bpm(), bpm);
    // `Continue` reprend sans recaler : c'est tout ce qui le distingue de `Start`
    let epoch = s.beat_epoch();
    s.on_midi(Event::ClockContinue, at + 2, 0);
    assert_eq!(s.beat_epoch(), epoch, "Continue ne recale pas");
    s.on_midi(Event::ClockStop, at + 3, 0);
    assert_eq!(s.heard_tempo_bpm(), bpm, "Stop garde le dernier tempo");
}

/// **Le tap du pédalier est la touche `*`** : même mesure, même message, et il **détrône**
/// l'horloge (ADR 0008) — on tape justement pour rattraper une désynchro du groupe.
#[test]
fn a_foot_tap_is_exactly_the_star_key() {
    let mut s = bank();
    let at = clock(&mut s, 92.0, 0, 3);
    assert_eq!(s.tempo_source(), TempoSource::MidiClock);
    let tap = |s: &mut State, at: u64| s.on_midi(Event::Control { cc: 14, value: 127 }, at, 0);
    tap(&mut s, at + 1);
    tap(&mut s, at + 1 + 400_000); // deux frappes à 400 ms = 150 BPM
    assert_eq!(s.tempo_source(), TempoSource::Tap, "le tap passe devant l'horloge");
    assert!((s.heard_tempo_bpm() - 150.0).abs() < 1.0, "{} BPM", s.heard_tempo_bpm());
    assert!(s.message.as_deref().is_some_and(|m| m.contains("(tap)")), "{:?}", s.message);
    // comme `*`, il modifie le preset : l'étoile « non enregistré » apparaît (ADR 0008)
    assert!(!s.unsaved().is_empty());
    // et l'horloge ne reprend la main que sur un événement net
    clock(&mut s, 92.0, at + 2_000_000, 3);
    assert_eq!(s.tempo_source(), TempoSource::Tap, "l'horloge suit sans écrire");
    s.on_midi(Event::ClockStart, at + 4_000_000, 0);
    clock(&mut s, 92.0, at + 4_000_001, 3);
    assert_eq!(s.tempo_source(), TempoSource::MidiClock, "Start lui rend la main");
}

/// `[midi] clock = false` : l'horloge ne dit plus rien — mais le **tap marche toujours**. Ce
/// n'est pas le MIDI qu'on coupe pour une jam, c'est l'horloge.
#[test]
fn the_clock_can_be_switched_off_without_losing_the_tap() {
    let mut s = bank();
    s.midi = super::MidiKeys { clock: false, ..Default::default() };
    let at = clock(&mut s, 120.0, 0, 3);
    assert_eq!(s.tempo_source(), TempoSource::Preset, "l'horloge est ignorée");
    assert_eq!(s.heard_tempo_bpm(), dsp::tempo::DEFAULT_BPM);
    s.on_midi(Event::ClockStart, at + 1, 0);
    s.on_midi(Event::ClockStop, at + 2, 0);
    assert_eq!(s.tempo_source(), TempoSource::Preset);
    let tap = |s: &mut State, at: u64| s.on_midi(Event::Control { cc: 14, value: 127 }, at, 0);
    tap(&mut s, at + 1_000_000);
    tap(&mut s, at + 1_500_000);
    assert_eq!(s.tempo_source(), TempoSource::Tap, "le tap, lui, répond");
}

/// Le CC du tap est **réglable** (`[midi] cc_tap`).
#[test]
fn the_tap_cc_number_is_configurable() {
    let mut s = bank();
    s.midi = super::MidiKeys { cc_tap: 64, ..Default::default() };
    s.on_midi(Event::Control { cc: 64, value: 127 }, 0, 0);
    s.on_midi(Event::Control { cc: 64, value: 127 }, 500_000, 0);
    assert_eq!(s.tempo_source(), TempoSource::Tap);
    s.midi = super::MidiKeys::default();
    s.message = None;
    s.on_midi(Event::Control { cc: 64, value: 127 }, 1_000_000, 0);
    assert_eq!(s.message, None, "64 ne veut plus rien dire : ignoré sans bruit");
}

/// Le morceau des tests du set : 2 mesures de 4/4 liées à `clean`, 1 de 4/4 liée à `fuzz`,
/// puis 1 de **7/8** — de quoi vérifier qu'une position tombe juste après un changement de
/// mesure. En doubles-croches : 32 pour la 1re section, 16 pour la 2e, 14 pour la 3e.
const CHO: &str = "{title: Le Phare}\n{tempo: 120}\n{time: 4/4}\n\
    {start_of_verse: Couplet}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 2}\na\nb\n{end_of_verse}\n\
    {start_of_chorus: Refrain}\n{x_namfoop_sound: fuzz}\n{x_namfoop_bars: 1}\nc\n{end_of_chorus}\n\
    {time: 7/8}\n{x_namfoop_beats: 2+2+3}\n\
    {start_of_bridge: Pont}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 1}\nd\n{end_of_bridge}\n";

/// Un set de concert ouvert, son premier son joué : « Le Phare » (PC 2, deux sons, un
/// prompteur) et « Marée basse » (PC 4, deux sons).
fn scene() -> State {
    use crate::browse::state::set::prompt::Loaded;
    use crate::browse::state::set::ActiveSet;
    use library::prompt::Prompt;
    use library::set::{SetSpec, Song, Sound};
    use std::path::PathBuf;

    let mut s = bank();
    let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
    let spec = SetSpec {
        name: "Concert".into(),
        base: PathBuf::from("/x/sets"),
        songs: vec![
            Song {
                title: Some("Le Phare".into()),
                pc: Some(2),
                tempo_bpm: Some(120.0),
                prompt: Some(PathBuf::from("prompts/le-phare.cho")),
                sounds: vec![sound("Chill verse"), sound("Punk chorus")],
                ..Default::default()
            },
            Song {
                title: Some("Marée basse".into()),
                pc: Some(4),
                sounds: vec![sound("Punk chorus"), sound("Chill verse")],
                ..Default::default()
            },
        ],
    };
    let names: Vec<String> = s.presets.iter().map(|p| p.name.clone()).collect();
    let find = |q: &str| names.iter().position(|n| n == q);
    s.set = Some(ActiveSet::new("s".into(), "concert".into(), spec, &find));
    s.adopt_prompts(vec![(PathBuf::from("/x/sets/prompts/le-phare.cho"), Loaded::Ok(Prompt::parse(CHO)))]);
    s.on_key(Key::Char('3'));
    s.play_set_sound((0, 0));
    s.mark_start();
    s.message = None;
    s
}

fn at(s: &State) -> Option<(usize, usize)> {
    s.set.as_ref().and_then(|x| x.at)
}

/// **PC n appelle le morceau dont le `pc` vaut n**, sur son premier son — et il attend la
/// fenêtre du CC avant de jouer, pour ne faire qu'un seul changement.
#[test]
fn a_program_change_calls_the_song_with_that_pc() {
    let mut s = scene();
    assert_eq!(s.on_midi(Event::ProgramChange(4), 0, 0), Action::None, "rien tout de suite");
    assert_eq!(at(&s), Some((0, 0)), "le son joué n'a pas encore bougé");
    // dans la fenêtre, le tour de boucle ne décide rien
    assert_eq!(s.tick(1), Action::None);
    assert_eq!(at(&s), Some((0, 0)));
    // la fenêtre passée, le morceau se joue sur son son 1
    assert!(matches!(s.tick(100), Action::Play { .. }));
    assert_eq!(at(&s), Some((1, 0)));
    assert!(s.message.as_deref().is_some_and(|m| m.contains("Marée basse")), "{:?}", s.message);
    // et une seule fois : la demande est consommée
    assert_eq!(s.tick(200), Action::None);
}

/// **Un PC suivi d'un CC ne fait qu'un seul changement de son** (§ 12) : Ableton envoie
/// souvent les deux en tête de clip, et deux changements s'entendraient.
#[test]
fn a_program_change_followed_by_a_cc_makes_one_single_change() {
    let mut s = scene();
    assert_eq!(s.on_midi(Event::ProgramChange(4), 0, 0), Action::None);
    // le CC arrive 2 ms après : un seul changement, directement sur le bon son
    assert!(matches!(s.on_midi(Event::Control { cc: 34, value: 1 }, 2_000, 0), Action::Play { .. }));
    assert_eq!(at(&s), Some((1, 1)), "le son 2 du morceau appelé");
    assert_eq!(s.tick(100), Action::None, "le PC ne joue pas une seconde fois");
    assert_eq!(at(&s), Some((1, 1)));
}

/// Un CC **hors** de la fenêtre parle du morceau joué, pas de celui du PC précédent.
#[test]
fn a_late_cc_speaks_of_the_song_being_played() {
    let mut s = scene();
    s.on_midi(Event::ProgramChange(4), 0, 0);
    assert!(matches!(s.tick(100), Action::Play { .. }));
    assert_eq!(at(&s), Some((1, 0)));
    assert!(matches!(s.on_midi(Event::Control { cc: 34, value: 1 }, 200_000, 0), Action::Play { .. }));
    assert_eq!(at(&s), Some((1, 1)));
}

/// **Un PC qu'aucun morceau ne porte est ignoré, et le dit** : jamais un autre morceau à la
/// place (ADR 0011).
#[test]
fn a_pc_no_song_carries_is_ignored_and_said() {
    let mut s = scene();
    assert_eq!(s.on_midi(Event::ProgramChange(3), 0, 0), Action::None);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("PC 3")), "{:?}", s.message);
    assert_eq!(s.tick(100), Action::None, "et rien ne se joue ensuite");
    assert_eq!(at(&s), Some((0, 0)));
}

/// **CC de valeur k : le son k+1 du morceau joué.** Une valeur au-delà est ignorée et le dit
/// — Fractal écrête, namfoop préfère le dire que de jouer un autre son.
#[test]
fn a_cc_calls_the_sound_of_the_song_played() {
    let mut s = scene();
    assert!(matches!(s.on_midi(Event::Control { cc: 34, value: 1 }, 0, 0), Action::Play { .. }));
    assert_eq!(at(&s), Some((0, 1)));

    assert_eq!(s.on_midi(Event::Control { cc: 34, value: 5 }, 1000, 0), Action::None);
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("Le Phare") && m.contains("2 sons"), "{m}");
    assert_eq!(at(&s), Some((0, 1)), "le son joué n'a pas changé");
}

/// `Start` remet la battue sur le **1 de la mesure 1** et la lance ; `Stop` l'arrête.
#[test]
fn start_puts_the_beat_on_bar_one_and_stop_stops_it() {
    let mut s = scene();
    // on part d'ailleurs dans le morceau
    s.on_midi(Event::SongPosition(36), 0, 0);
    assert_ne!(s.set.as_ref().map(|x| x.prompt_section), Some(0));

    s.on_midi(Event::ClockStart, 1000, 0);
    let set = s.set.as_ref().expect("set");
    assert_eq!((set.prompt_section, set.beat.bar, set.beat.tick), (0, 0, 0));
    assert!(set.beat.running, "le transport qui démarre lance la battue");

    s.on_midi(Event::ClockStop, 2000, 0);
    assert!(!s.set.as_ref().expect("set").beat.running, "Stop arrête la battue");
}

/// **Un saut de position (SPP) tombe juste**, y compris après un changement de mesure : la
/// carte des mesures convertit les doubles-croches.
#[test]
fn a_song_position_lands_in_the_right_bar() {
    let mut s = scene();
    // 40 doubles-croches : 32 pour la 1re section (2 mesures de 4/4), puis 8 dans la 2e,
    // soit sa mesure 1 sur le 3e temps
    s.on_midi(Event::SongPosition(40), 0, 0);
    let set = s.set.as_ref().expect("set");
    assert_eq!((set.prompt_section, set.beat.bar, set.beat.tick), (1, 0, 2));

    // 48 : juste au début de la section en 7/8
    s.on_midi(Event::SongPosition(48), 1000, 0);
    let set = s.set.as_ref().expect("set");
    assert_eq!((set.prompt_section, set.beat.bar, set.beat.tick), (2, 0, 0));

    // au-delà de ce que le morceau déclare : on le dit, et la battue ne bouge pas
    s.on_midi(Event::SongPosition(500), 2000, 0);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("au-delà")), "{:?}", s.message);
    let set = s.set.as_ref().expect("set");
    assert_eq!((set.prompt_section, set.beat.bar, set.beat.tick), (2, 0, 0));
}

/// **Sur scène verrouillée** (`run --set`), le MIDI répond comme ailleurs : c'est le clavier
/// qui est verrouillé, pas le pédalier.
#[test]
fn the_locked_stage_answers_midi_too() {
    let mut s = scene();
    s.locked = true;
    s.on_midi(Event::ProgramChange(4), 0, 0);
    assert!(matches!(s.tick(100), Action::Play { .. }));
    assert_eq!(at(&s), Some((1, 0)));
}

/// **CC 15 : l'accordeur** (`docs/design/accordeur.md` § 3 et § 8). 64 ou plus coupe, moins de
/// 64 rend le son, et ce n'est pas une bascule : chaque demande déjà satisfaite le dit.
#[test]
fn cc_15_cuts_and_gives_back_and_is_not_a_toggle() {
    let mut s = bank();
    s.on_midi(Event::Control { cc: 15, value: 127 }, 0, 0);
    assert!(s.bass_cut());
    assert_eq!(s.message.as_deref(), Some("CC 15 = 127 → basse coupée, accordeur"));
    s.on_midi(Event::Control { cc: 15, value: 127 }, 0, 0);
    assert!(s.bass_cut(), "pas une bascule");
    assert_eq!(s.message.as_deref(), Some("CC 15 = 127 : la basse est déjà coupée, rien ne change"));
    s.on_midi(Event::Control { cc: 15, value: 0 }, 0, 0);
    assert!(!s.bass_cut());
    assert_eq!(s.message.as_deref(), Some("CC 15 = 0 → son rendu"));
    s.on_midi(Event::Control { cc: 15, value: 0 }, 0, 0);
    assert_eq!(s.message.as_deref(), Some("CC 15 = 0 : la basse n'est pas coupée, rien ne change"));
}

/// **Jamais muet au début d'un titre** : un PC et un `Start` rendent le son ; `Continue` et
/// l'horloge, non.
#[test]
fn a_program_change_or_start_gives_the_sound_back() {
    let mut s = bank();
    s.on_midi(Event::Control { cc: 15, value: 127 }, 0, 0);
    s.on_midi(Event::ClockContinue, 0, 0);
    s.on_midi(Event::ClockTick, 0, 1);
    assert!(s.bass_cut(), "ni Continue ni l'horloge ne rendent le son");
    s.on_midi(Event::ProgramChange(2), 0, 0);
    assert!(!s.bass_cut(), "un PC rend le son");
    assert!(s.message.as_deref().unwrap_or("").ends_with("son rendu"), "{:?}", s.message);
    s.on_midi(Event::Control { cc: 15, value: 127 }, 0, 0);
    s.on_midi(Event::ClockStart, 0, 0);
    assert!(!s.bass_cut(), "Start rend le son");
    // l'horloge coupée par la config ne change rien à cette règle
    s.midi.clock = false;
    s.on_midi(Event::Control { cc: 15, value: 127 }, 0, 0);
    s.on_midi(Event::ClockStart, 0, 0);
    assert!(!s.bass_cut(), "Start rend le son même sans écouter l'horloge");
}

/// `[midi] cc_tuner` réglé à un autre numéro : le 15 n'agit plus.
#[test]
fn the_tuner_cc_is_settable() {
    let mut s = bank();
    s.midi.cc_tuner = 20;
    s.on_midi(Event::Control { cc: 15, value: 127 }, 0, 0);
    assert!(!s.bass_cut(), "le 15 n'est plus l'accordeur");
    s.on_midi(Event::Control { cc: 20, value: 127 }, 0, 0);
    assert!(s.bass_cut());
}

/// **Les numéros appartiennent au canal** (ADR 0012) : Ableton, canal 5, appelle les sons par
/// le CC 20 pendant que le pédalier, canal 1, garde le CC 34. Le dernier message est suivi,
/// d'où qu'il vienne : il n'y a pas de priorité.
#[test]
fn each_channel_calls_the_sounds_with_its_own_cc() {
    let mut s = scene();
    let n = |cc_sound| crate::cli_midi::Numbers { cc_sound, ..Default::default() };
    s.midi_channels = vec![(1, n(34)), (5, n(20))];
    let cc = |cc, value| Event::Control { cc, value };
    assert!(matches!(s.on_midi_from(cc(20, 1), Some(5), 0, 0), Action::Play { .. }), "le CC 20 du canal 5");
    assert_eq!(at(&s), Some((0, 1)));
    assert!(
        matches!(s.on_midi_from(cc(34, 0), Some(1), 1000, 0), Action::Play { .. }),
        "le CC 34 du canal 1"
    );
    assert_eq!(at(&s), Some((0, 0)), "le dernier message est suivi");
    assert_eq!(s.on_midi_from(cc(34, 1), Some(5), 2000, 0), Action::None, "le 34 n'est rien sur le canal 5");
    assert_eq!(at(&s), Some((0, 0)));
    assert!(matches!(s.on_midi_from(cc(20, 1), Some(5), 3000, 0), Action::Play { .. }));
    assert_eq!(at(&s), Some((0, 1)));
}

/// **Un geste coupé** (`b` dans `namfoop midi`) ne fait rien, et le dit ; les autres gestes du
/// canal, et le même geste sur un autre canal, marchent toujours.
#[test]
fn a_muted_gesture_does_nothing_and_says_so() {
    let mut s = scene();
    let muted = crate::cli_midi::Muted { sound: true, ..Default::default() };
    s.midi_channels = vec![
        (1, crate::cli_midi::Numbers { muted, ..Default::default() }),
        (5, crate::cli_midi::Numbers::default()),
    ];
    let cc = Event::Control { cc: 34, value: 1 };
    assert_eq!(s.on_midi_from(cc, Some(1), 0, 0), Action::None);
    assert!(s.message.clone().unwrap_or_default().contains("coupé"), "{:?}", s.message);
    assert!(
        matches!(s.on_midi_from(cc, Some(5), 1000, 0), Action::Play { .. }),
        "le canal 5 n'est pas coupé"
    );
}

/// **Morceau suivant / précédent au pied** (CC 104 / 103) : le chemin de `→` / `←` — le premier
/// son du morceau, sans boucler aux bornes du set. Un bouton qui envoie 127 puis 0 ne change
/// qu'une fois.
#[test]
fn the_song_ccs_step_through_the_set_like_the_arrows() {
    let mut s = scene();
    s.play_set_sound((0, 1));
    assert!(matches!(s.on_midi(Event::Control { cc: 104, value: 127 }, 0, 0), Action::Play { .. }));
    assert_eq!(at(&s), Some((1, 0)), "le morceau suivant, sur son premier son");
    assert_eq!(s.on_midi(Event::Control { cc: 104, value: 0 }, 1000, 0), Action::None, "le relâcher");
    assert_eq!(at(&s), Some((1, 0)), "une seule fois");
    assert_eq!(s.on_midi(Event::Control { cc: 104, value: 127 }, 2000, 0), Action::None);
    assert_eq!(s.message.as_deref(), Some("dernier morceau"), "pas de bouclage");
    assert!(matches!(s.on_midi(Event::Control { cc: 103, value: 127 }, 3000, 0), Action::Play { .. }));
    assert_eq!(at(&s), Some((0, 0)));
    assert_eq!(s.on_midi(Event::Control { cc: 103, value: 64 }, 4000, 0), Action::None);
    assert_eq!(s.message.as_deref(), Some("premier morceau"));
}

/// Pendant l'accordage, changer de morceau au pied **rend le son**, comme tout chargement.
#[test]
fn a_song_cc_gives_the_sound_back() {
    let mut s = scene();
    s.on_midi(Event::Control { cc: 15, value: 127 }, 0, 0);
    assert!(s.bass_cut());
    assert!(matches!(s.on_midi(Event::Control { cc: 104, value: 127 }, 1000, 0), Action::Play { .. }));
    assert!(!s.bass_cut(), "le son revient avec le morceau");
}

/// Sans set ouvert, les deux messages ne font rien, et l'écran le dit.
#[test]
fn the_song_ccs_without_a_set_say_so() {
    let mut s = bank();
    assert_eq!(s.on_midi(Event::Control { cc: 104, value: 127 }, 0, 0), Action::None);
    assert_eq!(s.message.as_deref(), Some("CC 104 : aucun set ouvert"));
}

/// Un PC en attente est oublié par un changement de morceau au pied : la dernière demande gagne.
#[test]
fn a_song_cc_wins_over_a_waiting_program_change() {
    let mut s = scene();
    s.on_midi(Event::ProgramChange(4), 0, 0);
    s.on_midi(Event::Control { cc: 103, value: 127 }, 1000, 0);
    assert_eq!(at(&s), Some((0, 0)), "déjà au premier morceau : rien ne bouge");
    assert_eq!(s.tick(100), Action::None, "le PC ne se joue plus");
    assert_eq!(at(&s), Some((0, 0)));
}

/// Les numéros se règlent, et le geste se coupe par canal.
#[test]
fn the_song_ccs_are_settable_and_mutable() {
    let mut s = scene();
    s.midi.cc_next_song = 110;
    assert_eq!(s.on_midi(Event::Control { cc: 104, value: 127 }, 0, 0), Action::None, "104 n'agit plus");
    assert!(matches!(s.on_midi(Event::Control { cc: 110, value: 127 }, 1000, 0), Action::Play { .. }));
    let muted = crate::cli_midi::Muted { prev_song: true, ..Default::default() };
    s.midi_channels = vec![(1, crate::cli_midi::Numbers { muted, ..Default::default() })];
    assert_eq!(s.on_midi_from(Event::Control { cc: 103, value: 127 }, Some(1), 2000, 0), Action::None);
    assert!(
        s.message.clone().unwrap_or_default().contains("le morceau précédent est coupé"),
        "{:?}",
        s.message
    );
}

/// **Un tap en plein set ne demande rien au morceau suivant** : c'est le tempo du soir, pas un
/// réglage du preset. Dans l'espace Chaîne, en revanche, il modifie le preset (test du tempo des
/// presets).
#[test]
fn a_tap_during_a_set_never_asks_to_save() {
    let mut s = scene();
    let t0 = 1_000_000;
    s.on_midi(Event::Control { cc: 14, value: 127 }, t0, 0);
    s.on_midi(Event::Control { cc: 14, value: 127 }, t0 + 577_000, 0);
    assert!((s.heard_tempo_bpm() - 104.0).abs() < 0.5, "le tap s'entend : {} BPM", s.heard_tempo_bpm());
    assert!(s.unsaved().is_empty(), "rien de non enregistré : {:?}", s.unsaved());
    assert!(matches!(
        s.on_midi(Event::Control { cc: 104, value: 127 }, t0 + 2_000_000, 0),
        Action::Play { .. }
    ));
    assert_eq!(at(&s), Some((1, 0)), "le morceau suivant, sans confirmation");
    assert_eq!(s.tempo_source(), TempoSource::Preset, "le tap est oublié avec le morceau");

    // dans l'espace Chaîne, le même tap modifie le preset
    s.on_key(Key::Char('2'));
    s.on_midi(Event::Control { cc: 14, value: 127 }, t0 + 3_000_000, 0);
    s.on_midi(Event::Control { cc: 14, value: 127 }, t0 + 3_577_000, 0);
    assert!(!s.unsaved().is_empty(), "le tap de l'espace Chaîne est un réglage");
}
