//! **Le prompteur d'un morceau** : un fichier [ChordPro](https://www.chordpro.org/) lu, pas écrit
//! (§ 10 de `docs/design/set.md`).
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

use crate::grid::{self, Row};
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
    /// Une **ligne de mesures** d'une grille (`{start_of_grid}`) : ses mesures et son repère.
    /// `text` garde alors la ligne telle qu'écrite.
    pub grid: Option<Row>,
}

impl Line {
    fn is_blank(&self) -> bool {
        self.text.trim().is_empty() && self.chords.is_empty() && !self.cue
    }

    /// Le nombre de mesures de la ligne : celles de sa grille, zéro pour du texte.
    pub fn bar_count(&self) -> u32 {
        self.grid.as_ref().map_or(0, Row::bar_count)
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

impl Section {
    /// La durée **écrite dans la grille** : la somme des mesures de ses lignes. `None` : la
    /// section n'a aucune ligne de mesures.
    pub fn grid_bars(&self) -> Option<u32> {
        self.lines.iter().any(|l| l.grid.is_some()).then(|| self.lines.iter().map(Line::bar_count).sum())
    }

    /// La durée de la section : `{x_namfoop_bars}` s'il est écrit, sinon celle de sa grille.
    pub fn bars(&self) -> Option<u32> {
        self.bars.or_else(|| self.grid_bars())
    }
}

/// Le prompteur d'un morceau.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prompt {
    pub title: Option<String>,
    /// `{key}` : la tonalité, telle qu'écrite.
    pub key: Option<String>,
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
             # ou une grille : {{start_of_grid label=\"Intro\"}} puis | Em . . . | G . . . | paroles\n\
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
        // de même pour une cue écrite hors de toute section : c'est celle de la section qui suit
        let mut pending_cues: Vec<Line> = Vec::new();
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
                let Directive { name, value } = d;
                match name.as_str() {
                    "title" | "t" => out.title = value.clone(),
                    "key" => out.key = value.clone().filter(|v| !v.trim().is_empty()),
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
                        let cue =
                            Line { text: value.clone().unwrap_or_default(), cue: true, ..Default::default() };
                        match open.as_mut() {
                            Some(s) => s.lines.push(cue),
                            None => pending_cues.push(cue),
                        }
                    }
                    _ => {
                        if let Some(kind) = starts_section(&name) {
                            push(&mut open, &mut out);
                            let label = value
                                .clone()
                                .map(|v| if kind == "grid" { strip_shape(&v) } else { v })
                                .filter(|v| !v.trim().is_empty())
                                .unwrap_or_else(|| default_label(&kind));
                            open = Some(Section {
                                kind,
                                label,
                                lines: std::mem::take(&mut pending_cues),
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
            let in_grid = open.as_ref().is_some_and(|s| s.kind == "grid");
            let parsed = match grid::row(line).filter(|_| in_grid) {
                Some(row) => Line { text: line.to_string(), grid: Some(row), ..Default::default() },
                None => lyrics(line),
            };
            if open.is_none() {
                if parsed.is_blank() {
                    continue;
                }
                open = Some(Section { lines: std::mem::take(&mut pending_cues), ..Default::default() });
            }
            let s = open.get_or_insert_with(Section::default);
            if !parsed.is_blank() || !s.lines.is_empty() {
                s.lines.push(parsed);
            }
        }
        push(&mut open, &mut out);
        // des cues après la dernière section : elles s'affichent quand même, à part
        if !pending_cues.is_empty() {
            out.sections.push(Section { lines: pending_cues, ..Default::default() });
        }
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

/// Une directive lue : son nom **replié en minuscules**, et sa valeur telle qu'écrite — ou, pour
/// la forme à attributs de ChordPro 6, son attribut `label`.
struct Directive {
    name: String,
    value: Option<String>,
}

/// `{nom}`, `{nom: valeur}`, et la forme à attributs de ChordPro 6 : `{nom label="…" shape="…"}`
/// ou `{nom: label="…"}`. Des attributs, seul `label` compte (l'étiquette d'une section) ; `shape`
/// et les autres sont ignorés — les mesures se comptent entre les barres.
fn directive(line: &str) -> Option<Directive> {
    let inner = line.strip_prefix('{')?.strip_suffix('}')?;
    // `{start_of_grid label="Intro"}` : pas de `:`, le nom s'arrête au premier blanc
    if let Some(at) = inner.find(char::is_whitespace) {
        let (name, rest) = inner.split_at(at);
        if !name.contains(':') {
            if let Some(attrs) = attributes(rest) {
                return Some(Directive { name: name.trim().to_lowercase(), value: label_of(&attrs) });
            }
        }
    }
    let (name, value) = match inner.split_once(':') {
        Some((n, v)) => {
            let v = v.trim();
            match attributes(v) {
                Some(attrs) => (n, label_of(&attrs)),
                None => (n, Some(v.to_string())),
            }
        }
        None => (inner, None),
    };
    Some(Directive { name: name.trim().to_lowercase(), value })
}

fn label_of(attrs: &[(String, String)]) -> Option<String> {
    attrs.iter().find(|(k, _)| k == "label").map(|(_, v)| v.trim().to_string())
}

/// `label="Couplet 1" shape="4x4"` → les paires. `None` si le texte ne **commence** pas par un
/// attribut entre guillemets : c'est alors une valeur simple (`{start_of_verse: Couplet 1}`). Ce
/// qui suit un attribut mal écrit est ignoré.
fn attributes(text: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let key_len = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).unwrap_or(0);
        let Some(after) = rest[key_len..].strip_prefix('=').filter(|_| key_len > 0) else { break };
        let key = rest[..key_len].to_lowercase();
        let (value, tail) = match after.chars().next() {
            Some(q @ ('"' | '\'')) => match after[1..].find(q) {
                Some(end) => (&after[1..1 + end], &after[2 + end..]),
                None => (&after[1..], ""),
            },
            // une valeur sans guillemets n'est pas un attribut : `{comment: x=1}` reste une cue
            _ => break,
        };
        out.push((key, value.to_string()));
        rest = tail.trim_start();
    }
    (!out.is_empty()).then_some(out)
}

/// La forme ancienne d'une grille, `{start_of_grid: 4x4 Intro}` : la forme d'abord (`4x4`,
/// `1+4x4+1`, `8`), le nom ensuite. On ne garde que le nom.
fn strip_shape(value: &str) -> String {
    let v = value.trim();
    let (first, rest) = v.split_once(char::is_whitespace).unwrap_or((v, ""));
    let is_shape = first.chars().any(|c| c.is_ascii_digit())
        && first.chars().all(|c| c.is_ascii_digit() || matches!(c, 'x' | 'X' | '+'));
    if is_shape {
        rest.trim().to_string()
    } else {
        v.to_string()
    }
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
        "grid" => "Grille".into(),
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
pub(crate) mod tests;
