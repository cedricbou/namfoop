//! Rendu du navigateur (ratatui). Lecture seule de l'état.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use super::state::{AbState, Editor, Field, IrChoice, IrRow, ModelRow, State, Tab, FIELDS};
use crate::tui::{meter_marked, meter_width};
use library::recommend::NeedsIr;

mod editor;
mod fx;
mod spectrum;
pub mod synoptic;

use editor::*;
pub use spectrum::SPECTRUM_MIN_WIDTH;
use spectrum::*;

/// Mesures affichées en bas (fournies par la boucle).
#[derive(Default, Clone)]
pub struct Status {
    pub in_peak: f32,
    pub out_peak: f32,
    pub load: f32,
    /// Crête attendue à l'entrée de la carte d'après la calibration (repère sur `in`).
    pub in_mark: Option<f32>,
    /// Trim de calibration (dB) : sert à afficher ce que reçoit le modèle.
    pub trim_db: f32,
    /// Réduction de gain du limiteur (dB).
    pub limiter: f32,
    pub xruns: u64,
    pub loading: bool,
    pub device: String,
}

pub(super) fn cursor_span(on: bool) -> Span<'static> {
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

/// Indicateur A/B permanent (en-tête) : quelle IR on entend ; dans l'onglet Effets, avec ou sans
/// effets (`r`).
fn ab_badge(s: &State) -> Vec<Span<'static>> {
    let on = |t: String, bg: Color| {
        Span::styled(t, Style::new().fg(Color::Black).bg(bg).add_modifier(Modifier::BOLD))
    };
    let off = |t: String| Span::styled(t, Style::new().fg(Color::DarkGray));
    if s.tab == Tab::Fx {
        let mut v = vec![Span::styled("A/B ", Style::new().add_modifier(Modifier::BOLD))];
        if s.fx.bypass_all {
            v.extend([off("A avec effets  ".into()), on(" B sans effets ".into(), Color::Yellow)]);
        } else {
            v.extend([on(" A avec effets ".into(), Color::Cyan), off("  B sans effets".into())]);
        }
        v.push(off("  (r)".into()));
        return v;
    }
    let name = |i: usize| s.irs[i].label.chars().take(24).collect::<String>();
    let mname = |i: usize| s.models[i].label.chars().take(28).collect::<String>();
    if let (Some(rm), false) = (s.ref_model, s.bypass) {
        let mut v = vec![Span::styled("A/B modèles ", Style::new().add_modifier(Modifier::BOLD))];
        let a = s.model.map_or("son direct".to_string(), mname);
        let b = format!("◆ {}", mname(rm));
        match s.ab_state() {
            AbState::Same => v.push(Span::styled(
                format!("{b} = modèle en cours : choisis-en un autre"),
                Style::new().fg(Color::Yellow),
            )),
            AbState::B => v.extend([
                off(format!("A {a}  ")),
                on(format!(" B {b} "), Color::Yellow),
                off("  (r)".into()),
            ]),
            _ => v.extend([on(format!(" A {a} "), Color::Cyan), off(format!("  B {b}  (r)"))]),
        }
        return v;
    }
    let mut v = vec![Span::styled("A/B ", Style::new().add_modifier(Modifier::BOLD))];
    match s.ab_state() {
        AbState::NoReference => {
            v.push(off("c : épingler un modèle ou une IR de référence · b : avec/sans IR".into()))
        }
        AbState::Same => v.push(Span::styled(
            format!("◆ {} = IR en cours : choisis-en une autre", name(s.reference.expect("épinglée"))),
            Style::new().fg(Color::Yellow),
        )),
        AbState::Bypass => v.push(on(" IR COUPÉE — b ".into(), Color::Red)),
        st => {
            let a = match s.ir {
                IrChoice::Item(i) => name(i),
                IrChoice::Blend => "mélange".into(),
                IrChoice::None => "aucune IR".into(),
            };
            let b = format!("◆ {}", name(s.reference.expect("épinglée")));
            if st == AbState::A {
                v.extend([on(format!(" A {a} "), Color::Cyan), off(format!("  B {b}  (r)"))]);
            } else {
                v.extend([
                    off(format!("A {a}  ")),
                    on(format!(" B {b} "), Color::Yellow),
                    off("  (r)".into()),
                ]);
            }
        }
    }
    v
}

/// Couleur du grain : clair vert, crunch jaune, saturé rouge.
fn grain_style(g: &str) -> Style {
    let c = if g.starts_with("clair") {
        Color::Green
    } else if g.starts_with("crunch") {
        Color::Yellow
    } else {
        Color::Red
    };
    Style::new().fg(c)
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
        Paragraph::new(Line::from(
            vec![
                Span::styled(" namfoop browse  ", Style::new().add_modifier(Modifier::BOLD)),
                tab(Tab::Models, format!("Modèles ({})", s.models.len())),
                Span::raw(" "),
                tab(Tab::Irs, format!("IR ({})", s.irs.len())),
                Span::raw(" "),
                tab(Tab::Blend, format!("Mélange ({})", s.blend.len())),
                Span::raw(" "),
                tab(
                    Tab::Fx,
                    match s.fx.len() {
                        0 => format!("Effets ({})", s.fx.catalog.len()),
                        n => format!("Effets ({}) ● {n}", s.fx.catalog.len()),
                    },
                ),
                Span::raw("    "),
            ]
            .into_iter()
            .chain(ab_badge(s))
            .collect::<Vec<_>>(),
        )),
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
            let rows = s.model_list();
            let label_w = rows
                .iter()
                .filter_map(|r| match r {
                    ModelRow::Model(m) => Some(s.models[*m].label.chars().count()),
                    ModelRow::Direct => None,
                })
                .max()
                .unwrap_or(0)
                .min(36);
            for i in window(rows.len(), cur, height) {
                let idx = match rows[i] {
                    ModelRow::Direct => {
                        let mut spans = vec![
                            cursor_span(i == cur),
                            Span::raw("       son direct (DI, sans modèle)"),
                            Span::styled("  pour écouter une IR seule", Style::new().fg(Color::DarkGray)),
                        ];
                        if s.model.is_none() {
                            spans.push(playing());
                        }
                        lines.push(Line::from(spans));
                        continue;
                    }
                    ModelRow::Model(m) => m,
                };
                let m = &s.models[idx];
                let mut spans = vec![
                    cursor_span(i == cur),
                    Span::styled(format!("{:<6}", m.id), Style::new().fg(Color::DarkGray)),
                    badge(m.needs),
                    if s.ref_model == Some(idx) {
                        Span::styled("◆", Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    } else if m.corrected() {
                        Span::styled("✎", Style::new().fg(Color::Magenta))
                    } else {
                        Span::raw(" ")
                    },
                    Span::raw(format!("{:<w$}", m.label, w = label_w)),
                ];
                let (chosen, refd) = (s.model == Some(idx), s.ref_model == Some(idx));
                match (s.ref_model.is_some(), s.listen_ref) {
                    (true, true) if refd => {
                        spans.push(playing());
                        spans.push(Span::styled(" (B)", Style::new().fg(Color::Yellow)));
                    }
                    (true, true) if chosen => {
                        spans.push(Span::styled("  A (r pour revenir)", Style::new().fg(Color::DarkGray)))
                    }
                    (true, false) if chosen && !refd => {
                        spans.push(playing());
                        spans.push(Span::styled(" (A)", Style::new().fg(Color::Cyan)));
                    }
                    _ if chosen => spans.push(playing()),
                    _ => {}
                }
                if let Some(d) = &m.profile {
                    spans.push(Span::styled(format!("  {}", d.spark), Style::new().fg(Color::Cyan)));
                    if let Some(g) = &d.grain {
                        spans.push(Span::styled(format!("  {g}"), grain_style(g)));
                    }
                    spans.push(Span::raw(format!("  {}", d.tags.join(", "))));
                }
                spans.push(Span::styled(
                    format!("  {}  [{}]", m.detail, m.pack),
                    Style::new().fg(Color::DarkGray),
                ));
                lines.push(Line::from(spans));
            }
        }
        Tab::Irs => {
            let rows = s.ir_rows();
            // noms alignés pour que les mini-spectres forment une colonne
            let label_w = rows
                .iter()
                .filter_map(|r| match r {
                    IrRow::Ir { index, .. } => Some(s.irs[*index].label.chars().count()),
                    IrRow::NoIr => None,
                })
                .max()
                .unwrap_or(0)
                .min(40);
            for i in window(rows.len(), cur, height) {
                let mut spans = vec![cursor_span(i == cur)];
                match rows[i] {
                    IrRow::NoIr => {
                        spans.push(Span::raw("       aucune IR"));
                        if s.effective_ir() == IrChoice::None && s.ir == IrChoice::None {
                            spans.push(playing());
                        }
                    }
                    IrRow::Ir { index, recommended } => {
                        let it = &s.irs[index];
                        let in_blend = s.blend.iter().find(|p| p.ir == index);
                        spans.push(Span::styled(format!("{:<6}", it.id), Style::new().fg(Color::DarkGray)));
                        spans.push(if s.reference == Some(index) {
                            Span::styled(" ◆ ", Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                        } else if recommended {
                            Span::styled(" ★ ", Style::new().fg(Color::Yellow))
                        } else if in_blend.is_some() {
                            Span::styled(" ● ", Style::new().fg(Color::Magenta))
                        } else {
                            Span::raw("   ")
                        });
                        spans.push(Span::raw(format!("{:<w$}", it.label, w = label_w)));
                        let (chosen, refd) = (s.ir == IrChoice::Item(index), s.reference == Some(index));
                        // marques A/B propres aux IR (pas pendant un A/B de modèles)
                        let ab = if s.ref_model.is_some() && !s.bypass {
                            AbState::NoReference
                        } else {
                            s.ab_state()
                        };
                        match ab {
                            AbState::Bypass if chosen => spans.push(Span::styled(
                                "  ◀ choisie, IR coupée (b)",
                                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
                            )),
                            AbState::B if refd => {
                                spans.push(playing());
                                spans.push(Span::styled(" (B)", Style::new().fg(Color::Yellow)));
                            }
                            AbState::B if chosen => spans
                                .push(Span::styled("  A (r pour revenir)", Style::new().fg(Color::DarkGray))),
                            AbState::A if chosen => {
                                spans.push(playing());
                                spans.push(Span::styled(" (A)", Style::new().fg(Color::Cyan)));
                            }
                            _ if chosen => spans.push(playing()),
                            _ => {}
                        }
                        if let Some(d) = &it.profile {
                            spans.push(Span::styled(format!("  {}", d.spark), Style::new().fg(Color::Cyan)));
                            spans.push(Span::raw(format!("  {}", d.tags.join(", "))));
                        }
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
        Tab::Fx => {}
    }
    if let Some(ed) = &s.editor {
        render_editor(s, ed, f, body);
    } else if s.tab == Tab::Fx {
        fx::render_fx(s, f, body);
    } else if body.width >= SPECTRUM_MIN_WIDTH {
        let [list, spectrum] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(body);
        f.render_widget(Paragraph::new(lines).block(Block::bordered()), list);
        render_spectrum(s, f, spectrum);
    } else {
        f.render_widget(Paragraph::new(lines).block(Block::bordered()), body);
    }

    let model = s
        .model
        .map_or("son direct (DI)".to_string(), |m| format!("{} {}", s.models[m].id, s.models[m].label));
    let ir = match s.effective_ir() {
        IrChoice::None if s.bypass => "coupée (b)".to_string(),
        IrChoice::None => "aucune".to_string(),
        IrChoice::Item(i) => format!("{} {}", s.irs[i].id, s.irs[i].label),
        IrChoice::Blend => format!("mélange de {} IR", s.blend.len()),
    };
    let mut meters = vec![Span::raw(" in ")];
    meters.extend(meter_marked(st.in_peak, 12, st.in_mark));
    if let Some(m) = s.model {
        // ce que reçoit le modèle : carte + trim + entrée propre (+ essai dans l'éditeur)
        let model_db = s.saved_meta(m).input_db + s.input_offset();
        let text = if st.in_peak > 1e-6 {
            format!(" → modèle {:.0} dB", dsp::gain_to_db(st.in_peak) + st.trim_db + model_db)
        } else {
            " → modèle  -inf".to_string()
        };
        meters.push(Span::styled(text, Style::new().fg(Color::DarkGray)));
    }
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
                Span::styled(
                    match (s.fx.len(), s.fx.bypass_all) {
                        (0, _) => String::new(),
                        (_, true) => "  +  effets coupés (r)".to_string(),
                        (n, false) => format!("  +  {n} effet(s)"),
                    },
                    Style::new().fg(Color::Magenta),
                ),
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
        _ if s.confirm_quit => "Q quitter sans enregistrer   Échap ou toute autre touche : rester",
        _ if s.editor.as_ref().is_some_and(|e| e.typing.is_some()) => {
            "tape le texte   Entrée valider   Échap annuler"
        }
        _ if s.editor.is_some() => {
            "↑↓ champ   ←→ changer (écoute directe)   Entrée saisir   s enregistrer   a appliquer le changement aux modèles filtrés   Échap fermer"
        }
        (true, ..) => "tape le nom   Entrée enregistrer   Échap annuler",
        (_, true, _) => "tape pour filtrer   Entrée valider   ↑↓ quitter le filtre",
        (_, _, Tab::Models) => "Tab/→ onglets IR, Mélange, Effets   ↑↓ écouter   e modifier / calibrer   c épingler réf.   r A/B   b avec/sans IR   / filtrer   q",
        (_, _, Tab::Irs) => {
            "Tab/←→ onglets   ↑↓ écouter   c épingler réf.   r A/B   b avec/sans IR   espace → mélange   / filtrer   q"
        }
        (_, _, Tab::Fx) => fx::fx_keys(s),
        (_, _, Tab::Blend) => {
            "Tab/→ Effets   ↑↓ choisir   +/- gain   p polarité   a alignement   x retirer   s sauver   b avec/sans IR   r A/B   q"
        }
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {keys}"), Style::new().fg(Color::DarkGray)))),
        foot,
    );

    if s.confirm_quit {
        render_confirm_quit(s, body, f);
    }
}

/// Fenêtre « Modifications non enregistrées » (`q`) : dit ce qui serait perdu, et n'offre qu'un
/// geste volontaire pour quitter (`Q` majuscule). Elle se dessine en dernier, par-dessus l'onglet.
fn render_confirm_quit(s: &State, body: Rect, f: &mut Frame) {
    let mut lines = vec![Line::from(Span::styled(
        "Ceci n'est pas enregistré :",
        Style::new().add_modifier(Modifier::BOLD),
    ))];
    for what in s.unsaved() {
        lines.push(Line::from(Span::styled(format!("  • {what}"), Style::new().fg(Color::Yellow))));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("  Q", Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::raw("     quitter sans enregistrer"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Échap", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" ou toute autre touche : rester (par défaut)"),
    ]));

    let w = lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16 + 4;
    let w = w.min(body.width);
    let h = (lines.len() as u16 + 2).min(body.height);
    let area = Rect {
        x: body.x + (body.width.saturating_sub(w)) / 2,
        y: body.y + (body.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(Span::styled(
                    " Modifications non enregistrées ",
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
                .style(Style::new().bg(Color::Black)),
        ),
        area,
    );
}

#[cfg(test)]
mod tests;
