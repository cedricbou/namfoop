//! Les tests du prompteur : chaque directive lue, et le fichier de référence en grille.

use super::*;

const PHARE: &str = r#"
{title: Le Phare}
{duration: 3:45}
{tempo: 92}
{time: 4/4}
# un commentaire du fichier : ni cue, ni texte

{start_of_verse: Couplet 1}
{x_namfoop_sound: clean-svt}
{x_namfoop_bars: 8}
[Am]Le phare tourne [F]encore
Et la [C]mer se tait
{end_of_verse}

{soc}
{x_namfoop_sound: fuzz-face}
{comment: monte le son}
[G]Tout casse
{eoc}
"#;

/// Le fichier de référence du § 2 de `docs/design/prompteur.md`.
pub(crate) const GRID_PHARE: &str = include_str!("../fixtures/le-phare-grille.cho");

/// Chaque directive du § 10, et ce qu'elle produit.
#[test]
fn the_subset_of_chordpro_of_the_spec_is_read() {
    let p = Prompt::parse(PHARE);
    assert_eq!(p.title.as_deref(), Some("Le Phare"));
    assert_eq!(p.duration_s, Some(225.0), "3:45 en secondes");
    assert_eq!(p.tempo_bpm, Some(92.0));
    assert_eq!(p.time.as_deref(), Some("4/4"));
    assert_eq!(p.sections.len(), 2, "{:#?}", p.sections);

    let v = &p.sections[0];
    assert_eq!((v.kind.as_str(), v.label.as_str()), ("verse", "Couplet 1"));
    assert_eq!(v.sound.as_deref(), Some("clean-svt"));
    assert_eq!(v.bars, Some(8));
    // le tempo et la mesure **en cours** sont portés par la section (carte des mesures, US 6b)
    assert_eq!((v.tempo_bpm, v.time.as_deref()), (Some(92.0), Some("4/4")));
    assert_eq!(v.lines.len(), 2, "{:#?}", v.lines);
    assert_eq!(v.lines[0].text, "Le phare tourne encore");
    assert_eq!(
        v.lines[0].chords,
        vec![Chord { at: 0, name: "Am".into() }, Chord { at: 16, name: "F".into() },],
        "les accords sont retirés du texte, à leur place en caractères"
    );
    assert_eq!(&v.lines[0].text[16..16 + 6], "encore", "la place pointe bien le mot");

    // la forme courte `{soc}` sans étiquette : le genre est traduit
    let c = &p.sections[1];
    assert_eq!((c.kind.as_str(), c.label.as_str()), ("chorus", "Refrain"));
    assert_eq!(c.sound.as_deref(), Some("fuzz-face"), "la directive avant le texte compte");
    assert!(c.lines[0].cue, "{{comment}} est une cue, pas des paroles");
    assert_eq!(c.lines[0].text, "monte le son");
    assert!(!c.lines[1].cue);
}

/// Les formes courtes et les sections **libres** : `{start_of_pont-final: …}` doit marcher,
/// ChordPro le permet et un musicien nomme ses parties comme il veut.
#[test]
fn short_forms_and_free_sections() {
    let p = Prompt::parse(
        "{sov}\na\n{eov}\n{sob}\nb\n{eob}\n{start_of_intro}\nc\n{end_of_intro}\n\
         {start_of_pont final: Pont final}\nd\n{end_of_pont final}\n",
    );
    let labels: Vec<&str> = p.sections.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, vec!["Couplet", "Pont", "Intro", "Pont final"]);
    let kinds: Vec<&str> = p.sections.iter().map(|s| s.kind.as_str()).collect();
    assert_eq!(kinds, vec!["verse", "bridge", "intro", "pont final"]);
}

/// **Ce qu'on ne comprend pas est ignoré, sans erreur** : c'est la règle qui permet d'ouvrir
/// ici un fichier écrit pour un autre logiciel.
#[test]
fn unknown_directives_are_ignored_without_error() {
    let p = Prompt::parse(
        "{subtitle: Trio}\n{define: Am base-fret 1 frets 0 0 2 2 1 0}\n{x_autre: 3}\n\
         {chordfont: Mono}\n{start_of_verse}\nparoles\n{end_of_verse}\n",
    );
    assert_eq!(p.sections.len(), 1);
    assert_eq!(p.sections[0].lines[0].text, "paroles");
    assert_eq!(p.title, None, "aucune directive connue n'a été inventée");
}

/// Un fichier **sans sections** s'affiche quand même : son texte va dans une section sans nom.
#[test]
fn text_without_any_section_is_kept() {
    let p = Prompt::parse("[Am]juste des paroles\net une autre ligne\n");
    assert_eq!(p.sections.len(), 1);
    assert_eq!(p.sections[0].label, "");
    assert_eq!(p.sections[0].lines.len(), 2);
    assert_eq!(p.line_count(), 2);
}

/// Un fichier vide, ou qui n'a que des directives, ne casse rien et ne promet rien.
#[test]
fn an_empty_file_is_an_empty_prompt() {
    for text in ["", "\n\n\n", "# rien\n", "{title: Vide}\n"] {
        let p = Prompt::parse(text);
        assert_eq!(p.line_count(), 0, "{text:?}");
        assert!(p.sections.is_empty(), "{text:?} → {:#?}", p.sections);
    }
}

/// Les cas tordus des accords : un crochet jamais fermé reste du texte, un accord vide n'en
/// est pas un, et deux accords collés gardent leur place.
#[test]
fn broken_chords_stay_text() {
    let l = lyrics("[Am][C]collés et un [crochet ouvert");
    assert_eq!(l.text, "collés et un [crochet ouvert");
    assert_eq!(l.chords.len(), 2);
    assert_eq!(l.chords[1], Chord { at: 0, name: "C".into() });
    assert_eq!(lyrics("[]rien").chords, vec![], "un accord vide n'est pas un accord");
}

/// `{duration}` et `{x_namfoop_beats}` acceptent les deux écritures du § 10.
#[test]
fn duration_and_beat_groups() {
    assert_eq!(duration_s("3:45"), Some(225.0));
    assert_eq!(duration_s("210"), Some(210.0));
    assert_eq!(duration_s("n'importe quoi"), None);
    let p = Prompt::parse("{time: 7/8}\n{x_namfoop_beats: 2+2+3}\n{sov}\na\n{eov}\n");
    assert_eq!(p.sections[0].beats, Some(vec![2, 2, 3]));
    assert_eq!(p.sections[0].time.as_deref(), Some("7/8"));
    // une nouvelle mesure annule le regroupement de l'ancienne
    let p = Prompt::parse("{time: 7/8}\n{x_namfoop_beats: 2+2+3}\n{time: 4/4}\n{sov}\na\n{eov}\n");
    assert_eq!(p.sections[0].beats, None);
}

/// **Le suivi du son** (§ 10) : changer de son amène à la **prochaine** section liée à ce son,
/// et à la première si on est déjà passé après.
#[test]
fn a_sound_leads_to_its_next_section() {
    let p = Prompt::parse(
        "{sov}\n{x_namfoop_sound: clean}\na\n{eov}\n\
         {soc}\n{x_namfoop_sound: fuzz}\nb\n{eoc}\n\
         {start_of_verse: Couplet 2}\n{x_namfoop_sound: clean}\nc\n{end_of_verse}\n",
    );
    assert_eq!(p.section_of_sound("clean", 0), Some(0));
    assert_eq!(p.section_of_sound("clean", 1), Some(2), "la PROCHAINE");
    assert_eq!(p.section_of_sound("fuzz", 2), Some(1), "rien après : on revient au début");
    assert_eq!(p.section_of_sound("inconnu", 0), None);
}

/// **Le fichier de référence du prompteur en grille** (§ 2 de `docs/design/prompteur.md`) :
/// chaque grille est une section nommée, ses lignes ont leurs mesures et leur repère, la cue
/// écrite avant une grille est la sienne, et la tonalité est lue.
#[test]
fn the_reference_grid_file_is_read() {
    let p = Prompt::parse(GRID_PHARE);
    assert_eq!(p.key.as_deref(), Some("Em"));
    let labels: Vec<&str> = p.sections.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, vec!["Intro", "Couplet 1", "Refrain", "Pont", "Pont", "Pont", "Refrain", "Fin"]);
    let bars: Vec<Option<u32>> = p.sections.iter().map(Section::bars).collect();
    let want = [8, 16, 8, 3, 1, 4, 8, 2].map(Some);
    assert_eq!(bars, want, "la durée se compte dans la grille, sans x_namfoop_bars");
    assert!(p.sections.iter().all(|s| s.kind == "grid" && s.bars.is_none()));

    // la cue écrite avant la grille est la première ligne de **sa** section
    let intro = &p.sections[0];
    assert!(intro.lines[0].cue);
    assert_eq!(intro.lines[0].text, "basse seule les 4 premières");
    assert_eq!(intro.lines[1].bar_count(), 4);
    let pont = &p.sections[3];
    assert_eq!((pont.lines[0].cue, pont.lines[0].text.as_str()), (true, "regarde le batteur"));

    let couplet = &p.sections[1];
    let row = couplet.lines[0].grid.as_ref().expect("une ligne de mesures");
    assert_eq!(row.right, "La lampe tourne et rien ne bouge / Sur le quai les filets sèchent");
    assert_eq!(row.bars[0].text(), "Em . . .");

    // le pont court est en 2/4, entre deux {time}
    let times: Vec<Option<&str>> = p.sections[3..6].iter().map(|s| s.time.as_deref()).collect();
    assert_eq!(times, vec![Some("4/4"), Some("2/4"), Some("4/4")]);
}

/// Les écritures d'une étiquette : attributs sur toute section (ChordPro 6), après `:`, et la
/// forme ancienne de la grille, la forme d'abord.
#[test]
fn labels_in_attributes_and_the_old_grid_form() {
    let p = Prompt::parse(
        "{start_of_grid: 4x4 Couplet 1}\n| A . . . |\n{end_of_grid}\n\
         {start_of_verse label=\"Couplet 2\"}\nparoles\n{end_of_verse}\n\
         {start_of_chorus: label=\"Refrain\" shape=\"x\"}\nb\n{end_of_chorus}\n\
         {start_of_grid: 1+4x4+1}\n| A |\n{end_of_grid}\n\
         {sog shape=\"2x4\"}\n| A . . . | B . . . |\n{eog}\n\
         {comment: x=1}\n",
    );
    let labels: Vec<&str> = p.sections.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, vec!["Couplet 1", "Couplet 2", "Refrain", "Grille", "Grille", ""]);
    assert_eq!(p.sections[4].bars(), Some(2));
    assert_eq!(p.sections[5].lines[0].text, "x=1", "une valeur sans guillemets reste du texte");
    // une ligne à barres hors d'une grille reste du texte, comme avant
    let p = Prompt::parse("{sov}\n| A . . . |\n{eov}\n");
    assert_eq!((p.sections[0].lines[0].grid.as_ref(), p.sections[0].bars()), (None, None));
}

/// Dans une grille, une ligne **sans barre** est du texte, et `{x_namfoop_bars}` gagne sur
/// la grille.
#[test]
fn text_in_a_grid_and_written_bars_win() {
    let p = Prompt::parse(
        "{start_of_grid label=\"A\"}\n{x_namfoop_bars: 12}\n| A . . . | % |\npas de barre\n{end_of_grid}\n",
    );
    let s = &p.sections[0];
    assert_eq!((s.grid_bars(), s.bars()), (Some(2), Some(12)));
    assert!(s.lines[1].grid.is_none() && s.lines[1].text == "pas de barre");
}

/// Le squelette créé par `t` s'analyse : ce qu'on écrit pour l'utilisateur doit être lisible
/// par notre propre lecteur.
#[test]
fn the_skeleton_of_a_new_prompt_parses() {
    let p = Prompt::parse(&Prompt::skeleton("Le Phare"));
    assert_eq!(p.title.as_deref(), Some("Le Phare"));
    assert_eq!(p.sections.len(), 1);
    assert_eq!(p.sections[0].label, "Couplet 1");
    assert_eq!(p.line_count(), 0, "une section vide, prête à écrire");
}
