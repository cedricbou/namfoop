//! Rendu du navigateur (ratatui). Lecture seule de l'état.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use super::state::{Editor, Field, IrChoice, IrRow, State, Tab, FIELDS};
use crate::recommend::NeedsIr;
use crate::tui::meter_width;

/// Mesures affichées en bas (fournies par la boucle).
#[derive(Default, Clone)]
pub struct Status {
    pub in_peak: f32,
    pub out_peak: f32,
    pub load: f32,
    /// Réduction de gain du limiteur (dB).
    pub limiter: f32,
    pub xruns: u64,
    pub loading: bool,
    pub device: String,
}

fn cursor_span(on: bool) -> Span<'static> {
    if on {
        Span::styled(" ▶ ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("   ")
    }
}

fn badge(needs: NeedsIr) -> Span<'static> {
    match needs {
        NeedsIr::No => Span::styled(" IR incluse ", Style::new().fg(Color::DarkGray)),
        NeedsIr::Yes => Span::styled(" IR requise ", Style::new().fg(Color::Yellow)),
        NeedsIr::Maybe => Span::styled(" IR ?      ", Style::new().fg(Color::DarkGray)),
    }
}

fn playing() -> Span<'static> {
    Span::styled("  ◀ en cours", Style::new().fg(Color::Green).add_modifier(Modifier::BOLD))
}

/// Fenêtre de `height` lignes autour du curseur.
fn window(len: usize, cursor: usize, height: usize) -> std::ops::Range<usize> {
    if len <= height {
        return 0..len;
    }
    let start = cursor.saturating_sub(height / 2).min(len - height);
    start..start + height
}

pub fn render(s: &State, st: &Status, f: &mut Frame) {
    let [tabs, filter, body, now, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .areas(f.area());

    let tab = |t: Tab, label: String| {
        let style = if s.tab == t {
            Style::new().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray)
        };
        Span::styled(format!(" {label} "), style)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" namfoop browse  ", Style::new().add_modifier(Modifier::BOLD)),
            tab(Tab::Models, format!("Modèles ({})", s.models.len())),
            Span::raw(" "),
            tab(Tab::Irs, format!("IR ({})", s.irs.len())),
            Span::raw(" "),
            tab(Tab::Blend, format!("Mélange ({})", s.blend.len())),
        ])),
        tabs,
    );

    let filter_line = if let Some(name) = &s.naming {
        Line::from(vec![
            Span::styled(" nom du mélange : ", Style::new().fg(Color::Yellow)),
            Span::raw(format!("{name}█")),
        ])
    } else if s.filtering || !s.filter.is_empty() {
        Line::from(vec![
            Span::styled(" filtre : ", Style::new().fg(Color::Cyan)),
            Span::raw(format!("{}{}", s.filter, if s.filtering { "█" } else { "" })),
        ])
    } else {
        Line::from(Span::styled(" / pour filtrer", Style::new().fg(Color::DarkGray)))
    };
    f.render_widget(Paragraph::new(filter_line), filter);

    let height = body.height.saturating_sub(2) as usize;
    let cur = s.cursor();
    let mut lines: Vec<Line> = Vec::new();
    match s.tab {
        Tab::Models => {
            let rows = s.model_rows();
            for i in window(rows.len(), cur, height) {
                let m = &s.models[rows[i]];
                let mut spans = vec![
                    cursor_span(i == cur),
                    Span::styled(format!("{:<6}", m.id), Style::new().fg(Color::DarkGray)),
                    badge(m.needs),
                    if m.corrected() {
                        Span::styled("✎", Style::new().fg(Color::Magenta))
                    } else {
                        Span::raw(" ")
                    },
                    Span::raw(m.label.to_string()),
                    Span::styled(format!("  {}  [{}]", m.detail, m.pack), Style::new().fg(Color::DarkGray)),
                ];
                if s.model == Some(rows[i]) {
                    spans.push(playing());
                }
                lines.push(Line::from(spans));
            }
        }
        Tab::Irs => {
            let rows = s.ir_rows();
            for i in window(rows.len(), cur, height) {
                let mut spans = vec![cursor_span(i == cur)];
                match rows[i] {
                    IrRow::NoIr => {
                        spans.push(Span::raw("       aucune IR"));
                        if s.ir == IrChoice::None {
                            spans.push(playing());
                        }
                    }
                    IrRow::Ir { index, recommended } => {
                        let it = &s.irs[index];
                        let in_blend = s.blend.iter().find(|p| p.ir == index);
                        spans.push(Span::styled(format!("{:<6}", it.id), Style::new().fg(Color::DarkGray)));
                        spans.push(if recommended {
                            Span::styled(" ★ ", Style::new().fg(Color::Yellow))
                        } else if in_blend.is_some() {
                            Span::styled(" ● ", Style::new().fg(Color::Magenta))
                        } else {
                            Span::raw("   ")
                        });
                        spans.push(Span::raw(it.label.clone()));
                        spans
                            .push(Span::styled(format!("  [{}]", it.pack), Style::new().fg(Color::DarkGray)));
                        if let Some(p) = in_blend {
                            spans.push(Span::styled(
                                format!(
                                    "  [mélange {:+.0} dB{}]",
                                    p.gain_db,
                                    if p.invert { " ø" } else { "" }
                                ),
                                Style::new().fg(Color::Magenta),
                            ));
                        }
                        if s.ir == IrChoice::Item(index) {
                            spans.push(playing());
                        }
                    }
                }
                lines.push(Line::from(spans));
            }
        }
        Tab::Blend => {
            if s.blend.is_empty() {
                lines.push(Line::raw("   Mélange vide. Onglet IR : espace sur une IR pour l'ajouter."));
            }
            for (i, p) in s.blend.iter().enumerate() {
                let it = &s.irs[p.ir];
                lines.push(Line::from(vec![
                    cursor_span(i == cur),
                    Span::styled(format!("{:+5.0} dB ", p.gain_db), Style::new().fg(Color::Magenta)),
                    Span::raw(if p.invert { "ø " } else { "  " }),
                    Span::raw(it.label.clone()),
                    Span::styled(format!("  [{}]", it.pack), Style::new().fg(Color::DarkGray)),
                ]));
            }
            lines.push(Line::raw(""));
            lines.push(Line::raw(format!(
                "   alignement automatique : {}",
                if s.align { "oui" } else { "non" }
            )));
        }
    }
    if let Some(ed) = &s.editor {
        render_editor(s, ed, f, body);
    } else {
        f.render_widget(Paragraph::new(lines).block(Block::bordered()), body);
    }

    let model = s
        .model
        .map_or("aucun (son direct)".to_string(), |m| format!("{} {}", s.models[m].id, s.models[m].label));
    let ir = match s.ir {
        IrChoice::None => "aucune".to_string(),
        IrChoice::Item(i) => format!("{} {}", s.irs[i].id, s.irs[i].label),
        IrChoice::Blend => format!("mélange de {} IR", s.blend.len()),
    };
    let mut meters = vec![Span::raw(" in ")];
    meters.extend(meter_width(st.in_peak, 12));
    meters.push(Span::raw("   out "));
    meters.extend(meter_width(st.out_peak, 12));
    meters.push(Span::raw(if st.limiter > 0.05 {
        format!("   lim -{:.1} dB", st.limiter)
    } else {
        "   lim    -   ".to_string()
    }));
    meters.push(Span::raw(format!("   dsp {:>4.0} %  xruns {}", st.load * 100.0, st.xruns)));
    if st.loading {
        meters.push(Span::styled("  chargement…", Style::new().fg(Color::Yellow)));
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(" ♪ ", Style::new().fg(Color::Green)),
                Span::raw(format!("{model}  +  IR : {ir}")),
            ]),
            Line::from(meters),
            Line::from(Span::styled(
                s.message.clone().unwrap_or_else(|| format!(" {}", st.device)),
                Style::new().fg(Color::Yellow),
            )),
        ]),
        now,
    );

    let keys = match (s.naming.is_some(), s.filtering, s.tab) {
        _ if s.editor.as_ref().is_some_and(|e| e.typing.is_some()) => {
            "tape le texte   Entrée valider   Échap annuler"
        }
        _ if s.editor.is_some() => {
            "↑↓ champ   ←→ changer (écoute directe)   Entrée saisir   s enregistrer   a appliquer aux modèles filtrés   Échap fermer"
        }
        (true, ..) => "tape le nom   Entrée enregistrer   Échap annuler",
        (_, true, _) => "tape pour filtrer   Entrée valider   ↑↓ quitter le filtre",
        (_, _, Tab::Models) => "↑↓ écouter   e modifier / calibrer   / filtrer   Tab → IR   q quitter",
        (_, _, Tab::Irs) => "↑↓ écouter   espace ajouter au mélange   / filtrer   Tab → mélange   q quitter",
        (_, _, Tab::Blend) => {
            "↑↓ choisir   +/- gain   p polarité   a alignement   x retirer   s sauver   Tab   q"
        }
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {keys}"), Style::new().fg(Color::DarkGray)))),
        foot,
    );
}

fn type_help(t: &str) -> &'static str {
    match crate::recommend::needs_ir(t) {
        crate::recommend::NeedsIr::No => "baffle inclus : pas d'IR",
        crate::recommend::NeedsIr::Yes => "sans baffle : IR requise",
        crate::recommend::NeedsIr::Maybe => "inconnu",
    }
}

/// Panneau d'édition : métadonnées du modèle en écoute, valeur du fichier en regard.
fn render_editor(s: &State, ed: &Editor, f: &mut Frame, area: ratatui::layout::Rect) {
    let item = &s.models[ed.model];
    let file = item.meta.as_ref().map(|b| b.1.clone()).unwrap_or_default();
    let d = &ed.draft;
    let or_q = |v: &str| if v.is_empty() { "?".to_string() } else { v.to_string() };
    let ir_name = |i: Option<usize>| {
        i.map_or("aucune (conseil automatique)".to_string(), |i| {
            format!("{} {}", s.irs[i].id, s.irs[i].label)
        })
    };
    let mut lines = vec![Line::raw("")];
    for (k, field) in FIELDS.iter().enumerate() {
        let on = k == ed.field;
        let typing = |v: &str| match (&ed.typing, on) {
            (Some(t), true) => format!("{t}█"),
            _ => or_q(v),
        };
        let (label, value, hint) = match field {
            Field::Type => (
                "Type de capture",
                format!("◀ {} ▶", or_q(&d.gear_type)),
                format!("{}   (fichier : {})", type_help(&d.gear_type), or_q(&file.gear_type)),
            ),
            Field::Input => (
                "Entrée",
                format!("◀ {:+.0} dB ▶", d.input_db),
                "plus = plus de saturation ; s'ajoute à --input-db".to_string(),
            ),
            Field::Ir => (
                "IR préférée",
                format!("◀ {} ▶", ir_name(d.ir)),
                "Entrée : prendre l'IR en écoute ; x : aucune".to_string(),
            ),
            Field::Make => ("Marque", typing(&d.make), format!("(fichier : {})", or_q(&file.make))),
            Field::Name => ("Nom", typing(&d.name), format!("(fichier : {})", or_q(&file.name))),
        };
        let style = if on { Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::new() };
        lines.push(Line::from(vec![
            cursor_span(on),
            Span::styled(format!("{label:<16}"), style),
            Span::styled(format!("{value:<40}"), style),
            Span::styled(format!(" {hint}"), Style::new().fg(Color::DarkGray)),
        ]));
    }
    lines.push(Line::raw(""));
    for t in [
        "Joue en réglant l'entrée jusqu'au grain voulu (vumètre out et limiteur en bas).",
        "Baffle inclus ou non ? Compare l'IR « aucune » et une IR : avec un baffle en double, le son",
        "devient sourd et étouffé ; sans baffle ni IR, il est criard et bourdonnant.",
    ] {
        lines.push(Line::from(Span::styled(format!("   {t}"), Style::new().fg(Color::DarkGray))));
    }
    let title = format!(" Modifier {} {}  [{}] ", item.id, item.label, item.pack);
    f.render_widget(Paragraph::new(lines).block(Block::bordered().title(title)), area);
}

#[cfg(test)]
mod tests {
    use super::super::state::tests::sample;
    use super::*;
    use crate::tui::Key;

    fn screen(s: &State) -> String {
        crate::tui::screen_text(120, 24, |f| render(s, &Status::default(), f))
    }

    #[test]
    fn models_tab_shows_ids_badges_and_current() {
        let mut s = sample();
        s.on_key(Key::Down);
        s.on_key(Key::Enter);
        let t = screen(&s);
        assert!(t.contains("m002") && t.contains("IR requise") && t.contains("IR incluse"), "{t}");
        assert!(t.contains("◀ en cours") && t.contains("Powerhouse 410"), "{t}");
    }

    #[test]
    fn ir_tab_marks_recommended_and_blend() {
        let mut s = sample();
        s.on_key(Key::Down);
        s.on_key(Key::Enter);
        s.on_key(Key::Tab);
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Char(' '));
        let t = screen(&s);
        assert!(t.contains("aucune IR") && t.contains("★"), "{t}");
        assert!(t.contains("[mélange +0 dB]"), "{t}");
        s.on_key(Key::Tab);
        let t = screen(&s);
        assert!(t.contains("alignement automatique : oui") && t.contains("p polarité"), "{t}");
    }

    #[test]
    fn editor_panel_shows_fields_and_file_values() {
        let mut s = sample();
        for m in s.models.iter_mut() {
            let file = super::super::state::ModelMeta { gear_type: "amp".into(), ..Default::default() };
            let mut eff = file.clone();
            eff.gear_type = "amp_cab".into();
            m.meta = Some(Box::new((eff, file)));
        }
        s.on_key(Key::Char('e'));
        let t = screen(&s);
        assert!(t.contains("Modifier m001") && t.contains("◀ amp_cab ▶"), "{t}");
        assert!(t.contains("baffle inclus : pas d'IR") && t.contains("(fichier : amp)"), "{t}");
        assert!(t.contains("s enregistrer"), "{t}");
        s.on_key(Key::Esc);
        assert!(screen(&s).contains("✎"), "modèle corrigé marqué");
    }

    #[test]
    fn long_lists_scroll_with_the_cursor() {
        assert_eq!(window(100, 0, 10), 0..10);
        assert_eq!(window(100, 50, 10), 45..55);
        assert_eq!(window(100, 99, 10), 90..100);
        assert_eq!(window(5, 3, 10), 0..5);
    }
}
