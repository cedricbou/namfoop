//! Les **fenêtres** de `browse`, dessinées par-dessus l'écran : « Modifications non enregistrées »
//! (`q`, § 8 de `browse-chaine.md`) et « Mettre « … » dans la chaîne » (Entrée dans la
//! Bibliothèque, § 4 de `browse-espaces.md`). La fenêtre du catalogue est dans `ui/fx.rs`.

use super::super::state::chain::Spot;
use super::super::state::fx::FxFocus;
use super::super::state::preset::Pending;
use super::*;

/// Fenêtre « Modifications non enregistrées » (`q`) : dit ce qui serait perdu, et n'offre qu'un
/// geste volontaire pour quitter (`Q` majuscule). Elle se dessine en dernier, par-dessus l'onglet.
pub(super) fn render_confirm_quit(s: &State, body: Rect, f: &mut Frame) {
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
fn help_lines(s: &State) -> (String, Vec<Line<'static>>) {
    let mut v = Vec::new();
    // les fenêtres et modes qui prennent le clavier passent avant le reste
    if s.send.is_some() {
        v.push(key_line("↑↓", "choisir le bloc à remplacer, ou « ajouter »"));
        if s.send.as_ref().is_some_and(|x| x.with_ir) {
            v.push(key_line("Espace", "cocher « avec son IR »"));
        }
        v.push(key_line("Entrée", "valider"));
        v.push(key_line("Échap", "annuler : rien n'est changé dans la chaîne"));
        return ("Mettre le son du banc dans la chaîne".into(), v);
    }
    if s.place.is_some() {
        v.push(key_line("←→ ↑↓", "choisir l'endroit dans la chaîne"));
        v.push(key_line("Entrée", "poser après le curseur"));
        v.push(key_line("Échap", "annuler et revenir à la Bibliothèque"));
        return ("Où poser le bloc".into(), v);
    }
    if s.editor.is_some() {
        v.push(key_line("↑↓", "champ (type, entrée, IR préférée, marque, nom)"));
        v.push(key_line("←→", "changer la valeur — le changement s'entend tout de suite"));
        v.push(key_line("Entrée", "saisir un texte (marque, nom)"));
        v.push(key_line("s", "enregistrer dans library/overrides.toml"));
        v.push(key_line("a", "appliquer ce changement à tous les modèles filtrés"));
        v.push(key_line("b / r", "avec ou sans IR · A/B avec la référence"));
        v.push(key_line("Échap", "fermer sans enregistrer"));
        return ("Modifier et calibrer le modèle (e)".into(), v);
    }
    if s.bank.is_some() {
        v.push(key_line("↑↓", "choisir un preset"));
        v.push(key_line("Entrée", "le charger (la chaîne en cours est remplacée)"));
        v.push(key_line("/", "filtrer par nom ou identifiant"));
        v.push(key_line("S", "enregistrer la chaîne en cours comme preset"));
        v.push(key_line("n", "repartir d'une chaîne vierge (la banque se referme)"));
        v.push(key_line("Échap", "fermer la banque"));
        v.push(Line::raw(""));
        v.push(Line::styled(
            " Les presets vivent dans library/presets/ ; un son manquant est signalé en rouge.",
            Style::new().fg(Color::DarkGray),
        ));
        return ("Banque de presets".into(), v);
    }
    if s.fx.catalog.is_some() {
        v.push(key_line("tape", "filtrer le catalogue"));
        v.push(key_line("↑↓", "choisir"));
        v.push(key_line("Entrée", "ajouter le bloc (nam et ir ouvrent aussitôt leur liste)"));
        v.push(key_line("Échap", "annuler"));
        return ("Catalogue des blocs".into(), v);
    }
    if let Some(p) = s.picker.as_ref() {
        v.push(key_line("↑↓", "essayer dans la chaîne, avec ses effets"));
        v.push(key_line("Entrée", "garder ce qu'on entend"));
        v.push(key_line(
            "Échap",
            if p.is_new { "ne rien ajouter : le bloc est retiré" } else { "revenir à l'ancien" },
        ));
        v.push(key_line("c", "actuel ↔ essai, sans bouger le curseur"));
        v.push(key_line("o", "n'écouter que ce bloc et son voisin (basse › nam › ir)"));
        v.push(key_line("r", "avec ou sans les effets de la chaîne"));
        v.push(key_line("/", "filtrer la liste"));
        v.push(Line::raw(""));
        v.push(Line::styled(
            " Tant que tu n'as pas validé, la chaîne n'est pas modifiée.",
            Style::new().fg(Color::DarkGray),
        ));
        let quoi = if p.nam { "le modèle" } else { "l'IR" };
        return (format!("Chaîne · choisir {quoi} d'un bloc"), v);
    }
    if s.space == Space::Chain {
        if s.fx.chain.spot(s.fx.cursor) == Spot::Merge {
            v.push(key_line("a", "ajouter un bloc **après** le parallèle"));
            v.push(key_line("i", "ajouter à la fin de la première branche"));
            v.push(key_line("←→", "la fin de la 1re branche · le bloc qui suit le parallèle"));
            v.push(key_line("↑", "le `◆` : mode, coupures, `+` branche, `x` défaire, `[ ]` déplacer"));
            v.push(Line::raw(""));
            v.push(Line::styled(
                " Les branches se somment ici. Ses réglages propres (niveau, mix) viendront.",
                Style::new().fg(Color::DarkGray),
            ));
            return ("Chaîne · sortie du parallèle (◇)".into(), v);
        }
        if s.fx.focus == FxFocus::Params {
            v.push(key_line("↑↓", "choisir le réglage"));
            v.push(key_line("←→", "changer la valeur (Maj : pas fin)"));
            v.push(key_line("Entrée / Échap", "revenir à la chaîne"));
            return ("Chaîne · réglages".into(), v);
        }
        v.push(key_line("←→ ↑↓", "se déplacer : séries et branches"));
        v.push(key_line("Entrée", "réglages · sur un nam ou un ir : choisir dans la bibliothèque"));
        v.push(key_line("Tab", "réglages de l'élément sous le curseur"));
        v.push(key_line("a / i", "ajouter un bloc après / avant le curseur (catalogue)"));
        v.push(key_line("s / +", "mettre en parallèle · une branche de plus"));
        v.push(key_line("x", "retirer le bloc, la branche, ou défaire le parallèle"));
        v.push(key_line("[ ] / { }", "déplacer dans la chaîne · changer de branche"));
        v.push(key_line("b", "couper ou remettre le bloc"));
        v.push(key_line("m / o / p", "muet · solo · polarité de la branche"));
        v.push(key_line("◇", "la sortie du parallèle : a y ajoute après lui (↑ : le ◆)"));
        v.push(key_line("v / r", "vumètres · avec ou sans effets"));
        v.push(key_line(", / .", "volume du preset (1 dB) : après l'alignement, donc il compte"));
        if s.level_match {
            // ce n'est pas une touche, mais c'est la question qu'on se pose devant un gain qui ne
            // fait rien : l'étage grisé en bout de synoptique (#178)
            v.push(key_line(
                "niveau DI",
                "volume aligné sur le DI : l'étage grisé annule les blocs gain (reste la saturation)",
            ));
        }
        v.push(key_line("S / P", "enregistrer la chaîne en preset · ouvrir la banque"));
        v.push(key_line("n", "nouvelle chaîne vierge (le preset courant est détaché)"));
        v.push(key_line("PgDn / PgUp", "preset suivant / précédent (la banque en boucle)"));
        v.push(key_line("t", "déplier le TOML du preset"));
        v.push(key_line("1", "revenir à la Bibliothèque (le banc d'essai)"));
        v.push(key_line("q", "quitter — Échap ne quitte jamais, il remonte d'un niveau"));
        return ("Chaîne · le synoptique est l'éditeur".into(), v);
    }
    // Bibliothèque
    v.push(key_line("↑↓ PgUp PgDn", "écouter au banc d'essai (basse › modèle › IR, sans effets)"));
    v.push(key_line("Entrée", "mettre le son du banc dans la chaîne"));
    v.push(key_line("Tab / Maj+Tab", "onglet suivant / précédent"));
    v.push(key_line("→ ←", "onglet voisin (ils ne mènent pas à la Chaîne)"));
    match s.tab {
        Tab::Models => {
            v.push(key_line("e", "modifier et calibrer le modèle (type, entrée, IR, nom)"));
            v.push(key_line("c / r", "épingler une référence (◆) · A/B ton choix ↔ référence"));
            v.push(key_line("b", "avec ou sans IR"));
            v.push(key_line("/", "filtrer (Entrée valide, Échap efface)"));
        }
        Tab::Irs => {
            v.push(key_line("espace", "ajouter ou retirer l'IR du mélange"));
            v.push(key_line("c / r", "épingler une référence (◆) · A/B ton choix ↔ référence"));
            v.push(key_line("b", "avec ou sans IR"));
            v.push(key_line("/", "filtrer (Entrée valide, Échap efface)"));
        }
        Tab::Blend => {
            v.push(key_line("+ / -", "gain de l'IR choisie (pas de 1 dB)"));
            v.push(key_line("p", "inverser sa polarité"));
            v.push(key_line("a", "alignement automatique des retards"));
            v.push(key_line("x", "retirer l'IR du mélange"));
            v.push(key_line("s", "sauver le mélange dans library/cabs/"));
        }
    }
    v.push(key_line("z", "panneau Bandes : ce que la chaîne fait aux 7 zones (sub … air)"));
    v.push(key_line("S / P", "enregistrer la chaîne en preset · ouvrir la banque"));
    v.push(key_line("n", "nouvelle chaîne vierge (on passe à la Chaîne pour la bâtir)"));
    v.push(key_line("2", "aller à la Chaîne (effets, parallèles)"));
    v.push(key_line("q", "quitter — Échap ne quitte jamais, il remonte d'un niveau"));
    let tab = match s.tab {
        Tab::Models => "Modèles",
        Tab::Irs => "IR",
        Tab::Blend => "Mélange",
    };
    (format!("Bibliothèque · {tab} (banc d'essai)"), v)
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
