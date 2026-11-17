//! **Le prompteur d'un morceau** : un fichier [ChordPro](https://www.chordpro.org/) lu, pas écrit
//! (§ 10 de `docs/design/set.md`, #222).
//!
//! Pourquoi ChordPro : c'est le format **de fait** des logiciels de set list, et le texte d'un
//! concert ne doit pas être prisonnier de namfoop. On en lit un **sous-ensemble** et on ignore le
//! reste **sans erreur** — un fichier écrit pour un autre logiciel doit s'afficher ici, même
//! imparfaitement, plutôt que de refuser de s'ouvrir la veille d'un concert.
//!
//! Ce module ne connaît **ni écran ni tempo** : il rend des sections, des lignes, des accords et
//! les quatre directives `x_namfoop_…`. Le placement dans le temps est l'affaire de la battue
//! (US 6b), l'affichage celle de `namfoop`.

use std::path::Path;

use crate::{Error, Result};

/// Un accord posé **dans** une ligne de paroles, à une position en **caractères** (et non en
/// octets : une ligne de paroles est pleine d'accents).
#[derive(Clone, Debug, PartialEq)]
pub struct Chord {
    pub at: usize,
    pub name: String,
}

/// Une ligne du prompteur.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    /// Les paroles, accords retirés. Vide : une ligne blanche (séparation de strophes).
    pub text: String,
    pub chords: Vec<Chord>,
    /// `{comment}` / `{c}` : une **cue**, à afficher en jaune — ce n'est pas du texte à chanter.
    pub cue: bool,
}

impl Line {
    fn is_blank(&self) -> bool {
        self.text.trim().is_empty() && self.chords.is_empty() && !self.cue
    }
}

/// Une **section** : couplet, refrain, pont, ou n'importe quel nom libre.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    /// Le genre tel que ChordPro le nomme (`verse`, `chorus`, `bridge`, un nom libre). Vide : du
    /// texte sans `{start_of_…}` autour — un `.cho` n'est pas obligé d'avoir des sections.
    pub kind: String,
    /// L'étiquette affichée : celle du fichier, sinon le genre traduit (`Couplet`, `Refrain`).
    pub label: String,
    pub lines: Vec<Line>,
    /// `{x_namfoop_sound}` : **le son du set** auquel cette section est liée. Passer sur ce son
    /// amène le prompteur ici (§ 10).
    pub sound: Option<String>,
    /// `{x_namfoop_bars}` : la durée de la section en mesures.
    pub bars: Option<u32>,
    /// Le tempo et la mesure **en cours** à l'entrée de la section (`{tempo}` / `{time}` valent à
    /// partir de là). Lus et gardés : c'est la carte des mesures de l'US 6b.
    pub tempo_bpm: Option<f32>,
    pub time: Option<String>,
    /// `{x_namfoop_beats: 2+2+3}` : le regroupement des temps d'une mesure composée ou impaire.
    pub beats: Option<Vec<u8>>,
}

/// Le prompteur d'un morceau.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prompt {
    pub title: Option<String>,
    /// `{duration}` en secondes (`3:45` ou `225`). Sans `x_namfoop_bars`, c'est elle qui se
    /// répartit sur les lignes (§ 10).
    pub duration_s: Option<f32>,
    /// Le **premier** `{tempo}` / `{time}` du fichier : ils servent quand le `.set.toml` ne donne
    /// rien (§ 10).
    pub tempo_bpm: Option<f32>,
    pub time: Option<String>,
    pub sections: Vec<Section>,
}

impl Prompt {
    /// Lit un `.cho`. L'erreur est **réservée au fichier** (absent, illisible) : un contenu
    /// bancal, lui, s'affiche — le § 10 l'exige, et un concert ne s'arrête pas pour une directive
    /// inconnue.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Msg(format!("prompteur {} : {e}", path.display())))?;
        Ok(Self::parse(&text))
    }

    /// Le contenu d'un `.cho` tout neuf, créé par `t` quand le morceau n'en a pas encore : son
    /// titre, et de quoi savoir quoi écrire sans ouvrir la documentation.
    pub fn skeleton(title: &str) -> String {
        format!(
            "{{title: {title}}}\n\
             # ChordPro — namfoop lit un sous-ensemble (docs/guide.md, « Le mode Set »).\n\
             # {{start_of_verse: Couplet 1}} … {{end_of_verse}} : une section\n\
             # {{comment: ...}} : une cue en jaune   ·   [Am]paroles : un accord\n\
             # {{x_namfoop_sound: <preset>}} : lie la section à un son du morceau\n\
             # {{x_namfoop_bars: 8}} : sa durée en mesures\n\
             \n\
             {{start_of_verse: Couplet 1}}\n\
             \n\
             {{end_of_verse}}\n"
        )
    }

    /// Analyse un texte ChordPro. **Rien ne peut échouer ici** : ce qu'on ne comprend pas est
    /// ignoré, et le reste s'affiche.
    pub fn parse(text: &str) -> Self {
        let mut out = Prompt::default();
        // les valeurs **en cours** : une `{tempo}` ou une `{time}` vaut à partir de là (§ 10)
        let (mut tempo, mut time, mut beats) = (None, None, None);
        // la section ouverte, ou celle qu'on a dû ouvrir pour accueillir du texte sans section
        let mut open: Option<Section> = None;
        // `{x_namfoop_sound}` rencontrée **avant** le `{start_of_…}` : on la garde pour lui
        let mut pending_sound: Option<String> = None;
        let push = |open: &mut Option<Section>, out: &mut Prompt| {
            if let Some(mut s) = open.take() {
                while s.lines.first().is_some_and(|l| l.is_blank()) {
                    s.lines.remove(0);
                }
                while s.lines.last().is_some_and(|l| l.is_blank()) {
                    s.lines.pop();
                }
                out.sections.push(s);
            }
        };
        for raw in text.lines() {
            let line = raw.trim();
            // `#` : un commentaire du fichier, pas une cue
            if line.starts_with('#') {
                continue;
            }
            if let Some(d) = directive(line) {
                let (name, value) = d;
                match name.as_str() {
                    "title" | "t" => out.title = value.clone(),
                    "duration" => out.duration_s = value.as_deref().and_then(duration_s),
                    "tempo" | "bpm" => {
                        tempo = value.as_deref().and_then(|v| v.trim().parse::<f32>().ok());
                        out.tempo_bpm = out.tempo_bpm.or(tempo);
                        if let Some(s) = open.as_mut() {
                            s.tempo_bpm = tempo;
                        }
                    }
                    "time" | "meter" => {
                        time = value.clone();
                        // une nouvelle mesure annule le regroupement de l'ancienne
                        beats = None;
                        out.time = out.time.clone().or_else(|| time.clone());
                        if let Some(s) = open.as_mut() {
                            s.time = time.clone();
                            s.beats = None;
                        }
                    }
                    "x_namfoop_beats" => {
                        beats = value.as_deref().map(beat_groups).filter(|b| !b.is_empty());
                        if let Some(s) = open.as_mut() {
                            s.beats = beats.clone();
                        }
                    }
                    "x_namfoop_sound" => match open.as_mut() {
                        Some(s) => s.sound = value.clone(),
                        None => pending_sound = value.clone(),
                    },
                    "x_namfoop_bars" => {
                        let bars = value.as_deref().and_then(|v| v.trim().parse::<u32>().ok());
                        if let Some(s) = open.as_mut() {
                            s.bars = bars;
                        }
                    }
                    // les cues : toutes les formes de commentaire de ChordPro
                    "comment" | "c" | "comment_italic" | "ci" | "comment_box" | "cb" => {
                        let text = value.clone().unwrap_or_default();
                        open.get_or_insert_with(Section::default).lines.push(Line {
                            text,
                            cue: true,
                            ..Default::default()
                        });
                    }
                    _ => {
                        if let Some(kind) = starts_section(&name) {
                            push(&mut open, &mut out);
                            let label = value
                                .clone()
                                .filter(|v| !v.trim().is_empty())
                                .unwrap_or_else(|| default_label(&kind));
                            open = Some(Section {
                                kind,
                                label,
                                sound: pending_sound.take(),
                                tempo_bpm: tempo,
                                time: time.clone(),
                                beats: beats.clone(),
                                ..Default::default()
                            });
                        } else if ends_section(&name) {
                            push(&mut open, &mut out);
                        }
                        // tout le reste est ignoré : c'est la règle du § 10
                    }
                }
                continue;
            }
            // une ligne de texte : elle ouvre au besoin une section sans nom — un `.cho` n'est
            // pas obligé d'avoir des sections, et son texte doit s'afficher quand même
            let parsed = lyrics(line);
            let s = open.get_or_insert_with(Section::default);
            if !parsed.is_blank() || !s.lines.is_empty() {
                s.lines.push(parsed);
            }
        }
        push(&mut open, &mut out);
        out.sections.retain(|s| !s.lines.is_empty() || !s.label.is_empty());
        out
    }

    /// La **prochaine section liée à ce son**, à partir de `from` (compris) : c'est ce que le § 10
    /// demande quand on change de son à l'arrêt. On repart du début si on ne trouve rien après,
    /// pour qu'un son joué plus tôt dans le morceau ramène bien à sa section.
    pub fn section_of_sound(&self, sound: &str, from: usize) -> Option<usize> {
        let is_it = |s: &Section| s.sound.as_deref().is_some_and(|x| x == sound);
        (from..self.sections.len())
            .find(|i| is_it(&self.sections[*i]))
            .or_else(|| (0..from.min(self.sections.len())).find(|i| is_it(&self.sections[*i])))
    }

    /// Le nombre de lignes de texte, cues comprises : de quoi dire « ce prompteur est vide ».
    pub fn line_count(&self) -> usize {
        self.sections.iter().map(|s| s.lines.len()).sum()
    }
}

/// `{nom}` ou `{nom: valeur}`. Le nom est **replié en minuscules**, la valeur gardée telle quelle.
fn directive(line: &str) -> Option<(String, Option<String>)> {
    let inner = line.strip_prefix('{')?.strip_suffix('}')?;
    let (name, value) = match inner.split_once(':') {
        Some((n, v)) => (n, Some(v.trim().to_string())),
        None => (inner, None),
    };
    Some((name.trim().to_lowercase(), value))
}

/// `start_of_verse`, `sov`, `soc`, `sob`… → le genre de la section. `None` : pas une ouverture.
fn starts_section(name: &str) -> Option<String> {
    if let Some(kind) = name.strip_prefix("start_of_") {
        return (!kind.is_empty()).then(|| kind.to_string());
    }
    // les formes courtes de ChordPro : `so` + la première lettre du genre
    match name {
        "sov" => Some("verse".into()),
        "soc" => Some("chorus".into()),
        "sob" => Some("bridge".into()),
        "sot" => Some("tab".into()),
        "sog" => Some("grid".into()),
        _ => None,
    }
}

fn ends_section(name: &str) -> bool {
    name.starts_with("end_of_") || matches!(name, "eov" | "eoc" | "eob" | "eot" | "eog")
}

/// L'étiquette d'une section qui n'en donne pas : les trois genres standard sont traduits, un nom
/// libre garde le sien avec une majuscule.
fn default_label(kind: &str) -> String {
    match kind {
        "verse" => "Couplet".into(),
        "chorus" => "Refrain".into(),
        "bridge" => "Pont".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

/// `3:45` ou `225` → des secondes.
fn duration_s(v: &str) -> Option<f32> {
    let v = v.trim();
    match v.split_once(':') {
        Some((m, s)) => Some(m.trim().parse::<f32>().ok()? * 60.0 + s.trim().parse::<f32>().unwrap_or(0.0)),
        None => v.parse().ok(),
    }
}

/// `2+2+3` → les groupes de temps d'une mesure.
fn beat_groups(v: &str) -> Vec<u8> {
    v.split(['+', ' ', ',']).filter_map(|x| x.trim().parse::<u8>().ok()).filter(|x| *x > 0).collect()
}

/// Sépare les accords `[Am]` des paroles. Les positions sont en **caractères** du texte rendu.
fn lyrics(line: &str) -> Line {
    let mut out = Line::default();
    let mut chars = line.chars().peekable();
    let mut at = 0usize;
    while let Some(c) = chars.next() {
        if c != '[' {
            out.text.push(c);
            at += 1;
            continue;
        }
        let mut name = String::new();
        let mut closed = false;
        for c in chars.by_ref() {
            if c == ']' {
                closed = true;
                break;
            }
            name.push(c);
        }
        // un `[` jamais fermé n'est pas un accord : on rend le texte tel quel
        if !closed {
            out.text.push('[');
            out.text.push_str(&name);
            at += 1 + name.chars().count();
        } else if !name.trim().is_empty() {
            out.chords.push(Chord { at, name: name.trim().to_string() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
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
}
