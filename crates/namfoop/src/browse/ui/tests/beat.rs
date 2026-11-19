//! **La bande de battue** (§ 11 de `docs/design/set.md`) aux deux tailles d'écran du projet : le
//! minimum **80×25** et l'écran du Pi en police 10×20, **160×50**. Ce qui compte ici n'est pas le
//! pixel mais ce qu'on doit pouvoir lire en jouant : les temps de la mesure, le tempo, où l'on en
//! est, et **quand changer de son**.
//!
//! Les couleurs ne sont pas vérifiées — `screen_text` ne rend que les symboles. Ce sont les
//! **positions** qui portent le sens d'une bande : un temps 1 au bon endroit, sept croches
//! visibles en 7/8 même sur 80 colonnes.

use super::super::super::state::set::prompt::Loaded;
use super::super::super::state::set::ActiveSet;
use super::super::super::state::tests::with_chain;
use super::super::super::state::State;
use super::super::{render, Status};
use crate::tui::Key;
use dsp::preset::PresetSpec;
use library::prompt::Prompt;
use library::set::{SetSpec, Song, Sound};
use std::path::PathBuf;

/// Les deux tailles du projet : le minimum, et l'écran du Pi.
const SIZES: [(u16, u16); 2] = [(80, 25), (160, 50)];

fn shot(s: &State, w: u16, h: u16) -> String {
    crate::tui::screen_text(w, h, |f| render(s, &Status::default(), f))
}

/// Un set d'un morceau à deux sons, prompteur donné, jouant son son 1 dans l'espace Set.
fn staged(cho: &str, time: Option<&str>, bpm: f32) -> State {
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
                tempo_bpm: Some(bpm),
                time: time.map(str::to_string),
                prompt: Some(PathBuf::from("prompts/le-phare.cho")),
                sounds: vec![sound("clean-svt"), sound("fuzz-face")],
                ..Default::default()
            },
            Song { title: Some("Néon".into()), sounds: vec![sound("clean-svt")], ..Default::default() },
        ],
    };
    let find = |q: &str| s.presets.iter().position(|p| p.name == q);
    s.set = Some(ActiveSet::new("s1".into(), "groupe-1".into(), set, &find));
    s.adopt_prompts(vec![(PathBuf::from("/x/sets/prompts/le-phare.cho"), Loaded::Ok(Prompt::parse(cho)))]);
    s.mark_start();
    s.on_key(Key::Char('3'));
    s.play_set_sound((0, 0));
    s
}

const PHARE: &str = "{title: Le Phare}\n\
    {start_of_verse: Couplet 1}\n{x_namfoop_sound: clean-svt}\n{x_namfoop_bars: 8}\n\
    le phare tourne encore\net la mer se tait\n{end_of_verse}\n\
    {start_of_chorus: Refrain}\n{x_namfoop_sound: fuzz-face}\n{x_namfoop_bars: 4}\ntout casse\n{end_of_chorus}\n";

/// La **bande en 4/4**, aux deux tailles : les quatre temps, le tempo, la mesure, où l'on en est,
/// et `⏸ Espace` tant qu'elle ne tourne pas.
#[test]
fn the_band_shows_four_beats_the_tempo_and_where_we_are() {
    for (w, h) in SIZES {
        let s = staged(PHARE, None, 124.0);
        let out = shot(&s, w, h);
        let band = line_with(&out, "♩124");
        assert!(band.contains("♩124 4/4"), "{w}×{h} : le tempo et la mesure\n{out}");
        assert!(band.contains("mes. 1/8 COUPLET 1"), "{w}×{h} : la mesure et la section\n{band}");
        assert!(band.contains('⏸'), "{w}×{h} : à l'arrêt, la bande le dit\n{band}");
        // la place de dire la touche, elle, ne vient que si l'écran l'offre : sur 80 colonnes la
        // barre du bas la donne déjà (`Espace battue`), la bande garde ses temps
        if w >= 160 {
            assert!(band.contains("⏸ Espace"), "{w}×{h} : et nomme la touche\n{band}");
        }
        // les quatre temps, dans l'ordre, avant le tempo
        let cells = &band[..band.find("♩124").expect("le tempo")];
        let digits: String = cells.chars().filter(|c| c.is_ascii_digit()).collect();
        assert_eq!(digits, "1234", "{w}×{h} : les quatre temps\n{band}");
        assert!(!cells.contains('·'), "{w}×{h} : aucune subdivision en 4/4\n{band}");
        // la 2e ligne annonce la section suivante et le changement de son
        let next = line_with(&out, "REFRAIN");
        assert!(next.contains("REFRAIN dans 8 mes."), "{w}×{h} :\n{next}");
        assert!(next.contains("→ fuzz-face dans 8 mes."), "{w}×{h} : le son qui vient\n{next}");
    }
}

/// **6/8** : deux pulsations et quatre subdivisions en points — c'est ce qui distingue une mesure
/// composée d'une mesure à six temps.
#[test]
fn the_band_groups_six_eight_by_three() {
    for (w, h) in SIZES {
        let s = staged("{sov}\n{x_namfoop_bars: 4}\na\n{eov}\n", Some("6/8"), 82.0);
        let out = shot(&s, w, h);
        let band = line_with(&out, "♩82");
        assert!(band.contains("♩82 6/8"), "{w}×{h} :\n{out}");
        let cells = &band[..band.find("♩82").expect("le tempo")];
        assert_eq!(
            cells.chars().filter(|c| c.is_ascii_digit()).collect::<String>(),
            "12",
            "{w}×{h} : deux pulsations\n{band}"
        );
        assert_eq!(
            cells.chars().filter(|c| *c == '·').count(),
            4,
            "{w}×{h} : quatre croches en subdivisions\n{band}"
        );
    }
}

/// **7/8 en 2+2+3** : trois temps inégaux, et le regroupement est écrit à côté de la mesure. Sur
/// 80 colonnes, les sept croches doivent **toutes** se voir : c'est la largeur des cases qui
/// cède, jamais la mesure.
#[test]
fn the_band_shows_all_seven_eighths_even_on_eighty_columns() {
    for (w, h) in SIZES {
        let s = staged(
            "{time: 7/8}\n{x_namfoop_beats: 2+2+3}\n{sov}\n{x_namfoop_bars: 4}\na\n{eov}\n",
            None,
            92.0,
        );
        let out = shot(&s, w, h);
        let band = line_with(&out, "♩92");
        assert!(band.contains("♩92 7/8 (2+2+3)"), "{w}×{h} : la mesure et son regroupement\n{out}");
        let cells = &band[..band.find("♩92").expect("le tempo")];
        assert_eq!(
            cells.chars().filter(|c| c.is_ascii_digit()).collect::<String>(),
            "123",
            "{w}×{h} : trois temps battus\n{band}"
        );
        assert_eq!(
            cells.chars().filter(|c| *c == '·').count(),
            4,
            "{w}×{h} : les quatre croches qui ne commencent pas un temps\n{band}"
        );
    }
}

/// **En marche**, la bande dit `⏵`, et sur la **dernière mesure** avant un changement de son elle
/// passe au décompte des temps : c'est l'avertissement du § 11, au moment où il sert.
#[test]
fn the_band_counts_the_beats_down_before_a_sound_change() {
    for (w, h) in SIZES {
        // une section d'une seule mesure : la première est donc déjà la dernière
        let cho = "{start_of_verse: Couplet 1}\n{x_namfoop_sound: clean-svt}\n{x_namfoop_bars: 1}\na\n\
            {end_of_verse}\n{start_of_chorus: Refrain}\n{x_namfoop_sound: fuzz-face}\n\
            {x_namfoop_bars: 4}\nb\n{end_of_chorus}\n";
        let mut s = staged(cho, None, 120.0);
        s.on_key(Key::Char(' '));
        let out = shot(&s, w, h);
        let band = line_with(&out, "♩120");
        assert!(band.contains('⏵') && !band.contains("Espace"), "{w}×{h} : en marche\n{band}");
        let down = line_with(&out, "FUZZ-FACE");
        assert!(down.contains("→ FUZZ-FACE"), "{w}×{h} : le son visé, en capitales\n{out}");
        for v in ["4", "3", "2", "1"] {
            assert!(down.contains(v), "{w}×{h} : le décompte {v}\n{down}");
        }
        assert!(down.contains("REFRAIN"), "{w}×{h} : et la section qui vient\n{down}");
    }
}

/// Un **changement de mesure ou de tempo** est annoncé avant d'arriver, et un **morceau sans
/// prompteur** a sa bande quand même (§ 11) : la battue y compte les mesures.
#[test]
fn the_band_announces_a_change_and_exists_without_a_prompt() {
    for (w, h) in SIZES {
        let cho = "{sov}\n{x_namfoop_bars: 4}\na\n{eov}\n\
            {time: 7/8}\n{x_namfoop_beats: 2+2+3}\n{tempo: 92}\n\
            {start_of_bridge: Pont}\n{x_namfoop_bars: 2}\nb\n{end_of_bridge}\n";
        let out = shot(&staged(cho, Some("4/4"), 124.0), w, h);
        let next = line_with(&out, "PONT");
        assert!(next.contains("passe en 7/8 (2+2+3)"), "{w}×{h} :\n{out}");
        assert!(next.contains("♩92"), "{w}×{h} : le tempo qui vient\n{next}");

        // le morceau 2 n'a pas de prompteur : sa bande existe et le dit
        let mut s = staged(cho, Some("4/4"), 124.0);
        s.play_set_sound((1, 0));
        let out = shot(&s, w, h);
        assert!(out.contains("mes. 1"), "{w}×{h} : les mesures sont comptées\n{out}");
        assert!(out.contains("pas de sections : la battue compte les mesures"), "{w}×{h} :\n{out}");
    }
}

/// La **colonne de droite** de la ligne qui contient ce texte, espaces multiples réduits.
///
/// Deux pièges d'un écran à deux colonnes, et le second coûte cher : chercher une ligne attrape
/// les **deux** colonnes mêlées, et la set list contient elle aussi le tempo du morceau
/// (`♩124`) — donc `find` sur la ligne entière mesurerait la mauvaise colonne. Les bordures
/// `│` découpent les colonnes : on garde le **dernier** segment qui contient le texte cherché.
fn line_with(screen: &str, needle: &str) -> String {
    let line = screen
        .lines()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("aucune ligne ne contient « {needle} » :\n{screen}"));
    let seg = line.rsplit('│').find(|s| s.contains(needle)).expect("la ligne porte le texte cherché");
    seg.split_whitespace().collect::<Vec<_>>().join(" ")
}
