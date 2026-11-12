//! Le panneau sous le synoptique quand le curseur est sur un bloc `nam` / `ir` : son **contenu en
//! entier** (ce que l'étiquette tronquée du synoptique ne peut pas montrer), et le **sélecteur**
//! une fois Entrée tapée (§ 5 de `docs/design/browse-espaces.md`).

use super::rows;
use super::*;
use crate::browse::state::chain::Leaf;

/// Le contenu du bloc `nam` / `ir` sous le curseur, en entier. `None` : ce n'est pas un tel bloc.
pub(super) fn block_info(s: &State) -> Option<Vec<Line<'static>>> {
    let dim = Style::new().fg(Color::DarkGray);
    let amp = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
    match s.fx.chain.leaf(s.fx.cursor)? {
        Leaf::Nam { model, .. } => {
            let mut lines = Vec::new();
            match model {
                Some(m) => {
                    let it = &s.models[*m];
                    lines.push(Line::from(vec![
                        Span::styled(format!(" {}", it.label), amp),
                        Span::styled(format!("   [{}]", it.pack), dim),
                    ]));
                    let mut second =
                        vec![Span::styled(format!(" {}", it.id), dim), Span::raw("  "), badge(it.needs)];
                    if let Some(d) = &it.profile {
                        second.push(Span::styled(format!(" {}", d.spark), Style::new().fg(Color::Cyan)));
                        if let Some(g) = &d.grain {
                            second.push(Span::styled(format!("  {g}"), grain_style(g)));
                        }
                    }
                    second.push(Span::styled(format!("  {}", it.detail), dim));
                    lines.push(Line::from(second));
                }
                None => lines.push(Line::styled(" (aucun modèle : Entrée pour en choisir un)", dim)),
            }
            lines.push(Line::styled(" Entrée : changer de modèle, essayé dans la chaîne", dim));
            Some(lines)
        }
        Leaf::Ir { ir, .. } => {
            let mut lines = Vec::new();
            match ir {
                IrChoice::Item(i) => {
                    let it = &s.irs[*i];
                    lines.push(Line::from(vec![
                        Span::styled(format!(" {}", it.label), amp),
                        Span::styled(format!("   [{}]", it.pack), dim),
                    ]));
                    let mut second = vec![Span::styled(format!(" {}", it.id), dim)];
                    if let Some(d) = &it.profile {
                        second.push(Span::styled(format!("  {}", d.spark), Style::new().fg(Color::Cyan)));
                        second.push(Span::raw(format!("  {}", d.tags.join(", "))));
                    }
                    lines.push(Line::from(second));
                }
                IrChoice::Blend => lines.push(Line::from(Span::styled(
                    format!(" mélange de {} IR (onglet Mélange)", s.blend.len()),
                    amp,
                ))),
                IrChoice::None => lines.push(Line::styled(" (aucune IR : Entrée pour en choisir une)", dim)),
            }
            lines.push(Line::styled(" Entrée : changer d'IR, essayée dans la chaîne", dim));
            Some(lines)
        }
        Leaf::Fx(_) => None,
    }
}

/// Les lignes du sélecteur : le filtre, puis la liste (même dessin que dans la Bibliothèque), avec
/// `● actuel` sur ce qui était là et `◀ à l'essai` sur ce qu'on entend.
pub(super) fn picker_lines(s: &State, height: usize) -> Vec<Line<'static>> {
    let Some(p) = s.picker.as_ref() else { return Vec::new() };
    let list = s.pick_rows();
    let cur = s.pick_cursor();
    let mut lines = vec![if p.filtering || !p.filter.is_empty() {
        Line::from(vec![
            Span::styled(" filtre : ", Style::new().fg(Color::Cyan)),
            Span::raw(format!("{}{}", p.filter, if p.filtering { "█" } else { "" })),
        ])
    } else {
        Line::styled(" / pour filtrer", Style::new().fg(Color::DarkGray))
    }];
    if list.is_empty() {
        lines.push(Line::styled("   rien ne correspond au filtre", Style::new().fg(Color::DarkGray)));
        return lines;
    }
    let label_w = rows::label_width(
        list.iter().map(|r| {
            let it = if p.nam { &s.models[r.index] } else { &s.irs[r.index] };
            it.label.chars().count()
        }),
        36,
    );
    let shown = window(list.len(), cur, height.max(1));
    let rest = list.len() - shown.end;
    for i in shown {
        let row = list[i];
        let is_current = p.current == Some(row.index);
        let mut tail = Vec::new();
        if is_current {
            tail.push(if p.ab {
                Span::styled(
                    "  ● actuel ◀ écouté (c)",
                    Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("  ● actuel", Style::new().fg(Color::Green))
            });
        }
        if p.trial == Some(row.index) && !is_current {
            tail.push(Span::styled(
                "  ◀ à l'essai",
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
        }
        lines.push(if p.nam {
            let mark = if s.models[row.index].corrected() {
                Span::styled("✎", Style::new().fg(Color::Magenta))
            } else {
                Span::raw(" ")
            };
            rows::model_line(s, row.index, i == cur, mark, label_w, tail)
        } else {
            let mark = if row.recommended {
                Span::styled(" ★ ", Style::new().fg(Color::Yellow))
            } else if row.blend {
                Span::styled(" ≡ ", Style::new().fg(Color::Magenta))
            } else {
                Span::raw("   ")
            };
            rows::ir_line(s, row.index, i == cur, mark, label_w, tail)
        });
    }
    if rest > 0 {
        lines.push(Line::styled(format!("   … {rest} de plus"), Style::new().fg(Color::DarkGray)));
    }
    lines
}
