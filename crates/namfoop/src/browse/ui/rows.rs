//! Une ligne de modèle, une ligne d'IR : le même dessin dans les listes de la Bibliothèque et dans
//! le sélecteur de la Chaîne (§ 5 de `docs/design/browse-espaces.md`). Seules la **marque** de la
//! colonne de gauche et la **queue** (ce qui suit le nom) changent d'un endroit à l'autre.

use super::*;

/// Largeur d'une colonne de noms : le plus long, borné.
pub(super) fn label_width(labels: impl Iterator<Item = usize>, max: usize) -> usize {
    labels.max().unwrap_or(0).min(max)
}

/// Ligne d'un modèle : identifiant, badge IR, marque, nom, queue, mini-spectre, grain, étiquettes,
/// type de capture et pack.
pub(super) fn model_line(
    s: &State,
    idx: usize,
    on_cursor: bool,
    mark: Span<'static>,
    label_w: usize,
    tail: Vec<Span<'static>>,
) -> Line<'static> {
    let m = &s.models[idx];
    let mut spans = vec![
        cursor_span(on_cursor),
        Span::styled(format!("{:<6}", m.id), Style::new().fg(theme::dim())),
        badge(m.needs),
        mark,
        Span::raw(format!("{:<w$}", m.label, w = label_w)),
    ];
    spans.extend(tail);
    if let Some(d) = &m.profile {
        spans.push(Span::styled(format!("  {}", d.spark), Style::new().fg(theme::curve())));
        if let Some(g) = &d.grain {
            spans.push(Span::styled(format!("  {g}"), grain_style(g, s)));
        }
        spans.push(Span::raw(format!("  {}", d.tags.join(", "))));
    }
    spans.push(Span::styled(format!("  {}  [{}]", m.detail, m.pack), Style::new().fg(theme::dim())));
    Line::from(spans)
}

/// Ligne d'une IR (ou d'un mélange enregistré) : identifiant, marque, nom, queue, mini-spectre,
/// étiquettes, pack, et sa part du mélange en cours s'il y en a une.
pub(super) fn ir_line(
    s: &State,
    idx: usize,
    on_cursor: bool,
    mark: Span<'static>,
    label_w: usize,
    tail: Vec<Span<'static>>,
) -> Line<'static> {
    let it = &s.irs[idx];
    let mut spans = vec![
        cursor_span(on_cursor),
        Span::styled(format!("{:<6}", it.id), Style::new().fg(theme::dim())),
        mark,
        Span::raw(format!("{:<w$}", it.label, w = label_w)),
    ];
    spans.extend(tail);
    if let Some(d) = &it.profile {
        spans.push(Span::styled(format!("  {}", d.spark), Style::new().fg(theme::curve())));
        spans.push(Span::raw(format!("  {}", d.tags.join(", "))));
    }
    spans.push(Span::styled(format!("  [{}]", it.pack), Style::new().fg(theme::dim())));
    if let Some(p) = s.blend.iter().find(|p| p.ir == idx) {
        spans.push(Span::styled(
            format!("  [mélange {:+.0} dB{}]", p.gain_db, if p.invert { " ø" } else { "" }),
            Style::new().fg(theme::blend()),
        ));
    }
    Line::from(spans)
}
