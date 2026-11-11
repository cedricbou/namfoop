//! Onglet Effets : catalogue | chaîne (pré › [modèle › IR] › post) | réglages de l'effet choisi,
//! puis la ligne de commande équivalente. Tout vient de `ParamInfo` (plages, unités, aides).

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use super::super::state::fx::{FxFocus, Place};
use super::super::state::{IrChoice, State};
use super::cursor_span;

const BAR: usize = 20;

fn title(text: &str, active: bool) -> Span<'static> {
    let style = if active {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::Gray)
    };
    Span::styled(format!(" {text} "), style)
}

/// Barre de valeur entre `min` et `max`.
fn bar(v: f32, min: f32, max: f32) -> String {
    let f = if max > min { ((v - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
    let n = (f * BAR as f32).round() as usize;
    format!("{}{}", "█".repeat(n), "·".repeat(BAR - n))
}

/// Valeur lisible : sans décimales inutiles.
fn num(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn render_fx(s: &State, f: &mut Frame, area: Rect) {
    let fx = &s.fx;
    let [cat, right] = Layout::horizontal([Constraint::Length(30), Constraint::Min(30)]).areas(area);
    let [chain, params, cmd] =
        Layout::vertical([Constraint::Min(5), Constraint::Length(9), Constraint::Length(3)]).areas(right);

    // catalogue
    let lines: Vec<Line> = fx
        .catalog
        .iter()
        .enumerate()
        .map(|(i, e)| {
            Line::from(vec![
                cursor_span(fx.focus == FxFocus::Catalog && i == fx.catalog_cursor),
                Span::raw(e.kind().to_string()),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(title("catalogue (a pré, A post)", fx.focus == FxFocus::Catalog))),
        cat,
    );

    // chaîne
    let mut lines = Vec::new();
    let slot_line = |i: usize| {
        let sl = fx.slot(i).expect("index de la chaîne");
        let style = if !sl.on || fx.bypass_all {
            Style::new().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT)
        } else {
            Style::new()
        };
        let mut v = vec![
            cursor_span(fx.focus != FxFocus::Catalog && i == fx.selected),
            Span::styled(sl.spec.to_string(), style),
        ];
        if !sl.on {
            v.push(Span::styled("  coupé (b)", Style::new().fg(Color::Red)));
        }
        Line::from(v)
    };
    lines.push(Line::styled(" pré (avant le modèle) :", Style::new().fg(Color::DarkGray)));
    lines.extend((0..fx.pre.len()).map(slot_line));
    let model = s.model.map_or("son direct".to_string(), |m| s.models[m].label.chars().take(30).collect());
    let ir = match s.ir {
        IrChoice::None => "sans IR".to_string(),
        IrChoice::Item(i) => s.irs[i].label.chars().take(24).collect(),
        IrChoice::Blend => "mélange".to_string(),
    };
    lines.push(Line::styled(format!("   [{model} › {ir}]"), Style::new().fg(Color::Green)));
    lines.push(Line::styled(" post :", Style::new().fg(Color::DarkGray)));
    lines.extend((fx.pre.len()..fx.len()).map(slot_line));
    let head = if fx.bypass_all {
        "chaîne — SANS EFFETS (r)".to_string()
    } else {
        format!("chaîne : {} effet(s) ([ ] déplacer, x retirer, b couper)", fx.len())
    };
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title(&head, fx.focus == FxFocus::Chain))),
        chain,
    );

    // réglages de l'effet choisi
    let mut lines = Vec::new();
    let mut name = "réglages".to_string();
    if let Some(sl) = fx.selected_slot() {
        let place = match fx.place_of(fx.selected) {
            Some((Place::Pre, _)) => "pré",
            _ => "post",
        };
        name = format!("{} ({place}) : {}", sl.spec.kind(), sl.spec.help());
        for (k, p) in sl.spec.params().iter().enumerate() {
            let v = sl.spec.get(p.name).unwrap_or(p.default);
            let on = fx.focus == FxFocus::Params && k == fx.param;
            lines.push(Line::from(vec![
                cursor_span(on),
                Span::raw(format!("{:<10}", p.name)),
                Span::styled(
                    bar(v, p.min, p.max),
                    Style::new().fg(if on { Color::Cyan } else { Color::Blue }),
                ),
                Span::raw(format!(" {:>7} {}", num(v), p.unit.symbol())),
            ]));
        }
        if let Some(p) = sl.spec.params().get(fx.param) {
            lines.push(Line::styled(
                format!("   {} ({} … {} {})", p.help, num(p.min), num(p.max), p.unit.symbol()),
                Style::new().fg(Color::DarkGray),
            ));
        }
    } else {
        lines.push(Line::raw("   Chaîne vide : choisis un effet dans le catalogue, a (pré) ou A (post)."));
    }
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title(&name, fx.focus == FxFocus::Params))),
        params,
    );

    // ligne de commande équivalente
    let args = fx.args();
    f.render_widget(
        Paragraph::new(Line::raw(if args.is_empty() { "(aucun effet)".to_string() } else { args }))
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(title("à copier dans namfoop run", false))),
        cmd,
    );
}

/// Touches affichées en bas de l'onglet.
pub fn fx_keys(s: &State) -> &'static str {
    match s.fx.focus {
        FxFocus::Catalog => "Tab/Maj+Tab onglets   ↑↓ effet   a ajouter en pré   A en post   → chaîne   ← Mélange   r avec/sans effets   q",
        FxFocus::Chain => {
            "↑↓ choisir   → réglages   [ ] déplacer   x retirer   b couper   a/A ajouter   ← catalogue   r avec/sans effets   q"
        }
        FxFocus::Params => {
            "↑↓ paramètre   ←→ valeur (Maj : fin)   Entrée retour   x retirer   b couper   r avec/sans effets"
        }
    }
}
