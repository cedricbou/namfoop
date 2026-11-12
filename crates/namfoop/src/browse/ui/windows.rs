//! Les **fenêtres** de `browse`, dessinées par-dessus l'écran : « Modifications non enregistrées »
//! (`q`, § 8 de `browse-chaine.md`) et « Mettre « … » dans la chaîne » (Entrée dans la
//! Bibliothèque, § 4 de `browse-espaces.md`). La fenêtre du catalogue est dans `ui/fx.rs`.

use super::*;

/// Fenêtre « Modifications non enregistrées » (`q`) : dit ce qui serait perdu, et n'offre qu'un
/// geste volontaire pour quitter (`Q` majuscule). Elle se dessine en dernier, par-dessus l'onglet.
pub(super) fn render_confirm_quit(s: &State, body: Rect, f: &mut Frame) {
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

/// Fenêtre « Mettre « … » dans la chaîne » (§ 4) : un `remplacer` par bloc du bon type, `ajouter`
/// en dernier, et la case « avec son IR ». Elle se dessine par-dessus la liste.
pub(super) fn render_send(s: &State, body: Rect, f: &mut Frame) {
    let Some(send) = s.send.as_ref() else { return };
    let mut lines = Vec::new();
    let cur = send.cursor.min(send.opts.len() - 1);
    for (i, opt) in send.opts.iter().enumerate() {
        lines.push(Line::from(vec![
            cursor_span(i == cur),
            Span::styled(
                s.send_option_text(*opt),
                if i == cur { Style::new().add_modifier(Modifier::BOLD) } else { Style::new() },
            ),
        ]));
    }
    if let Some(text) = s.send_ir_text() {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("  [{}] ", if send.checked { "x" } else { " " }),
                Style::new().fg(Color::Cyan),
            ),
            Span::raw(text),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("  Entrée valider{} · Échap annuler", if send.with_ir { " · Espace cocher" } else { "" }),
        Style::new().fg(Color::DarkGray),
    ));
    let title = format!(" Mettre « {} » dans la chaîne ", send.what);
    let w = (lines.iter().map(|l| l.width()).max().unwrap_or(0).max(title.chars().count()) as u16 + 4)
        .min(body.width);
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
                .border_style(Style::new().fg(Color::Cyan))
                .title(Span::styled(title, Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
                .style(Style::new().bg(Color::Black)),
        ),
        area,
    );
}
