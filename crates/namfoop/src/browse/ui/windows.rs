//! Les **fenêtres** de `browse`, dessinées par-dessus l'écran : « Modifications non enregistrées »
//! (`q`, § 8 de `browse-chaine.md`) et « Mettre « … » dans la chaîne » (Entrée dans la
//! Bibliothèque, § 4 de `browse-espaces.md`). La fenêtre du catalogue est dans `ui/fx.rs`.

use super::super::state::preset::Pending;
use super::*;
use crate::browse::state::bindings::bindings;
use crate::browse::state::context::Context;

/// Fenêtre « Modifications non enregistrées » (`q`) : dit ce qui serait perdu, et n'offre qu'un
/// geste volontaire pour quitter (`Q` majuscule). Elle se dessine en dernier, par-dessus l'onglet.
pub(super) fn render_confirm_quit(s: &State, body: Rect, f: &mut Frame) {
    // la scène (§ 9) : rien ne s'enregistre ici, donc la question est simple — mais elle est posée,
    // parce que couper le son d'un concert sur une touche serait pire
    if s.locked {
        let lines = vec![
            Line::raw("  Le set joue."),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Q", Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw("       quitter la scène"),
            ]),
            Line::from(vec![
                Span::styled("  Échap", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw("   rester (par défaut)"),
            ]),
        ];
        let w = (lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16 + 4).min(body.width);
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
                        " Quitter la scène ? ",
                        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ))
                    .style(Style::new().bg(Color::Black)),
            ),
            area,
        );
        return;
    }
    let empty = s.empty_chain_warning();
    let mut lines = Vec::new();
    // chaîne vide alors que le banc joue un son : on le signale toujours (§ 9)
    if let Some(what) = &empty {
        lines.push(Line::from(Span::styled(
            format!("Tu écoutes au banc {what}, mais ce son n'est"),
            Style::new().fg(Color::Yellow),
        )));
        lines.push(Line::from(Span::styled(
            "pas dans la chaîne : la commande affichée en quittant ne le jouera pas.",
            Style::new().fg(Color::Yellow),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("  Entrée", Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("  le mettre dans la chaîne et rester (2 pour la voir)"),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Q", Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("       quitter quand même"),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Échap", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("   rester (par défaut)"),
        ]));
    }
    let unsaved = s.unsaved();
    if !unsaved.is_empty() {
        if empty.is_some() {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(Span::styled(
            "Ceci n'est pas enregistré :",
            Style::new().add_modifier(Modifier::BOLD),
        )));
        for what in unsaved {
            lines.push(Line::from(Span::styled(format!("  • {what}"), Style::new().fg(Color::Yellow))));
        }
    }
    if empty.is_none() {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("  Q", Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("     quitter sans enregistrer"),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Échap", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" ou toute autre touche : rester (par défaut)"),
        ]));
    }

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
                    if empty.is_some() {
                        " La chaîne est vide "
                    } else {
                        " Modifications non enregistrées "
                    },
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

/// Une ligne de l'aide : les touches, puis ce qu'elles font.
fn key_line(keys: &str, what: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {keys:<14}"), Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(what.to_string()),
    ])
}

/// Le titre et les touches de l'écran où l'on est (`h`, § 11 de `browse-espaces.md`).
/// **`L` : choisir le set** (§ 7 de `docs/design/set.md`). `aucun set` rend la banque entière,
/// `＋ nouveau set…` en crée un vide (la suite du geste est l'US 4).
pub(super) fn render_set_picker(s: &State, f: &mut Frame, area: Rect) {
    let Some(p) = s.set_picker.as_ref() else { return };
    let rows: Vec<(String, String)> = p.sets.iter().map(|e| e.columns()).collect();
    let left = rows.iter().map(|(a, _)| a.chars().count()).max().unwrap_or(10).min(34);
    let mut lines: Vec<Line> = Vec::new();
    for (i, (a, b)) in rows.iter().enumerate() {
        let on = i == p.cursor;
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} {:<left$}  ", if on { "▶" } else { " " }, a, left = left),
                if on {
                    Style::new().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                },
            ),
            Span::styled(b.clone(), Style::new().fg(Color::DarkGray)),
        ]));
    }
    lines.push(Line::raw(""));
    lines
        .push(Line::styled("  ↑↓ choisir · Entrée activer · Échap fermer", Style::new().fg(Color::DarkGray)));
    let h = (lines.len() as u16 + 2).min(area.height);
    let w = (left as u16 + 56).min(area.width);
    let win = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, win);
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" Choisir le set ").border_style(Style::new().fg(Color::Green))),
        win,
    );
}

/// Le titre et les touches de l'écran où l'on est (`h`, § 11 de `browse-espaces.md`).
///
/// **Dérivée de la table** depuis #243 : les ~96 paires (touche, explication) qui vivaient ici,
/// écrites une par une, étaient une copie du dispatch que rien ne vérifiait. Ne restent ici que ce
/// que la table ne peut pas porter : le **titre** et les **notes** en gris.
fn help_lines(s: &State) -> (String, Vec<Line<'static>>) {
    // l'aide s'ouvre AU-DESSUS d'un contexte : c'est celui du dessous qu'elle explique
    let ctx = s.context_under_help();
    let mut v: Vec<Line<'static>> = bindings(ctx).iter().map(|b| key_line(b.label, b.what)).collect();
    // l'alignement sur le DI n'est pas une touche, mais c'est la question qu'on se pose devant un
    // gain qui ne fait rien : l'étage grisé en bout de synoptique (#178)
    if matches!(ctx, Context::Chain | Context::ChainMerge) && s.level_match {
        v.push(key_line(
            "niveau DI",
            "volume aligné sur le DI : l'étage grisé annule les blocs gain (reste la saturation)",
        ));
    }
    let notes = help_notes(s, ctx);
    if !notes.is_empty() {
        v.push(Line::raw(""));
        for n in notes {
            v.push(Line::styled(n, Style::new().fg(Color::DarkGray)));
        }
    }
    (help_title(s, ctx), v)
}

/// Le titre de l'overlay : il dit **où l'on est**, ce que la liste de touches ne dit pas.
fn help_title(s: &State, ctx: Context) -> String {
    match ctx {
        Context::Stage | Context::StageQuit => "🔒 run --set — la scène".into(),
        Context::Help => "Aide".into(),
        Context::ConfirmQuit | Context::ConfirmQuitEmpty => "Quitter ?".into(),
        Context::ConfirmSwitch => "Modifications non enregistrées".into(),
        Context::Send => "Mettre le son du banc dans la chaîne".into(),
        Context::Place => "Où poser le bloc".into(),
        Context::Editor | Context::EditorTyping => "Modifier et calibrer le modèle (e)".into(),
        Context::Bank => "Banque de presets".into(),
        Context::Catalog => "Catalogue des blocs".into(),
        Context::Picker | Context::PickerFiltering => {
            let quoi = if s.picker.as_ref().is_some_and(|p| p.nam) { "le modèle" } else { "l'IR" };
            format!("Chaîne · choisir {quoi} d'un bloc")
        }
        Context::SetPicker => "Choisir le set".into(),
        Context::Set => "Set — la vue scène".into(),
        Context::SetEdit | Context::SetEditTyping | Context::SetEditPicking | Context::SetEditRemove => {
            format!("✎ Éditer le set — {}", s.set.as_ref().map(|x| x.stem.as_str()).unwrap_or(""))
        }
        Context::ChainMerge => "Chaîne · sortie du parallèle (◇)".into(),
        Context::Params => "Chaîne · réglages".into(),
        Context::Chain => "Chaîne · le synoptique est l'éditeur".into(),
        Context::Naming | Context::Filtering | Context::Models | Context::Irs | Context::Blend => {
            let tab = match s.tab {
                Tab::Models => "Modèles",
                Tab::Irs => "IR",
                Tab::Blend => "Mélange",
            };
            format!("Bibliothèque · {tab} (banc d'essai)")
        }
    }
}

/// Les notes en gris : ce qu'il faut savoir et qui n'est pas une touche.
fn help_notes(s: &State, ctx: Context) -> Vec<String> {
    match ctx {
        Context::Stage | Context::StageQuit => vec![
            "  Rien ne s'édite ici : pas de preset, pas de bibliothèque, pas de choix de set.".into(),
            "  Pour régler quelque chose, quitte et ouvre `namfoop browse`.".into(),
        ],
        Context::Bank => {
            vec![" Les presets vivent dans library/presets/ ; un son manquant est signalé en rouge.".into()]
        }
        Context::Picker | Context::PickerFiltering => {
            vec![" Tant que tu n'as pas validé, la chaîne n'est pas modifiée.".into()]
        }
        Context::ChainMerge => {
            vec![" Les branches se somment ici. Ses réglages propres (niveau, mix) viendront.".into()]
        }
        Context::SetPicker if s.set.is_none() => {
            vec![" Aucun set actif : la banque entière répond à PgDn / PgUp, en boucle.".into()]
        }
        _ => {
            let _ = s;
            Vec::new()
        }
    }
}

/// L'overlay d'aide (`h`) : les touches de l'écran où l'on est, par-dessus tout l'écran.
pub(super) fn render_help(s: &State, area: Rect, f: &mut Frame) {
    let (what, mut lines) = help_lines(s);
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        " h ou n'importe quelle touche : fermer cette aide",
        Style::new().fg(Color::DarkGray),
    ));
    let title = format!(" Aide · {what} ");
    let w = (lines.iter().map(|l| l.width()).max().unwrap_or(40).max(title.chars().count()) as u16 + 4)
        .min(area.width);
    let h = (lines.len() as u16 + 2).min(area.height);
    let at = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, at);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_style(Style::new().fg(Color::Cyan))
                .title(Span::styled(title, Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
                .style(Style::new().bg(Color::Black)),
        ),
        at,
    );
}

/// La **banque** de presets (`P`, #110) : la liste, le filtre `/`, et la chaîne de chacun en une
/// ligne. Un preset dont un son manque est grisé, avec la raison à la place de sa chaîne.
pub(super) fn render_bank(s: &State, area: Rect, f: &mut Frame) {
    let Some(bank) = s.bank.as_ref() else { return };
    let rows = s.bank_rows();
    let cur = s.bank_cursor();
    let mut lines = vec![if bank.filtering || !bank.filter.is_empty() {
        Line::from(vec![
            Span::styled(" filtre : ", Style::new().fg(Color::Cyan)),
            Span::raw(format!("{}{}", bank.filter, if bank.filtering { "█" } else { "" })),
        ])
    } else {
        Line::styled(" / pour filtrer", Style::new().fg(Color::DarkGray))
    }];
    if rows.is_empty() {
        lines.push(Line::styled(
            if s.presets.is_empty() {
                "   aucun preset : S enregistre la chaîne dans library/presets/"
            } else {
                "   rien ne correspond au filtre"
            },
            Style::new().fg(Color::DarkGray),
        ));
    }
    let name_w = rows.iter().map(|&i| s.presets[i].name.chars().count()).max().unwrap_or(0).min(32);
    let shown = window(rows.len(), cur, (area.height as usize).saturating_sub(7).max(1));
    let rest = rows.len() - shown.end;
    for k in shown {
        let i = rows[k];
        let p = &s.presets[i];
        let broken = p.chain.is_none();
        let courant = s.preset.as_ref().is_some_and(|r| r.path == p.path);
        lines.push(Line::from(vec![
            cursor_span(k == cur),
            Span::styled(format!("{:<6}", p.id), Style::new().fg(Color::DarkGray)),
            Span::styled(
                format!("{:<w$}", p.name, w = name_w),
                if broken {
                    Style::new().fg(Color::DarkGray)
                } else if courant {
                    Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                },
            ),
            Span::styled(
                format!("  {}", s.preset_line(i)),
                if broken { Style::new().fg(Color::Red) } else { Style::new().fg(Color::DarkGray) },
            ),
            Span::styled(if courant { "  ◀ en place" } else { "" }, Style::new().fg(Color::Green)),
        ]));
    }
    if rest > 0 {
        lines.push(Line::styled(format!("   … {rest} de plus"), Style::new().fg(Color::DarkGray)));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  ↑↓ choisir · Entrée charger · / filtrer · Échap fermer · S enregistrer la chaîne",
        Style::new().fg(Color::DarkGray),
    ));
    let title = " Banque de presets ";
    let w = (lines.iter().map(|l| l.width()).max().unwrap_or(40).max(title.chars().count()) as u16 + 4)
        .min(area.width);
    let h = (lines.len() as u16 + 2).min(area.height);
    let at = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, at);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_style(Style::new().fg(Color::Cyan))
                .title(Span::styled(title, Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
                .style(Style::new().bg(Color::Black)),
        ),
        at,
    );
}

/// « La chaîne n'est pas enregistrée » : demandé **une fois** avant de charger un autre preset ou
/// de repartir d'une chaîne vierge (`n`, #170).
pub(super) fn render_confirm_switch(s: &State, body: Rect, f: &mut Frame) {
    let what = match s.pending_switch {
        Some(Pending::New) => "Repartir d'une chaîne vierge remplacerait la chaîne en cours.".to_string(),
        Some(Pending::Preset(i)) => format!(
            "Charger « {} » remplacerait la chaîne en cours.",
            s.presets.get(i).map(|p| p.name.as_str()).unwrap_or_default()
        ),
        None => String::new(),
    };
    let mut lines = vec![Line::from(Span::styled(what, Style::new().fg(Color::Yellow)))];
    for what in s.unsaved() {
        lines.push(Line::from(Span::styled(format!("  • {what}"), Style::new().fg(Color::Yellow))));
    }
    lines.push(Line::raw(""));
    let go = if s.pending_switch == Some(Pending::New) { "repartir" } else { "charger" };
    for (k, t) in [
        ("  S", format!("      enregistrer la chaîne, puis {go}")),
        ("  Entrée", format!(" {go} quand même")),
        ("  Échap", "  rester (par défaut)".to_string()),
    ] {
        lines.push(Line::from(vec![
            Span::styled(k, Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t),
        ]));
    }
    let w = (lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16 + 4).min(body.width);
    let h = (lines.len() as u16 + 2).min(body.height);
    let at = Rect {
        x: body.x + (body.width.saturating_sub(w)) / 2,
        y: body.y + (body.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, at);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(Span::styled(
                    " La chaîne n'est pas enregistrée ",
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
                .style(Style::new().bg(Color::Black)),
        ),
        at,
    );
}
