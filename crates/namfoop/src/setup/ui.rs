//! Rendu plein écran de l'assistant (ratatui). Lecture seule de l'état.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use super::state::{BufferTest, State, Step, BACKENDS};

const BAR: usize = 24;

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Barre de niveau -60..0 dBFS, colorée vert / jaune (> -12) / rouge (> -3).
pub fn meter(level: f32) -> Vec<Span<'static>> {
    let d = db(level);
    let filled = (((d + 60.0) / 60.0).clamp(0.0, 1.0) * BAR as f32).round() as usize;
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
        Span::styled("░".repeat(BAR - filled), Style::new().fg(Color::DarkGray)),
        Span::raw(format!(" {text} dB")),
    ]
}

fn cursor(on: bool) -> Span<'static> {
    if on {
        Span::styled(" ▶ ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("   ")
    }
}

fn tag(text: &str, color: Color) -> Span<'static> {
    Span::styled(format!("  ◀ {text}"), Style::new().fg(color).add_modifier(Modifier::BOLD))
}

fn help(text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::new().fg(Color::DarkGray)))
}

pub fn render(s: &State, f: &mut Frame) {
    let [top, body, bottom] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(6), Constraint::Length(3)]).areas(f.area());
    let n = Step::ALL.len();
    let title = format!(" namfoop setup — étape {}/{n} : {} ", s.step.index() + 1, s.step.title());
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(title, Style::new().add_modifier(Modifier::BOLD)))),
        top,
    );

    let mut lines: Vec<Line> = Vec::new();
    let keys = match s.step {
        Step::Backend => {
            lines.push(Line::raw("Comment namfoop doit-il accéder à la carte son ?"));
            lines.push(Line::raw(""));
            for (i, (_, name, desc)) in BACKENDS.iter().enumerate() {
                lines.push(Line::from(vec![
                    cursor(i == s.backend),
                    Span::styled(*name, Style::new().bold_if(i == s.backend)),
                ]));
                lines.push(Line::from(Span::styled(format!("      {desc}"), Style::new().fg(Color::Gray))));
            }
            "↑↓ choisir   Entrée valider   Échap quitter"
        }
        Step::Card => {
            lines.push(Line::raw("Quelle carte son ?"));
            lines.push(Line::raw(""));
            if s.devices.is_empty() {
                lines.push(Line::raw("   (aucune carte trouvée — r pour relister)"));
            }
            for (i, d) in s.devices.iter().enumerate() {
                let chans = match (d.n_in, d.n_out) {
                    (Some(a), Some(b)) => format!("{a} entrée(s) / {b} sortie(s)"),
                    _ => "canaux connus à l'ouverture".into(),
                };
                let style = if d.usable() {
                    Style::new().bold_if(i == s.card)
                } else {
                    Style::new().fg(Color::DarkGray)
                };
                let mut spans = vec![
                    cursor(i == s.card),
                    Span::styled(d.label.clone(), style),
                    Span::raw(format!("  — {chans}")),
                ];
                if let Some(n) = &d.note {
                    spans.push(Span::styled(format!("  ({n})"), Style::new().fg(Color::Yellow)));
                }
                lines.push(Line::from(spans));
            }
            "↑↓ choisir   Entrée valider   r relister   ← retour   Échap quitter"
        }
        Step::Inputs => {
            lines.push(Line::raw(
                "Joue de la basse : l'entrée qui bouge est la bonne. Parle dans le micro pour le repérer.",
            ));
            lines.push(Line::raw(""));
            let loud = s.loudest_input();
            for c in 0..s.n_in {
                let mut spans = vec![cursor(c == s.in_cursor), Span::raw(format!("Entrée {:<3}", c + 1))];
                spans.extend(meter(s.levels.get(c).copied().unwrap_or(0.0)));
                if s.bass_in == Some(c) {
                    spans.push(tag("BASSE", Color::Cyan));
                } else if s.mic_in == Some(c) {
                    spans.push(tag("MICRO", Color::Magenta));
                } else if loud == Some(c) {
                    spans.push(tag("signal", Color::Green));
                }
                lines.push(Line::from(spans));
            }
            "↑↓ choisir   Entrée = basse   m = micro   x = pas de micro   → suivant   ← retour"
        }
        Step::Outputs => {
            lines.push(Line::raw(
                "Rôle de chaque sortie. t envoie un bip court (-20 dBFS) sur la sortie choisie seulement.",
            ));
            lines.push(Line::from(Span::styled(
                "⚠ baisse le volume de l'ampli / du casque avant le premier bip.",
                Style::new().fg(Color::Yellow),
            )));
            lines.push(Line::raw(""));
            for c in 0..s.n_out {
                let role = s.roles.get(c).copied();
                lines.push(Line::from(vec![
                    cursor(c == s.out_cursor),
                    Span::raw(format!("Sortie {:<3} ", c + 1)),
                    Span::styled(
                        role.map_or("?", |r| r.label()).to_string(),
                        Style::new().fg(if role == Some(super::state::OutRole::Nothing) {
                            Color::DarkGray
                        } else {
                            Color::Cyan
                        }),
                    ),
                ]));
            }
            "↑↓ choisir   ←→/espace rôle   t bip   Entrée suivant   b retour   Échap quitter"
        }
        Step::Buffer => {
            lines.push(Line::raw(
                "Test de stabilité : 5 s par taille, sans son. Le plus petit buffer sans xrun est retenu.",
            ));
            lines.push(Line::raw(""));
            for (i, b) in s.buffers.iter().enumerate() {
                let ms = b.frames as f64 * 1000.0 / 48000.0;
                let (txt, color) = match &b.test {
                    BufferTest::Pending => ("en attente".to_string(), Color::DarkGray),
                    BufferTest::Running => ("test en cours…".to_string(), Color::Yellow),
                    BufferTest::Done { xruns: 0, .. } => ("stable, 0 xrun ✓".to_string(), Color::Green),
                    BufferTest::Done { xruns, .. } => (format!("{xruns} xrun(s) ✗"), Color::Red),
                    BufferTest::Failed(e) => (format!("échec : {e}"), Color::Red),
                };
                lines.push(Line::from(vec![
                    cursor(i == s.buf_cursor && !s.buffer_running()),
                    Span::raw(format!("{:>4} frames ({ms:.2} ms)   ", b.frames)),
                    Span::styled(txt, Style::new().fg(color)),
                ]));
            }
            if s.buffer_running() {
                "test en cours, patiente…"
            } else {
                "↑↓ choisir   Entrée valider   r refaire le test   ← retour   Échap quitter"
            }
        }
        Step::Summary => {
            lines.push(Line::raw("Configuration à enregistrer dans ~/.config/namfoop/config.toml :"));
            lines.push(Line::raw(""));
            let d = s.device().map_or("?".to_string(), |d| format!("{} ({})", d.label, d.id));
            lines.push(Line::raw(format!("   backend   {}", BACKENDS[s.backend].1)));
            lines.push(Line::raw(format!("   carte     {d}")));
            lines.push(Line::raw(format!(
                "   buffer    {} frames",
                s.chosen_buffer().map_or("?".to_string(), |b| b.to_string())
            )));
            lines.push(Line::raw(format!("   basse     entrée {}", s.bass_in.map_or(0, |b| b + 1))));
            lines.push(Line::raw(format!(
                "   micro     {}",
                s.mic_in.map_or("aucun".to_string(), |m| format!("entrée {}", m + 1))
            )));
            for (c, r) in s.roles.iter().enumerate() {
                lines.push(Line::raw(format!("   sortie {:<2} {}", c + 1, r.label())));
            }
            lines.push(Line::raw(""));
            lines
                .push(Line::raw("Ensuite : `namfoop calibrate` (une fois), puis `namfoop run -m <modèle>`."));
            "Entrée enregistrer   ← retour   Échap quitter sans enregistrer"
        }
    };
    f.render_widget(Paragraph::new(lines).block(Block::bordered()).wrap(Wrap { trim: false }), body);

    let mut foot = vec![help(keys)];
    if let Some(m) = &s.message {
        foot.insert(0, Line::from(Span::styled(m.clone(), Style::new().fg(Color::Yellow))));
    }
    f.render_widget(Paragraph::new(foot), bottom);
}

trait BoldIf {
    fn bold_if(self, on: bool) -> Self;
}

impl BoldIf for Style {
    fn bold_if(self, on: bool) -> Self {
        if on {
            self.add_modifier(Modifier::BOLD).fg(Color::Cyan)
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::{DeviceInfo, Key};
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn screen(s: &State) -> String {
        let mut t = Terminal::new(TestBackend::new(100, 24)).unwrap();
        t.draw(|f| render(s, f)).unwrap();
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

    #[test]
    fn inputs_screen_shows_meters_and_tags() {
        let mut s = State::new(None);
        s.step = Step::Inputs;
        s.set_channels(3, 2);
        s.levels = vec![0.25, 0.0, 0.1];
        s.on_key(Key::Down);
        s.on_key(Key::Char('m'));
        let txt = screen(&s);
        assert!(txt.contains("étape 3/6 : entrées"), "{txt}");
        assert!(txt.contains("◀ BASSE") && txt.contains("◀ MICRO"), "{txt}");
        assert!(txt.contains("-12.0 dB") && txt.contains("-inf"), "{txt}");
        assert!(txt.contains("◀ signal"), "entrée 3 active non affectée : {txt}");
    }

    #[test]
    fn outputs_and_summary_screens() {
        let mut s = State::new(None);
        s.devices = vec![DeviceInfo {
            id: "hw:iTwo".into(),
            label: "PreSonus AudioBox iTwo".into(),
            n_in: Some(2),
            n_out: Some(2),
            note: None,
        }];
        s.step = Step::Outputs;
        s.set_channels(2, 2);
        let txt = screen(&s);
        assert!(txt.contains("sondier (basse traitée)") && txt.contains("in-ear"), "{txt}");
        assert!(txt.contains("baisse le volume"), "{txt}");
        s.step = Step::Summary;
        let txt = screen(&s);
        assert!(
            txt.contains("PreSonus AudioBox iTwo (hw:iTwo)") && txt.contains("basse     entrée 1"),
            "{txt}"
        );
    }

    #[test]
    fn meter_colors_and_width() {
        let spans = meter(1.0);
        assert_eq!(spans[0].content.chars().count(), BAR);
        assert_eq!(spans[0].style.fg, Some(Color::Red));
        assert_eq!(meter(0.01)[0].style.fg, Some(Color::Green));
    }
}
