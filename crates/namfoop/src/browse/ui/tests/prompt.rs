//! **Le prompteur en grille** (§ 4 de `docs/design/prompteur.md`) aux tailles d'écran du projet, sur
//! le fichier de référence : chaque grille est une section nommée, ses accords en couleur, la
//! mesure jouée allumée, le repère à droite — ou dessous quand l'écran est étroit —, et la set list
//! qui se replie quand la grille ne tiendrait pas à côté d'elle.

use super::super::super::state::set::prompt::Loaded;
use super::super::super::state::set::ActiveSet;
use super::super::super::state::tests::with_chain;
use super::super::super::state::State;
use super::super::{render, Status};
use crate::theme;
use crate::tui::Key;
use dsp::preset::PresetSpec;
use library::prompt::Prompt;
use library::set::{SetSpec, Song, Sound};
use ratatui::buffer::Buffer;
use std::path::PathBuf;

/// Le fichier de référence du § 2 de `docs/design/prompteur.md`.
const GRID_PHARE: &str = include_str!("../../../../../library/src/fixtures/le-phare-grille.cho");

/// Un set de deux morceaux, « Le Phare » en grille et joué sur son son 1, dans l'espace Set.
fn staged() -> State {
    let mut s = with_chain(vec![]);
    let preset = |name: &str| {
        let spec = PresetSpec { name: name.into(), tempo_bpm: 100.0, ..Default::default() };
        (name.to_string(), name.to_string(), format!("/x/{name}.preset.toml").into(), spec)
    };
    s.adopt_presets(vec![preset("clean-svt"), preset("fuzz-face")]);
    let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
    let set = SetSpec {
        name: "Groupe 1".into(),
        base: PathBuf::from("/x/sets"),
        songs: vec![
            Song {
                title: Some("Le Phare".into()),
                tempo_bpm: Some(124.0),
                prompt: Some(PathBuf::from("prompts/le-phare.cho")),
                sounds: vec![sound("clean-svt"), sound("fuzz-face")],
                ..Default::default()
            },
            Song { title: Some("Néon".into()), sounds: vec![sound("clean-svt")], ..Default::default() },
        ],
    };
    let find = |q: &str| s.presets.iter().position(|p| p.name == q);
    s.set = Some(ActiveSet::new("s1".into(), "groupe-1".into(), set, &find));
    s.adopt_prompts(vec![(
        PathBuf::from("/x/sets/prompts/le-phare.cho"),
        Loaded::Ok(Prompt::parse(GRID_PHARE)),
    )]);
    s.mark_start();
    s.on_key(Key::Char('3'));
    s.play_set_sound((0, 0));
    s
}

fn draw(s: &State, w: u16, h: u16) -> Buffer {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
    t.draw(|f| render(s, &Status::default(), f)).unwrap();
    t.backend().buffer().clone()
}

fn text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

/// **La mesure allumée** : le texte des cellules en inverse vert sur la ligne courante (`›`) de
/// la grille — la bande de battue et la set list ont aussi des cellules allumées, d'où le repère.
fn lit_bar(buf: &Buffer) -> Option<String> {
    (0..buf.area.height).find_map(|y| {
        let line = row(buf, y);
        if !(line.contains(" › ") && line.contains('|')) {
            return None;
        }
        let lit: String = (0..buf.area.width)
            .filter(|x| buf[(*x, y)].bg == theme::set())
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        Some(lit.trim().to_string())
    })
}

/// L'horloge comme la donne la boucle de `browse` : une image toutes les 30 ms, depuis `from`.
fn run_to(s: &mut State, from: u64, to_ms: u64) {
    let mut t = from;
    while t < to_ms {
        t = (t + 30).min(to_ms);
        s.tick(t);
    }
}

/// **Le fichier en grille s'ouvre** : chaque grille est une section qui porte son nom (et non
/// `GRID LABEL=…`), sa durée se compte dans la grille, la cue écrite avant elle est sous son titre,
/// la tonalité est dans le titre, et le pont en trois grilles n'a qu'un titre et dit son 2/4.
#[test]
fn a_grid_file_shows_named_sections_bars_and_cues() {
    let s = staged();
    for (w, h) in [(160, 50), (80, 25)] {
        let out = text(&draw(&s, w, h));
        assert!(!out.to_lowercase().contains("label="), "{w}×{h} : le nom, pas la directive\n{out}");
        assert!(out.contains("▸ INTRO") && out.contains("8 mes."), "{w}×{h}\n{out}");
        assert!(out.contains("Le Phare · Em ·"), "{w}×{h} : la tonalité dans le titre\n{out}");
        assert!(out.contains("basse seule les 4 premières"), "{w}×{h} : la cue de l'intro\n{out}");
        // les cases s'alignent sur la plus large de la section : `G . . .` est complétée
        assert!(out.contains("| Em . . . | Em . . . | G . . .  | D . . .  |"), "{w}×{h}\n{out}");
    }
    // tout le morceau tient en 160×50 : on y lit la suite
    let out = text(&draw(&s, 160, 50));
    assert!(out.contains("COUPLET 1") && out.contains("16 mes."), "{out}");
    assert_eq!(out.matches("PONT").count(), 1, "trois grilles voisines, un seul titre\n{out}");
    assert!(out.contains("2/4   | G ."), "la grille courte dit sa mesure\n{out}");
    assert!(out.contains("regarde le batteur"), "la cue du pont\n{out}");
}

/// **Battue en marche, la mesure jouée est allumée** et les mesures déjà jouées de la ligne sont
/// grisées ; à l'arrêt, rien ne s'allume.
#[test]
fn the_bar_being_played_is_lit() {
    for (w, h) in [(160, 50), (80, 25)] {
        let mut s = staged();
        assert_eq!(lit_bar(&draw(&s, w, h)).as_deref(), Some(""), "{w}×{h} : à l'arrêt, rien");
        // l'horloge de l'état part de zéro : l'appui sur `Espace` est le 1, à 0 ms
        s.tick(0);
        s.on_key(Key::Char(' '));
        let start = 0;
        // ♩124 en 4/4 : 1 935 ms la mesure ; 6 mesures et demie plus tard, la 7ᵉ de l'intro —
        // la 3ᵉ de sa deuxième ligne, sur G
        run_to(&mut s, start, start + 6 * 1935 + 900);
        let set = s.set.as_ref().unwrap();
        assert_eq!((set.prompt_section, set.beat.bar), (0, 6), "{w}×{h}");
        let buf = draw(&s, w, h);
        assert_eq!(lit_bar(&buf).as_deref(), Some("G . . ."), "{w}×{h}\n{}", text(&buf));
        // les deux premières mesures de la ligne sont déjà jouées : en gris
        let y = (0..h).find(|y| row(&buf, *y).contains(" › ")).unwrap();
        let line = row(&buf, y);
        let x = line.chars().position(|c| c == 'E').unwrap() as u16;
        assert_eq!(buf[(x, y)].fg, theme::dim(), "{w}×{h} : la mesure jouée est grisée\n{line}");
    }
}

/// **Le repère** (les paroles écrites après la dernière barre) suit la ligne à droite, tronqué
/// s'il dépasse ; quand il reste moins de vingt colonnes, il passe dessous.
#[test]
fn the_lyrics_cue_goes_right_then_below() {
    let mut s = staged();
    s.on_key(Key::End); // le couplet 1
    let out = text(&draw(&s, 160, 50));
    let line = out.lines().find(|l| l.contains("La lampe tourne")).expect("le repère").to_string();
    assert!(line.contains("| D . . .  |  La lampe tourne et rien ne bouge"), "à droite\n{out}");
    // en 80×25, la set list se replie pour laisser la grille entière, et le repère reste à droite,
    // tronqué
    let out = text(&draw(&s, 80, 25));
    assert!(!out.contains("♪ Groupe 1"), "la set list est repliée en une ligne\n{out}");
    let line = out.lines().find(|l| l.contains("La lampe")).expect("le repère").to_string();
    assert!(line.contains("| D . . .  |  La lampe") && line.contains('…'), "{out}");
    // en 64 colonnes, il passe dessous
    let out = text(&draw(&s, 64, 25));
    let line = out.lines().find(|l| l.contains("La lampe")).expect("le repère").to_string();
    assert!(!line.contains('|') || line.trim_start().starts_with('│'), "dessous : {line}\n{out}");
    assert!(!line.contains("D . . ."), "dessous : {line}\n{out}");
}
