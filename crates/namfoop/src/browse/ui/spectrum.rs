//! Panneau spectre (≥ 130 colonnes) : courbe sous le curseur, référence, écarts par zone, grain.

use super::*;

/// Largeur minimale du corps pour afficher le panneau spectre à droite de la liste.
pub const SPECTRUM_MIN_WIDTH: u16 = 130;

pub(super) fn curve(p: &dsp::profile::IrProfile) -> Vec<(f64, f64)> {
    dsp::profile::band_centers()
        .iter()
        .zip(&p.bands_db)
        .map(|(f, d)| ((*f as f64).log10(), (*d as f64).clamp(-24.0, 12.0)))
        .collect()
}

/// Panneau « Spectre » : courbe sous le curseur (ou du mélange), référence épinglée, écarts par zone.
pub(super) fn render_spectrum(s: &State, f: &mut Frame, area: ratatui::layout::Rect) {
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
            let (a, b) = (library::profile::zones(p), library::profile::zones(r));
            let parts: Vec<String> = library::profile::ZONES
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
