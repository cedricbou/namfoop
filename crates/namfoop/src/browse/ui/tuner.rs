//! **L'accordeur** (`docs/design/accordeur.md` § 4 à § 7) : la note en grandes lettres, l'aiguille
//! de -50 à +50 cents, l'écart et le geste à faire. Dans l'espace Set et sur scène, il prend la
//! colonne du prompteur et garde le morceau suivant dessous ; ailleurs, c'est une fenêtre centrée.
//!
//! **Le gris se pose sur l'image** (§ 7) : l'écran se dessine comme d'habitude, [`grey_out`] passe
//! tout ce qu'il a dessiné en gris, puis l'accordeur se dessine par-dessus. Aucun écran n'a à
//! savoir que la basse est coupée.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::super::state::tuner::{TunerView, IN_TUNE_CENTS, NEAR_CENTS};
use super::super::state::State;
use crate::theme;
use dsp::pitch::Reading;

/// Le titre du cadre.
const TITLE: &str = " ACCORDEUR · ma basse est coupée partout · micro et clic continuent ";
/// Largeur de la fenêtre centrée (Bibliothèque, Chaîne).
const WINDOW_W: u16 = 72;
/// Sous cette hauteur, les grandes lettres laissent place à la note sur une ligne (§ 11).
const BIG_MIN_H: u16 = 16;
/// Les cases de l'aiguille : 41 cases de 2,5 cents, de -50 à +50.
const CELLS: usize = 41;

/// Passe tout l'écran déjà dessiné en gris, sans fond ni gras (§ 7).
pub fn grey_out(buf: &mut Buffer) {
    let area = buf.area;
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_fg(theme::dim());
                cell.set_bg(theme::inherit());
                cell.modifier = Modifier::empty();
            }
        }
    }
}

/// La couleur d'un écart : les trois paliers (§ 7).
fn grade(cents: f32) -> Style {
    let c = cents.abs();
    if c <= IN_TUNE_CENTS {
        theme::tuner_in()
    } else if c <= NEAR_CENTS {
        theme::tuner_near()
    } else {
        theme::tuner_far()
    }
}

/// Les lettres de 5 lignes : un `#` est un trait doublé (`██`), pour se lire de loin.
fn glyph(c: char) -> &'static [&'static str; 5] {
    match c {
        'A' => &[".###.", "#...#", "#####", "#...#", "#...#"],
        'B' => &["####.", "#...#", "####.", "#...#", "####."],
        'C' => &[".####", "#....", "#....", "#....", ".####"],
        'D' => &["####.", "#...#", "#...#", "#...#", "####."],
        'E' => &["#####", "#....", "####.", "#....", "#####"],
        'F' => &["#####", "#....", "####.", "#....", "#...."],
        'G' => &[".####", "#....", "#..##", "#...#", ".####"],
        // le bémol, plus étroit
        _ => &["#..", "#..", "###", "#.#", "###"],
    }
}

/// La note en grandes lettres : 5 lignes de texte.
fn big_note(name: &str) -> [String; 5] {
    let mut rows: [String; 5] = Default::default();
    for (i, c) in name.chars().enumerate() {
        for (row, bits) in rows.iter_mut().zip(glyph(c)) {
            if i > 0 {
                row.push_str("  ");
            }
            for b in bits.chars() {
                row.push_str(if b == '#' { "██" } else { "  " });
            }
        }
    }
    rows
}

/// L'écart écrit : `+7 cents`, `−25 cents`, `±0 cents`.
fn cents_text(cents: f32) -> String {
    let c = cents.round() as i32;
    match c {
        0 => "±0 cents".into(),
        c if c > 0 => format!("+{c} cents"),
        c => format!("−{} cents", -c),
    }
}

/// Les lignes de l'accordeur, pour une largeur `w` et une hauteur `h`.
fn lines(s: &State, w: usize, h: u16) -> Vec<Line<'static>> {
    let view = s.tuner_view().unwrap_or(TunerView::Idle);
    let (reading, live): (Option<Reading>, bool) = match view {
        TunerView::Note(r) => (Some(r), true),
        TunerView::Fading(r) => (Some(r), false),
        TunerView::Idle => (None, false),
    };
    let style = match reading {
        Some(r) if live => grade(r.cents),
        _ => Style::new().fg(theme::dim()),
    };
    let pad = |text_w: usize| " ".repeat(w.saturating_sub(text_w) / 2);
    let mut out = vec![Line::raw("")];
    let big = h >= BIG_MIN_H;
    match reading {
        None => {
            let room = if big { 5 } else { 1 };
            for i in 0..room {
                if i == room / 2 {
                    let t = "joue une corde";
                    out.push(Line::styled(
                        format!("{}{t}", pad(t.chars().count())),
                        Style::new().fg(theme::dim()),
                    ));
                } else {
                    out.push(Line::raw(""));
                }
            }
        }
        Some(r) if big => {
            let rows = big_note(r.name());
            let width = rows[0].chars().count() + 12;
            for (i, row) in rows.into_iter().enumerate() {
                let mut spans = vec![Span::raw(pad(width)), Span::styled(row, style)];
                if i == 4 {
                    spans.push(Span::styled(
                        format!("  octave {}", r.octave()),
                        Style::new().fg(theme::dim()),
                    ));
                }
                out.push(Line::from(spans));
            }
        }
        Some(r) => {
            let t = format!("{}{}", r.name(), r.octave());
            out.push(Line::from(vec![Span::raw(pad(t.chars().count())), Span::styled(t, style)]));
        }
    }
    out.push(Line::raw(""));
    // l'aiguille : 41 cases, un repère à 0, ±25 et ±50
    let scale = "-50       -25        0        +25       +50";
    let left = pad(CELLS + 2);
    out.push(Line::styled(format!("{left}{scale}"), Style::new().fg(theme::dim())));
    let ticks: String = (0..CELLS)
        .map(|i| match i {
            20 => '┃',
            0 | 10 | 30 | 40 => '┆',
            _ => '·',
        })
        .collect();
    out.push(Line::styled(format!("{left} {ticks}"), Style::new().fg(theme::dim())));
    match reading {
        Some(r) if live => {
            let at = (((r.cents + 50.0) / 2.5).round() as usize).min(CELLS - 1);
            out.push(Line::from(vec![
                Span::raw(format!("{left} {}", " ".repeat(at))),
                Span::styled("▲", style),
            ]));
        }
        _ => out.push(Line::raw("")),
    }
    out.push(Line::raw(""));
    // l'écart et le geste à faire : la flèche dit le geste, pas l'état
    match reading {
        Some(r) if live => {
            let gesture = if r.cents.abs() <= IN_TUNE_CENTS {
                Span::styled("✓ juste", theme::tuner_in())
            } else if r.cents > 0.0 {
                Span::styled("▼ descends", theme::tuner_far())
            } else {
                Span::styled("▲ monte", theme::tuner_far())
            };
            let text = cents_text(r.cents);
            let width = text.chars().count() + 4 + gesture.content.chars().count();
            out.push(Line::from(vec![
                Span::raw(pad(width)),
                Span::styled(text, style),
                Span::raw("    "),
                gesture,
            ]));
        }
        _ => out.push(Line::raw("")),
    }
    out
}

fn frame() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme::cut()))
        .title(Span::styled(TITLE, Style::new().fg(theme::cut()).add_modifier(Modifier::BOLD)))
}

/// L'accordeur dans la colonne du prompteur, avec `below` (le morceau suivant) dessous.
pub fn render_column(s: &State, f: &mut Frame, area: Rect, below: Vec<Line<'static>>) {
    f.render_widget(Clear, area);
    let inner_w = area.width.saturating_sub(2) as usize;
    let mut v = lines(s, inner_w, area.height);
    // le bloc « ensuite » se réduit à son titre quand la hauteur manque (§ 11)
    let room = (area.height as usize).saturating_sub(v.len() + 3);
    if !below.is_empty() && room > 0 {
        v.push(Line::raw(""));
        v.extend(below.into_iter().take(room));
    }
    f.render_widget(Paragraph::new(v).block(frame()), area);
}

/// L'accordeur en fenêtre centrée (Bibliothèque, Chaîne, `run --preset`).
pub fn render_window(s: &State, f: &mut Frame, area: Rect) {
    let w = WINDOW_W.min(area.width);
    let h = 15u16.min(area.height);
    let r = Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h };
    f.render_widget(Clear, r);
    let v = lines(s, w.saturating_sub(2) as usize, if area.height >= BIG_MIN_H { BIG_MIN_H } else { h });
    f.render_widget(
        Paragraph::new(v).alignment(Alignment::Left).block(frame()).style(Style::new().bg(theme::inherit())),
        r,
    );
}

/// Le bandeau de l'en-tête : `🔇 BASSE COUPÉE` en violet, ou `♪ SON RENDU` en vert 2 s.
pub fn banner(s: &State) -> Option<Span<'static>> {
    let flat = |t: &'static str, bg: Color| {
        Span::styled(t, Style::new().fg(theme::on_accent()).bg(bg).add_modifier(Modifier::BOLD))
    };
    if s.bass_cut() {
        Some(flat(" 🔇 BASSE COUPÉE ", theme::cut()))
    } else if s.sound_back() {
        Some(flat(" ♪ SON RENDU ", theme::set()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_letters_are_five_rows_and_flats_are_narrower() {
        let e = big_note("E");
        assert_eq!(e[0], "██████████");
        let eb = big_note("E♭");
        assert_eq!(eb[0].chars().count(), 10 + 2 + 6);
    }

    #[test]
    fn cents_are_written_with_their_sign() {
        assert_eq!(cents_text(7.2), "+7 cents");
        assert_eq!(cents_text(-25.0), "−25 cents");
        assert_eq!(cents_text(0.3), "±0 cents");
    }
}
