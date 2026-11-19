//! Panneau spectre (≥ [`SPECTRUM_MIN_WIDTH`] colonnes, 128 : la police 10×20 de l'écran du Pi) : la courbe de la ligne sous le curseur,
//! une **référence** superposée, les écarts par zone et le grain.
//!
//! Il sert à deux écrans (§ 5 de `docs/design/browse-espaces.md`) : dans la **Bibliothèque** la
//! référence est celle épinglée par `c` (`◆`), dans le **sélecteur** de la Chaîne c'est l'**actuel**
//! (`●`) — d'où le paramétrage par « courbes + référence » plutôt que par onglet.

use super::*;

/// Largeur minimale du corps pour afficher le panneau spectre à droite de la liste. C'est aussi ce
/// qui définit un écran « large » pour le panneau Bandes : une seule valeur pour les deux.
pub const SPECTRUM_MIN_WIDTH: u16 = crate::browse::state::bands::WIDE_COLUMNS;

type Profile = std::sync::Arc<dsp::profile::IrProfile>;

pub(super) fn curve(p: &dsp::profile::IrProfile) -> Vec<(f64, f64)> {
    dsp::profile::band_centers()
        .iter()
        .zip(&p.bands_db)
        .map(|(f, d)| ((*f as f64).log10(), (*d as f64).clamp(-24.0, 12.0)))
        .collect()
}

/// Ce que le panneau montre.
pub(super) struct Shown {
    /// Les courbes au premier plan : (nom, profil, couleur).
    pub curves: Vec<(String, Profile, Color)>,
    /// La référence, superposée en jaune.
    pub reference: Option<(String, Profile)>,
    /// La marque de la référence : `◆` (épinglée) ou `●` (l'actuel du sélecteur).
    pub mark: &'static str,
    /// Ligne « grain : … » (modèles) : le grain sous le curseur, et celui de la référence.
    pub grain: Option<(String, Option<String>)>,
    /// Ce que dit le panneau quand il n'y a pas de référence.
    pub hint: &'static str,
    /// Le panneau **Bandes** est dessiné juste en dessous : les écarts par zone, qu'il
    /// montre en colonnes, n'ont plus à être répétés en texte ici.
    pub bands_shown: bool,
}

/// Panneau « Spectre » de la Bibliothèque : courbe sous le curseur (ou du mélange), référence
/// épinglée par `c`, écarts par zone.
pub(super) fn render_spectrum(s: &State, f: &mut Frame, area: ratatui::layout::Rect) {
    let profile_of = |i: usize| s.irs[i].profile.as_ref().map(|d| d.profile.clone());
    let mprofile = |i: usize| s.models[i].profile.as_ref().map(|d| d.profile.clone());
    let mut curves: Vec<(String, Profile, Color)> = Vec::new();
    match s.tab {
        Tab::Models => {
            if let Some(m) = s.model_at_cursor() {
                if let Some(pr) = mprofile(m) {
                    curves.push((s.models[m].label.clone(), pr, Color::Cyan));
                }
            }
        }
        Tab::Blend => {
            if let Some(b) = &s.blend_profile {
                curves.push(("mélange".into(), b.clone(), Color::Magenta));
            }
            if let Some(p) = s.blend.get(s.cursor()) {
                if let Some(pr) = profile_of(p.ir) {
                    curves.push((s.irs[p.ir].label.clone(), pr, Color::Cyan));
                }
            }
        }
        Tab::Irs => {
            if let Some(i) = s.ir_at_cursor() {
                if let Some(pr) = profile_of(i) {
                    curves.push((s.irs[i].label.clone(), pr, Color::Cyan));
                }
            }
        }
    }
    let reference = if s.tab == Tab::Models {
        s.ref_model.and_then(|r| mprofile(r).map(|p| (s.models[r].label.clone(), p)))
    } else {
        s.reference.and_then(|r| profile_of(r).map(|p| (s.irs[r].label.clone(), p)))
    };
    let grain = (s.tab == Tab::Models)
        .then(|| (grain_of(s, s.model_at_cursor()), s.ref_model.map(|r| grain_of(s, Some(r)))));
    // sur un écran large, le panneau Bandes se glisse sous la courbe : il ne prend aucune ligne à
    // la liste, et il remplace la ligne de texte des écarts par zone. Sur un panneau trop
    // court, la courbe garde toute la place et le texte reste.
    let (top, panel) = if s.show_bands() && area.height >= bands::HEIGHT + 8 {
        let [a, b] = Layout::vertical([Constraint::Min(8), Constraint::Length(bands::HEIGHT)]).areas(area);
        (a, Some(b))
    } else {
        (area, None)
    };
    draw(
        f,
        top,
        &Shown {
            curves,
            reference,
            mark: "◆",
            grain,
            hint: " c : épingler la ligne sous le curseur comme référence (comparaison)",
            bands_shown: panel.is_some(),
        },
    );
    if let Some(b) = panel {
        bands::render(s, f, b);
    }
}

/// Panneau « Spectre » du **sélecteur** (§ 5) : la courbe de l'élément sous le curseur, et
/// l'**actuel** comme référence.
pub(super) fn render_picker_spectrum(s: &State, f: &mut Frame, area: ratatui::layout::Rect) {
    let Some(p) = s.picker.as_ref() else { return };
    let profile = |index: usize| -> Option<(String, Profile)> {
        let it = if p.nam { s.models.get(index)? } else { s.irs.get(index)? };
        Some((it.label.clone(), it.profile.as_ref()?.profile.clone()))
    };
    let under = s.pick_row().and_then(|r| profile(r.index));
    let current = p.current.filter(|c| Some(*c) != s.pick_row().map(|r| r.index)).and_then(profile);
    let grain =
        p.nam.then(|| (grain_of(s, s.pick_row().map(|r| r.index)), p.current.map(|c| grain_of(s, Some(c)))));
    draw(
        f,
        area,
        &Shown {
            curves: under.map(|(n, pr)| vec![(n, pr, Color::Cyan)]).unwrap_or_default(),
            reference: current,
            mark: "●",
            grain,
            hint: " ↑↓ : essayer un autre son ; l'actuel est superposé en référence",
            bands_shown: false,
        },
    );
}

/// Le grain d'un modèle, pour la ligne du bas.
fn grain_of(s: &State, m: Option<usize>) -> String {
    m.and_then(|m| s.models.get(m))
        .and_then(|it| it.profile.as_ref().and_then(|d| d.grain.clone()))
        .unwrap_or_else(|| "?".into())
}

/// Dessine le panneau : le graphique, puis les écarts par zone et le grain.
fn draw(f: &mut Frame, area: ratatui::layout::Rect, shown: &Shown) {
    use ratatui::symbols::Marker;
    use ratatui::widgets::{Axis, Chart, Dataset, GraphType};
    let info_h = if shown.bands_shown { 2 } else { 4 };
    let [chart_area, info] = Layout::vertical([Constraint::Min(6), Constraint::Length(info_h)]).areas(area);
    let curves: Vec<Vec<(f64, f64)>> = shown
        .curves
        .iter()
        .map(|(_, p, _)| curve(p))
        .chain(shown.reference.iter().map(|(_, p)| curve(p)))
        .collect();
    let mut datasets = Vec::new();
    for (k, (name, _, color)) in shown.curves.iter().enumerate() {
        datasets.push(
            Dataset::default()
                .name(name.clone())
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(*color))
                .data(&curves[k]),
        );
    }
    if let Some((name, _)) = &shown.reference {
        datasets.push(
            Dataset::default()
                .name(format!("{} {name}", shown.mark))
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
    match (shown.curves.last(), &shown.reference) {
        // les écarts par zone, en texte : le panneau Bandes les dit mieux quand il est là
        (Some((name, p, _)), Some((rname, r))) if !shown.bands_shown => {
            let (a, b) = (library::profile::zones(p), library::profile::zones(r));
            let parts: Vec<String> = library::profile::ZONES
                .iter()
                .enumerate()
                .map(|(k, (z, ..))| format!("{z} {:+.0}", a[k] - b[k]))
                .collect();
            text.push(Line::from(Span::styled(
                format!(" {name} vs {} {rname} :", shown.mark),
                Style::new().fg(Color::Yellow),
            )));
            text.push(Line::raw(format!(" {}", parts.join(" · "))));
        }
        (Some(_), Some(_)) => {}
        (Some(_), None) => text.push(Line::from(Span::styled(shown.hint, Style::new().fg(Color::DarkGray)))),
        (None, _) => text.push(Line::from(Span::styled(
            " pas de profil pour cette ligne",
            Style::new().fg(Color::DarkGray),
        ))),
    }
    if let Some((grain, rgrain)) = &shown.grain {
        let mut g = vec![Span::raw(format!(" grain : {grain}"))];
        if let Some(r) = rgrain {
            g.push(Span::styled(format!("  ·  {} {r}", shown.mark), Style::new().fg(Color::Yellow)));
        }
        text.insert(0, Line::from(g));
    }
    f.render_widget(Paragraph::new(text).wrap(ratatui::widgets::Wrap { trim: true }), info);
}
