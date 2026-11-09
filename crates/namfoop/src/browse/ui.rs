//! Rendu du navigateur (ratatui). Lecture seule de l'état.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use super::state::{AbState, Editor, Field, IrChoice, IrRow, ModelRow, State, Tab, FIELDS};
use crate::recommend::NeedsIr;
use crate::tui::{meter_marked, meter_width};

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

/// Indicateur A/B permanent (en-tête) : quelle IR on entend.
fn ab_badge(s: &State) -> Vec<Span<'static>> {
    let on = |t: String, bg: Color| {
        Span::styled(t, Style::new().fg(Color::Black).bg(bg).add_modifier(Modifier::BOLD))
    };
    let off = |t: String| Span::styled(t, Style::new().fg(Color::DarkGray));
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
    }
    if let Some(ed) = &s.editor {
        render_editor(s, ed, f, body);
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
            "↑↓ champ   ←→ changer (écoute directe)   Entrée saisir   s enregistrer   a appliquer le changement aux modèles filtrés   Échap fermer"
        }
        (true, ..) => "tape le nom   Entrée enregistrer   Échap annuler",
        (_, true, _) => "tape pour filtrer   Entrée valider   ↑↓ quitter le filtre",
        (_, _, Tab::Models) => "↑↓ écouter   e modifier / calibrer   c épingler réf.   r A/B réf.   b avec/sans IR   / filtrer   Tab → IR   q",
        (_, _, Tab::Irs) => {
            "↑↓ écouter   c épingler réf.   r A/B réf.   b avec/sans IR   espace → mélange   / filtrer   Tab   q"
        }
        (_, _, Tab::Blend) => {
            "↑↓ choisir   +/- gain   p polarité   a alignement   x retirer   s sauver   b avec/sans   r A/B réf.   Tab   q"
        }
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {keys}"), Style::new().fg(Color::DarkGray)))),
        foot,
    );
}

/// Largeur minimale du corps pour afficher le panneau spectre à droite de la liste.
pub const SPECTRUM_MIN_WIDTH: u16 = 130;

fn curve(p: &dsp::profile::IrProfile) -> Vec<(f64, f64)> {
    dsp::profile::band_centers()
        .iter()
        .zip(&p.bands_db)
        .map(|(f, d)| ((*f as f64).log10(), (*d as f64).clamp(-24.0, 12.0)))
        .collect()
}

/// Panneau « Spectre » : courbe sous le curseur (ou du mélange), référence épinglée, écarts par zone.
fn render_spectrum(s: &State, f: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::symbols::Marker;
    use ratatui::widgets::{Axis, Chart, Dataset, GraphType};
    let profile_of = |i: usize| s.irs[i].profile.as_ref().map(|d| d.profile.clone());
    // (nom, profil, couleur)
    let mut shown: Vec<(String, std::sync::Arc<dsp::profile::IrProfile>, Color)> = Vec::new();
    let mprofile = |i: usize| s.models[i].profile.as_ref().map(|d| d.profile.clone());
    match s.tab {
        Tab::Models => {
            if let Some(m) = s.model_at_cursor() {
                if let Some(pr) = mprofile(m) {
                    shown.push((s.models[m].label.clone(), pr, Color::Cyan));
                }
            }
        }
        Tab::Blend => {
            if let Some(b) = &s.blend_profile {
                shown.push(("mélange".into(), b.clone(), Color::Magenta));
            }
            if let Some(p) = s.blend.get(s.cursor()) {
                if let Some(pr) = profile_of(p.ir) {
                    shown.push((s.irs[p.ir].label.clone(), pr, Color::Cyan));
                }
            }
        }
        _ => {
            if let Some(i) = s.ir_at_cursor() {
                if let Some(pr) = profile_of(i) {
                    shown.push((s.irs[i].label.clone(), pr, Color::Cyan));
                }
            }
        }
    }
    let reference = if s.tab == Tab::Models {
        s.ref_model.and_then(|r| mprofile(r).map(|p| (s.models[r].label.clone(), p)))
    } else {
        s.reference.and_then(|r| profile_of(r).map(|p| (s.irs[r].label.clone(), p)))
    };
    let [chart_area, info] = Layout::vertical([Constraint::Min(6), Constraint::Length(4)]).areas(area);
    let curves: Vec<Vec<(f64, f64)>> =
        shown.iter().map(|(_, p, _)| curve(p)).chain(reference.iter().map(|(_, p)| curve(p))).collect();
    let mut datasets = Vec::new();
    for (k, (name, _, color)) in shown.iter().enumerate() {
        datasets.push(
            Dataset::default()
                .name(name.clone())
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(*color))
                .data(&curves[k]),
        );
    }
    if let Some((name, _)) = &reference {
        datasets.push(
            Dataset::default()
                .name(format!("◆ {name}"))
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(Color::Yellow))
                .data(&curves[curves.len() - 1]),
        );
    }
    let x_labels: Vec<Span> =
        ["31", "90", "250", "700", "2k", "5,7k", "16k"].into_iter().map(Span::raw).collect();
    let chart = Chart::new(datasets)
        .block(Block::bordered().title(" Spectre (dB, IR telle qu'écoutée) "))
        .x_axis(
            Axis::default()
                .bounds([31.25f64.log10(), 16000f64.log10()])
                .labels(x_labels)
                .style(Style::new().fg(Color::DarkGray)),
        )
        .y_axis(
            Axis::default()
                .bounds([-24.0, 12.0])
                .labels(["-24", "-12", "0", "+12"].into_iter().map(Span::raw).collect::<Vec<_>>())
                .style(Style::new().fg(Color::DarkGray)),
        )
        .legend_position(Some(ratatui::widgets::LegendPosition::TopRight))
        .hidden_legend_constraints((Constraint::Percentage(70), Constraint::Percentage(70)));
    f.render_widget(chart, chart_area);

    let mut text = Vec::new();
    match (shown.last(), &reference) {
        (Some((name, p, _)), Some((rname, r))) => {
            let (a, b) = (crate::profile::zones(p), crate::profile::zones(r));
            let parts: Vec<String> = crate::profile::ZONES
                .iter()
                .enumerate()
                .map(|(k, (z, ..))| format!("{z} {:+.0}", a[k] - b[k]))
                .collect();
            text.push(Line::from(Span::styled(
                format!(" {name} vs ◆ {rname} :"),
                Style::new().fg(Color::Yellow),
            )));
            text.push(Line::raw(format!(" {}", parts.join(" · "))));
        }
        (Some(_), None) => {
            text.push(Line::from(Span::styled(
                " c : épingler la ligne sous le curseur comme référence (comparaison)",
                Style::new().fg(Color::DarkGray),
            )));
        }
        (None, _) => {
            text.push(Line::from(Span::styled(
                " pas de profil pour cette ligne",
                Style::new().fg(Color::DarkGray),
            )));
        }
    }
    if s.tab == Tab::Models {
        let grain =
            |m: usize| s.models[m].profile.as_ref().and_then(|d| d.grain.clone()).unwrap_or("?".into());
        let mut g = Vec::new();
        if let Some(m) = s.model_at_cursor() {
            g.push(Span::raw(format!(" grain : {}", grain(m))));
        }
        if let Some(r) = s.ref_model {
            g.push(Span::styled(format!("  ·  ◆ {}", grain(r)), Style::new().fg(Color::Yellow)));
        }
        text.insert(0, Line::from(g));
    }
    f.render_widget(Paragraph::new(text).wrap(ratatui::widgets::Wrap { trim: true }), info);
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
                match &s.editor_grain {
                    Some(g) => format!("grain : {g}"),
                    None => "plus = plus de saturation ; s'ajoute à --input-db".to_string(),
                },
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
        s.on_key(Key::Down); // ligne « son direct »
        s.on_key(Key::Down);
        s.on_key(Key::Enter);
        let t = screen(&s);
        assert!(t.contains("m002") && t.contains("IR requise") && t.contains("IR incluse"), "{t}");
        assert!(t.contains("◀ en cours") && t.contains("Powerhouse 410"), "{t}");
        assert!(t.contains("son direct (DI, sans modèle)"), "{t}");
    }

    #[test]
    fn ir_tab_marks_recommended_and_blend() {
        let mut s = sample();
        s.on_key(Key::Down); // ligne « son direct »
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
    fn ir_rows_show_spark_and_tags() {
        let mut s = sample();
        s.irs[0].profile = Some(crate::profile::Described {
            profile: std::sync::Arc::new(dsp::profile::IrProfile {
                bands_db: vec![0.0; 28],
                low_cut_hz: 40.0,
                high_cut_hz: 5000.0,
            }),
            spark: "▅▆▇▆▅▄▃▃▂▂▁▁".into(),
            tags: vec!["sub +4".into(), "sombre".into()],
            grain: None,
        });
        s.on_key(Key::Tab);
        let t = screen(&s);
        assert!(t.contains("▅▆▇▆▅▄▃▃▂▂▁▁  sub +4, sombre"), "{t}");
    }

    fn with_profiles(s: &mut State) {
        for (k, it) in s.irs.iter_mut().enumerate() {
            let tilt = k as f32 * 3.0;
            it.profile = Some(crate::profile::Described {
                profile: std::sync::Arc::new(dsp::profile::IrProfile {
                    bands_db: dsp::profile::band_centers()
                        .iter()
                        .map(|f| {
                            if *f > 2000.0 {
                                -6.0 - tilt
                            } else if *f < 80.0 {
                                tilt
                            } else {
                                0.0
                            }
                        })
                        .collect(),
                    low_cut_hz: 40.0,
                    high_cut_hz: 6000.0,
                }),
                spark: "▅▆▇▆▅▄▃▃▂▂▁▁".into(),
                tags: vec![],
                grain: None,
            });
        }
    }

    #[test]
    fn spectrum_panel_with_reference_on_wide_terminals() {
        let mut s = sample();
        with_profiles(&mut s);
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Enter); // 800D
        s.on_key(Key::Tab);
        s.on_key(Key::Down); // IR 2 (conseillée)
        s.on_key(Key::Down); // IR 0
        s.on_key(Key::Char('c'));
        s.on_key(Key::Up); // IR 2 sous le curseur
        let wide = crate::tui::screen_text(160, 30, |f| render(&s, &Status::default(), f));
        assert!(wide.contains("Spectre") && wide.contains("◆"), "{wide}");
        assert!(wide.contains("◆ 121 Lower - Cap") && wide.contains("Powerhouse 410"), "légende : {wide}");
        assert!(wide.contains("Powerhouse 410 vs ◆ 121 Lower - Cap"), "{wide}");
        assert!(wide.contains("sub +6") && wide.contains("air -6"), "{wide}");
        assert!(wide.contains("c épingler réf."), "{wide}");
        // terminal étroit : pas de panneau
        let narrow = crate::tui::screen_text(110, 30, |f| render(&s, &Status::default(), f));
        assert!(!narrow.contains("Spectre"), "{narrow}");
        // A/B : l'en-tête le signale
        s.on_key(Key::Char('r'));
        let t = crate::tui::screen_text(160, 30, |f| render(&s, &Status::default(), f));
        assert!(t.contains(" B ◆ 121 Lower - Cap ") && t.contains("◀ en cours (B)"), "{t}");
        s.on_key(Key::Char('b'));
        let t = crate::tui::screen_text(160, 30, |f| render(&s, &Status::default(), f));
        assert!(t.contains(" IR COUPÉE — b ") && t.contains("IR : coupée (b)"), "{t}");
    }

    #[test]
    fn input_meter_shows_calibration_mark_and_model_level() {
        let mut s = sample();
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Enter); // un modèle en écoute
        let st = Status {
            in_peak: dsp::db_to_gain(-9.0),
            in_mark: Some(-8.5),
            trim_db: -21.5,
            ..Default::default()
        };
        let t = crate::tui::screen_text(140, 24, |f| render(&s, &st, f));
        assert!(t.contains("┃") && t.contains("→ modèle -30 dB"), "{t}");
    }

    #[test]
    fn models_tab_spectrum_and_model_ab() {
        let mut s = sample();
        for (k, m) in s.models.iter_mut().enumerate() {
            m.profile = Some(crate::profile::Described {
                profile: std::sync::Arc::new(dsp::profile::IrProfile {
                    bands_db: dsp::profile::band_centers()
                        .iter()
                        .map(|f| if *f < 80.0 { k as f32 * 3.0 } else { 0.0 })
                        .collect(),
                    low_cut_hz: 40.0,
                    high_cut_hz: 6000.0,
                }),
                spark: "▅▆▇▆▅▄▃▃▂▂▁▁".into(),
                tags: vec!["grave +3".into()],
                grain: Some(if k == 0 { "saturé 18 %".into() } else { "clair · casse à +9 dB".into() }),
            });
        }
        s.on_key(Key::Down); // B7K
        s.on_key(Key::Char('c'));
        s.on_key(Key::Down); // 800D
        s.on_key(Key::Enter);
        let t = crate::tui::screen_text(170, 30, |f| render(&s, &Status::default(), f));
        assert!(
            t.contains("A/B modèles") && t.contains(" A 800D Modern ") && t.contains("B ◆ B7K Ultra"),
            "{t}"
        );
        assert!(t.contains("clair · casse à +9 dB") && t.contains("Spectre"), "{t}");
        assert!(t.contains("grain : clair · casse à +9 dB") && t.contains("◆ saturé 18 %"), "{t}");
        assert!(t.contains("800D Modern vs ◆ B7K Ultra") && t.contains("sub +3"), "{t}");
        s.on_key(Key::Char('r'));
        let t = crate::tui::screen_text(170, 30, |f| render(&s, &Status::default(), f));
        assert!(t.contains("◀ en cours (B)") && t.contains("A (r pour revenir)"), "{t}");
        // éditeur : grain mesuré affiché sur le champ Entrée
        s.on_key(Key::Char('r'));
        s.editor_grain = Some("crunch 4 %".into());
        s.editor =
            Some(super::super::state::Editor { model: 1, field: 1, draft: Default::default(), typing: None });
        let t = crate::tui::screen_text(170, 30, |f| render(&s, &Status::default(), f));
        assert!(t.contains("grain : crunch 4 %"), "{t}");
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
        s.on_key(Key::Down); // ligne « son direct »
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
