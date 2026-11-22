//! Fenêtre de l'éditeur `e` des métadonnées d'un modèle.

use super::*;

pub(super) fn type_help(t: &str) -> &'static str {
    match library::recommend::needs_ir(t) {
        library::recommend::NeedsIr::No => "baffle inclus : pas d'IR",
        library::recommend::NeedsIr::Yes => "sans baffle : IR requise",
        library::recommend::NeedsIr::Maybe => "inconnu",
    }
}

/// Panneau d'édition : métadonnées du modèle en écoute, valeur du fichier en regard.
pub(super) fn render_editor(s: &State, ed: &Editor, f: &mut Frame, area: ratatui::layout::Rect) {
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
        let style =
            if on { Style::new().fg(theme::key()).add_modifier(Modifier::BOLD) } else { Style::new() };
        lines.push(Line::from(vec![
            cursor_span(on),
            Span::styled(format!("{label:<16}"), style),
            Span::styled(format!("{value:<40}"), style),
            Span::styled(format!(" {hint}"), Style::new().fg(theme::dim())),
        ]));
    }
    lines.push(Line::raw(""));
    for t in [
        "Joue en réglant l'entrée jusqu'au grain voulu (vumètre out et limiteur en bas).",
        "Baffle inclus ou non ? Compare l'IR « aucune » et une IR : avec un baffle en double, le son",
        "devient sourd et étouffé ; sans baffle ni IR, il est criard et bourdonnant.",
    ] {
        lines.push(Line::from(Span::styled(format!("   {t}"), Style::new().fg(theme::dim()))));
    }
    let title = format!(" Modifier {} {}  [{}] ", item.id, item.label, item.pack);
    f.render_widget(Paragraph::new(lines).block(Block::bordered().title(title)), area);
}
