//! Briques communes des écrans plein écran (`setup`, `browse`) : touches, session terminal,
//! vumètre.

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use std::time::Duration;

/// Touches, indépendantes de crossterm (les machines à états restent testables sans terminal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    /// Maj + ← / → (réglage fin).
    ShiftLeft,
    ShiftRight,
    PageUp,
    PageDown,
    Enter,
    Tab,
    Esc,
    Backspace,
    Char(char),
}

pub fn map_key(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Char(c) => Key::Char(c),
        _ => return None,
    })
}

/// Attend une touche pendant au plus `timeout` (pour rafraîchir l'écran entre deux).
pub fn poll_key(timeout: Duration) -> std::io::Result<Option<Key>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    match event::read()? {
        Event::Key(k) if k.kind == KeyEventKind::Press => {
            let shift = k.modifiers.contains(event::KeyModifiers::SHIFT);
            Ok(match (map_key(k.code), shift) {
                (Some(Key::Left), true) => Some(Key::ShiftLeft),
                (Some(Key::Right), true) => Some(Key::ShiftRight),
                (key, _) => key,
            })
        }
        _ => Ok(None),
    }
}

/// Redirige stderr vers /dev/null (PipeWire / ALSA écrivent parfois directement sur stderr, ce qui
/// casserait l'affichage plein écran).
struct StderrSilencer(Option<i32>);

impl StderrSilencer {
    fn new() -> Self {
        unsafe {
            let saved = libc::dup(2);
            let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
            if saved < 0 || null < 0 {
                return Self(None);
            }
            libc::dup2(null, 2);
            libc::close(null);
            Self(Some(saved))
        }
    }
}

impl Drop for StderrSilencer {
    fn drop(&mut self) {
        if let Some(fd) = self.0 {
            unsafe {
                libc::dup2(fd, 2);
                libc::close(fd);
            }
        }
    }
}

/// Session plein écran : logs coupés, stderr redirigé, terminal en mode ratatui.
/// Tout est restauré au drop (et par ratatui en cas de panic).
pub struct Session {
    pub terminal: ratatui::DefaultTerminal,
    _silencer: StderrSilencer,
    level: log::LevelFilter,
}

impl Session {
    pub fn start() -> anyhow::Result<Self> {
        use std::io::IsTerminal;
        if !std::io::stdout().is_terminal() {
            anyhow::bail!("cette commande a besoin d'un terminal interactif");
        }
        let level = log::max_level();
        log::set_max_level(log::LevelFilter::Off);
        let silencer = StderrSilencer::new();
        let terminal = ratatui::try_init()?;
        Ok(Self { terminal, _silencer: silencer, level })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        ratatui::restore();
        log::set_max_level(self.level);
    }
}

pub const BAR: usize = 24;

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Barre de niveau -60..0 dBFS, colorée vert / jaune (> -12) / rouge (> -3).
pub fn meter(level: f32) -> Vec<Span<'static>> {
    meter_width(level, BAR)
}

/// Position (0..width) d'un niveau en dB sur une barre -60..0 dBFS.
pub fn meter_pos(db: f32, width: usize) -> usize {
    ((((db + 60.0) / 60.0).clamp(0.0, 1.0) * width as f32).round() as usize).min(width.saturating_sub(1))
}

/// Barre de niveau avec un repère `┃` à `mark_db` (crête attendue d'après la calibration).
pub fn meter_marked(level: f32, width: usize, mark_db: Option<f32>) -> Vec<Span<'static>> {
    let mut spans = meter_width(level, width);
    let Some(mark) = mark_db else { return spans };
    let pos = meter_pos(mark, width);
    // recompose la barre caractère par caractère (plein / vide), repère inséré
    let filled = spans[0].content.chars().count();
    let (fill_style, empty_style) = (spans[0].style, spans[1].style);
    let text = spans.pop().expect("texte");
    let mut out: Vec<Span<'static>> = Vec::new();
    let push = |out: &mut Vec<Span<'static>>, c: char, st: Style| match out.last_mut() {
        Some(last) if last.style == st => last.content.to_mut().push(c),
        _ => out.push(Span::styled(c.to_string(), st)),
    };
    for k in 0..width {
        if k == pos {
            push(&mut out, '┃', Style::new().fg(Color::White));
        } else if k < filled {
            push(&mut out, '█', fill_style);
        } else {
            push(&mut out, '░', empty_style);
        }
    }
    out.push(text);
    out
}

pub fn meter_width(level: f32, width: usize) -> Vec<Span<'static>> {
    let d = db(level);
    let filled = (((d + 60.0) / 60.0).clamp(0.0, 1.0) * width as f32).round() as usize;
    let color = if d > -3.0 {
        Color::Red
    } else if d > -12.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    let text = if level <= 1e-6 { "  -inf".to_string() } else { format!("{d:>6.1}") };
    vec![
        Span::styled("█".repeat(filled), Style::new().fg(color)),
        Span::styled("░".repeat(width - filled), Style::new().fg(Color::DarkGray)),
        Span::raw(format!(" {text} dB")),
    ]
}

/// Contenu texte d'un écran rendu (tests avec `TestBackend`).
#[cfg(test)]
pub fn screen_text(width: u16, height: u16, draw: impl FnOnce(&mut ratatui::Frame)) -> String {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    t.draw(draw).unwrap();
    let buf = t.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_colors_and_width() {
        let spans = meter(1.0);
        assert_eq!(spans[0].content.chars().count(), BAR);
        assert_eq!(spans[0].style.fg, Some(Color::Red));
        assert_eq!(meter(0.01)[0].style.fg, Some(Color::Green));
    }

    #[test]
    fn calibration_mark() {
        assert_eq!(meter_pos(-8.5, 12), 10);
        assert_eq!(meter_pos(-8.5, 24), 21);
        assert_eq!(meter_pos(0.0, 12), 11, "jamais hors de la barre");
        let bar: String = meter_marked(0.1, 12, Some(-8.5)).iter().map(|s| s.content.to_string()).collect();
        // -20 dBFS : 8 cases pleines, repère en 10e position
        assert!(bar.starts_with("████████░░┃░"), "{bar}");
        assert_eq!(meter_marked(0.1, 12, None).len(), 3);
    }
}
