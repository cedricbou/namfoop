//! **Le prompteur** de la vue scène (§ 10 de `docs/design/set.md`, § 4 de
//! `docs/design/prompteur.md`) : le texte du morceau joué, ses sections, ses accords, ses cues, et
//! ses **grilles** — une ligne de mesures, la mesure jouée allumée, les paroles en repère à droite.
//!
//! Ce module dessine, il ne calcule pas la position : la section, la ligne et la mesure dans la
//! ligne sont celles de `ActiveSet` (`prompt_section`, `prompt_line`, `prompt_bar`), que la battue
//! tient à jour.

use crate::theme;
use library::grid::Row;
use library::prompt::{Prompt, Section};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::super::state::set::ActiveSet;
use super::super::state::State;
use super::set::clip;

/// La marge devant une ligne de mesures : `  2/4 › ` — la mesure quand elle change, puis le
/// repère de la ligne courante.
const ROW_LEAD: usize = 7;
/// En dessous de cette place à droite d'une ligne de mesures, son repère passe dessous (§ 4).
const CUE_MIN: usize = 20;

/// Le **texte du morceau joué** : sections étiquetées, accords, grilles, cues, et la position.
///
/// Trois états de section, et c'est ce qui permet de se repérer d'un coup d'œil sur scène : la
/// **courante** en vert avec `▸`, les **passées** en gris, les **suivantes** en gras. La ligne
/// courante porte `›`. Le texte part **deux lignes avant** la position : on ne lit pas la première
/// ligne de l'écran, on a besoin de voir d'où l'on vient.
pub(super) fn prompt_lines(
    s: &State,
    set: &ActiveSet,
    at: (usize, usize),
    w: usize,
    room: usize,
) -> Vec<Line<'static>> {
    use crate::browse::state::set::prompt::Loaded;
    let song = &set.spec.songs[at.0];
    let Some(loaded) = s.prompt_of(at.0) else {
        // pas de `prompt =` dans le set : on dit **comment** en avoir un, pas « rien »
        let mut v = vec![Line::styled("  pas de prompteur pour ce morceau", Style::new().fg(theme::dim()))];
        if !s.locked {
            let file = crate::browse::state::set::prompt::prompt_file_name(song.label());
            v.push(Line::styled(
                format!("  t l'écrit dans ton éditeur (ChordPro : prompts/{file})"),
                Style::new().fg(theme::dim()),
            ));
        }
        return v;
    };
    let prompt = match loaded {
        Loaded::Ok(p) => p,
        // fichier absent ou illisible : **jamais** d'erreur bloquante, un message dans le cadre
        Loaded::Failed(why) => {
            return vec![Line::styled(format!("  ⚠ {why}"), Style::new().fg(theme::warn()))];
        }
    };
    if prompt.sections.is_empty() {
        return vec![Line::styled(
            format!("  prompteur vide{}", if s.locked { "" } else { " : t pour l'écrire" }),
            Style::new().fg(theme::dim()),
        )];
    }
    // on met tout à plat une fois : c'est la seule façon de faire défiler le texte **et** ses
    // en-têtes de section ensemble
    let mut flat: Vec<Line<'static>> = Vec::new();
    let mut here = 0usize;
    let now = set.prompt_section;
    let lit = if set.beat.running { set.prompt_bar } else { None };
    for group in groups(prompt) {
        let state = |i: usize| match i.cmp(&now) {
            std::cmp::Ordering::Less => Was::Past,
            std::cmp::Ordering::Equal => Was::Now,
            std::cmp::Ordering::Greater => Was::Next,
        };
        let head_state = if group.contains(&now) { Was::Now } else { state(group.start) };
        let first = &prompt.sections[group.start];
        if !first.label.is_empty() {
            flat.push(section_head(&prompt.sections[group.clone()], head_state, prompt));
        }
        let cell_w = cell_width(&prompt.sections[group.clone()]);
        for i in group.clone() {
            let sec = &prompt.sections[i];
            // devant une grille dont la mesure change, au sein d'un même nom : sa mesure (§ 4)
            let changes = i > group.start && sec.time != prompt.sections[i - 1].time;
            let tag = if changes { sec.time.clone().unwrap_or_default() } else { String::new() };
            let mut tagged = false;
            for (k, line) in sec.lines.iter().enumerate() {
                let cur = state(i) == Was::Now && k == set.prompt_line;
                if cur {
                    here = flat.len();
                }
                match line.grid.as_ref() {
                    Some(row) => {
                        let tag = if tagged { "" } else { tag.as_str() };
                        tagged = true;
                        let look = RowLook { cur, past: state(i) == Was::Past, lit: lit.filter(|_| cur) };
                        flat.extend(grid_row(row, tag, cell_w, w, look));
                    }
                    None => flat.extend(lyric_lines(line, w, cur, state(i) == Was::Past)),
                }
            }
        }
        if group.end < prompt.sections.len() {
            flat.push(Line::raw(""));
        }
    }
    // la fenêtre : deux lignes avant la position (§ 10)
    let from = here.saturating_sub(2).min(flat.len().saturating_sub(room.max(1)));
    flat.into_iter().skip(from).take(room.max(1)).collect()
}

/// **Des sections voisines de même nom** s'affichent comme une seule (§ 4) : le pont en trois
/// grilles, dont une en 2/4, a un seul titre. `Fin` / `Début`, eux, vont toujours de l'une à
/// l'autre : seul le dessin les regroupe.
fn groups(prompt: &Prompt) -> Vec<std::ops::Range<usize>> {
    let same = |a: &Section, b: &Section| {
        !a.label.trim().is_empty() && a.label.trim().to_lowercase() == b.label.trim().to_lowercase()
    };
    let mut out: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, sec) in prompt.sections.iter().enumerate() {
        match out.last_mut() {
            Some(g) if same(&prompt.sections[g.end - 1], sec) => g.end = i + 1,
            _ => out.push(i..i + 1),
        }
    }
    out
}

/// La largeur d'une case de mesure dans un groupe de sections : la plus large, pour que les
/// mesures s'alignent d'une ligne à l'autre (§ 4).
fn cell_width(sections: &[Section]) -> usize {
    sections
        .iter()
        .flat_map(|s| s.lines.iter())
        .filter_map(|l| l.grid.as_ref())
        .flat_map(|r| r.bars.iter())
        .map(|b| b.text().chars().count())
        .max()
        .unwrap_or(0)
}

/// La largeur de la plus large ligne de mesures du morceau, marge comprise, sans son repère :
/// en deçà, la set list se replie pour laisser la grille entière (§ 4). 0 : aucune grille.
pub(super) fn grid_width(prompt: &Prompt) -> usize {
    groups(prompt)
        .into_iter()
        .flat_map(|g| {
            let secs = &prompt.sections[g];
            let cell_w = cell_width(secs);
            secs.iter()
                .flat_map(|s| s.lines.iter())
                .filter_map(|l| l.grid.as_ref())
                .map(move |r| ROW_LEAD + row_width(r, cell_w))
                .collect::<Vec<_>>()
        })
        .max()
        .unwrap_or(0)
}

/// La largeur d'une ligne de mesures (marge gauche, mesures, barre finale), sans repère.
fn row_width(row: &Row, cell_w: usize) -> usize {
    let left = if row.left.is_empty() { 0 } else { row.left.chars().count() + 1 };
    left + row.bars.iter().map(|b| bar_width(&b.open, cell_w)).sum::<usize>() + row.close.chars().count()
}

/// `| Em . . . ` : la barre, puis la case entourée d'un espace.
fn bar_width(open: &str, cell_w: usize) -> usize {
    open.chars().count() + cell_w + 2
}

/// Ce qui change le dessin d'une ligne de mesures.
#[derive(Clone, Copy)]
struct RowLook {
    /// La ligne courante : `›`, et son repère en gras.
    cur: bool,
    /// Une section passée : tout en gris.
    past: bool,
    /// La mesure allumée dans la ligne, battue en marche.
    lit: Option<u32>,
}

/// Une **ligne de mesures** : les accords en couleur, les barres et les points en gris, la mesure
/// jouée allumée et celles déjà jouées grisées, puis le repère à droite — ou dessous quand il reste
/// moins de [`CUE_MIN`] colonnes. Une ligne trop large se coupe à une barre de mesure (§ 4).
fn grid_row(row: &Row, tag: &str, cell_w: usize, w: usize, look: RowLook) -> Vec<Line<'static>> {
    let dim = Style::new().fg(theme::dim());
    let mark = if look.cur { "›" } else { " " };
    let lead = format!("{tag:>4} {mark} ");
    let blank_lead = " ".repeat(ROW_LEAD);
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = vec![Span::styled(lead, if look.cur { Style::new() } else { dim })];
    let mut used = ROW_LEAD;
    if !row.left.is_empty() {
        used += row.left.chars().count() + 1;
        spans.push(Span::styled(format!("{} ", row.left), dim));
    }
    for (k, bar) in row.bars.iter().enumerate() {
        let bw = bar_width(&bar.open, cell_w);
        // la ligne ne tient pas : elle se coupe **à une barre** et continue dessous
        if used + bw + row.close.chars().count() > w && used > ROW_LEAD {
            out.push(Line::from(std::mem::take(&mut spans)));
            spans.push(Span::raw(blank_lead.clone()));
            used = ROW_LEAD;
        }
        used += bw;
        spans.push(Span::styled(bar.open.clone(), dim));
        let k = k as u32;
        let text = format!(" {:<cell_w$} ", bar.text(), cell_w = cell_w);
        match look.lit {
            Some(l) if l == k => spans.push(Span::styled(
                text,
                Style::new().fg(theme::on_accent()).bg(theme::set()).add_modifier(Modifier::BOLD),
            )),
            // les mesures déjà jouées de la ligne courante, et toute section passée : en gris
            Some(l) if k < l => spans.push(Span::styled(text, dim)),
            _ if look.past => spans.push(Span::styled(text, dim)),
            _ => spans.extend(cells(&text)),
        }
    }
    spans.push(Span::styled(row.close.clone(), dim));
    used += row.close.chars().count();
    let right = row.right.trim();
    if right.is_empty() {
        out.push(Line::from(spans));
        return out;
    }
    let cue_style = match (look.cur, look.past) {
        (true, _) => Style::new().add_modifier(Modifier::BOLD),
        (_, true) => dim,
        _ => Style::new().fg(theme::text()),
    };
    let left_room = w.saturating_sub(used + 2);
    if left_room >= CUE_MIN {
        spans.push(Span::styled(format!("  {}", clip(right, left_room)), cue_style));
        out.push(Line::from(spans));
    } else {
        out.push(Line::from(spans));
        out.push(Line::styled(format!("{blank_lead}{}", clip(right, w.saturating_sub(ROW_LEAD))), dim));
    }
    out
}

/// Le contenu d'une mesure : les accords en couleur, `.`, `/` et `%` en gris.
fn cells(text: &str) -> Vec<Span<'static>> {
    let chord = Style::new().fg(theme::chord()).add_modifier(Modifier::BOLD);
    let dim = Style::new().fg(theme::dim());
    let mut out = Vec::new();
    for (i, part) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(Span::raw(" "));
        }
        if part.is_empty() {
            continue;
        }
        let quiet = matches!(part, "." | "/" | "%" | "%%");
        out.push(Span::styled(part.to_string(), if quiet { dim } else { chord }));
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Was {
    Past,
    Now,
    Next,
}

/// L'en-tête d'une section — ou d'un groupe de sections voisines de même nom : son étiquette, sa
/// durée en mesures, ce qu'elle change (mesure, tempo), et **le son auquel elle est liée** — c'est
/// ce qui fait le lien avec la set list.
fn section_head(secs: &[Section], state: Was, prompt: &Prompt) -> Line<'static> {
    let sec = &secs[0];
    let mark = if state == Was::Now { "▸" } else { " " };
    let bars = match secs.iter().map(Section::bars).sum::<Option<u32>>() {
        Some(b) => format!("{b} mes."),
        None => String::new(),
    };
    let mut extra = String::new();
    // une mesure ou un tempo ne se dit que s'il **change** : l'écrire partout serait du bruit
    if sec.time != prompt.time {
        if let Some(t) = sec.time.as_deref() {
            extra.push_str(&format!(" {t}"));
        }
    }
    if sec.tempo_bpm != prompt.tempo_bpm {
        if let Some(t) = sec.tempo_bpm {
            extra.push_str(&format!(" ♩{t:.0}"));
        }
    }
    let mut spans = vec![Span::styled(
        format!(" {mark} {:<14}{:<8}{extra}", sec.label.to_uppercase(), bars),
        match state {
            Was::Now => Style::new().fg(theme::set()).add_modifier(Modifier::BOLD),
            Was::Past => Style::new().fg(theme::dim()),
            Was::Next => Style::new().add_modifier(Modifier::BOLD),
        },
    )];
    if let Some(sound) = secs.iter().find_map(|s| s.sound.as_deref()) {
        spans.push(Span::styled(
            format!("  → {sound}"),
            Style::new().fg(if state == Was::Now { theme::set() } else { theme::dim() }),
        ));
    }
    Line::from(spans)
}

/// Une ligne de paroles : les accords **au-dessus** quand la largeur le permet (§ 10), sinon
/// entre crochets dans le texte, comme dans le fichier.
fn lyric_lines(line: &library::prompt::Line, w: usize, cur: bool, past: bool) -> Vec<Line<'static>> {
    let text_style = match (cur, past, line.cue) {
        // une cue n'est pas du texte à chanter : elle est en jaune (§ 10)
        (_, _, true) => Style::new().fg(theme::cue()),
        (true, _, _) => Style::new().fg(theme::on_accent()).bg(theme::set()).add_modifier(Modifier::BOLD),
        (_, true, _) => Style::new().fg(theme::dim()),
        _ => Style::new(),
    };
    let mark = if cur { "›" } else { " " };
    if line.chords.is_empty() {
        return vec![Line::from(Span::styled(format!("   {mark} {}", clip(&line.text, w)), text_style))];
    }
    let chord_line = chord_row(line);
    // les accords ne tiennent pas au-dessus : on les remet dans le texte, lisible à coup sûr
    if chord_line.chars().count() + 5 > w {
        return vec![Line::from(Span::styled(format!("   {mark} {}", clip(&inline(line), w)), text_style))];
    }
    vec![
        Line::from(Span::styled(
            format!("     {chord_line}"),
            Style::new().fg(theme::chord()).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(format!("   {mark} {}", clip(&line.text, w)), text_style)),
    ]
}

/// Les accords placés **au-dessus** de leur syllabe.
fn chord_row(line: &library::prompt::Line) -> String {
    let mut out = String::new();
    for c in &line.chords {
        // deux accords trop proches : on garde un espace, plutôt que de les coller
        while out.chars().count() < c.at {
            out.push(' ');
        }
        if !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(&c.name);
    }
    out
}

/// La ligne telle qu'elle est écrite dans le fichier : `[Am]paroles`.
fn inline(line: &library::prompt::Line) -> String {
    let mut out = String::new();
    let chars: Vec<char> = line.text.chars().collect();
    let mut k = 0usize;
    for c in &line.chords {
        while k < c.at.min(chars.len()) {
            out.push(chars[k]);
            k += 1;
        }
        out.push('[');
        out.push_str(&c.name);
        out.push(']');
    }
    out.extend(chars[k.min(chars.len())..].iter());
    out
}
