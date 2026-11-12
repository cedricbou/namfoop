//! Onglet Effets : le **synoptique** de la chaîne (l'éditeur, `ui::synoptic`), les **réglages** de
//! l'élément sous le curseur — ou son contenu et le sélecteur pour un bloc `nam` / `ir`
//! (`ui::picker`) —, la **ligne de commande équivalente**, et la fenêtre du **catalogue** quand on
//! ajoute un bloc (`a` / `i`). Plages, unités et aides viennent de `ParamInfo`.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::super::state::chain::Spot;
use super::super::state::fx::FxFocus;
use super::super::state::State;
use super::{cursor_span, synoptic};

const BAR: usize = 20;

fn title(text: &str, active: bool) -> Span<'static> {
    let style = if active {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::Gray)
    };
    Span::styled(format!(" {text} "), style)
}

/// Barre de position d'un réglage dans sa plage.
fn bar(frac: f32) -> String {
    let n = (frac.clamp(0.0, 1.0) * BAR as f32).round() as usize;
    format!("{}{}", "█".repeat(n), "·".repeat(BAR - n))
}

pub fn render_fx(s: &State, f: &mut Frame, area: Rect) {
    // le sélecteur prend toute la hauteur qui reste sous le synoptique (4 lignes de liste au
    // moins, § 7) ; sinon, la disposition habituelle (réglages, ligne de commande)
    let [chain, params, cmd] = if s.picker.is_some() {
        let syn = synoptic::height(s).min(area.height / 2).max(3) + 2;
        Layout::vertical([Constraint::Length(syn), Constraint::Min(7), Constraint::Length(0)]).areas(area)
    } else {
        Layout::vertical([Constraint::Min(6), Constraint::Length(9), Constraint::Length(3)]).areas(area)
    };

    // --- le synoptique, qui est l'éditeur ---
    let syn = synoptic::build(s);
    let inner = chain.width.saturating_sub(2) as usize;
    let scroll = syn.scroll_for(inner);
    let mut lines = syn.lines(inner, scroll);
    if scroll > 0 {
        lines.push(Line::styled(
            format!(" ← {scroll} colonnes à gauche (terminal étroit)"),
            Style::new().fg(Color::DarkGray),
        ));
    }
    let (head, warn) = synoptic::title(&s.fx.chain);
    let head = if s.fx.bypass_all { "♪ CHAÎNE — SANS EFFETS (r)".to_string() } else { head };
    let mut block = Block::bordered().title(title(&head, s.fx.focus == FxFocus::Chain));
    if let Some(w) = warn.filter(|_| !s.fx.bypass_all) {
        block = block.title(Span::styled(w, Style::new().fg(Color::Yellow)));
    }
    f.render_widget(Paragraph::new(lines).block(block), chain);

    // --- le sélecteur sur place, s'il est ouvert (§ 5) ---
    if s.picker.is_some() {
        let inner = params.height.saturating_sub(3) as usize; // bordures + ligne de filtre
        let lines = super::picker::picker_lines(s, inner);
        f.render_widget(
            Paragraph::new(lines).block(
                Block::bordered().border_style(Style::new().fg(Color::Yellow)).title(Span::styled(
                    format!(" {} ", s.pick_title()),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )),
            ),
            params,
        );
        return;
    }

    // --- le contenu du bloc `nam` / `ir` sous le curseur, ou les réglages de l'effet ---
    if let Some(lines) = super::picker::block_info(s) {
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(title(&s.fields_title(), false))),
            params,
        );
        render_command_line(s, f, cmd);
        return;
    }
    let fields = s.fields();
    let mut lines: Vec<Line> = Vec::new();
    for (k, field) in fields.iter().enumerate() {
        let on = s.fx.focus == FxFocus::Params && k == s.fx.param;
        let (value, frac) = s.field_value(*field);
        lines.push(Line::from(vec![
            cursor_span(on),
            Span::raw(format!("{:<10}", field.name())),
            Span::styled(bar(frac), Style::new().fg(if on { Color::Cyan } else { Color::Blue })),
            Span::raw(format!(" {value:>9}")),
        ]));
    }
    match fields.get(s.fx.param) {
        Some(field) => {
            lines.push(Line::styled(format!("   {}", field.help()), Style::new().fg(Color::DarkGray)))
        }
        None => lines.push(Line::styled(
            match s.fx.chain.spot(s.fx.cursor) {
                Spot::Empty => "   Chaîne vide : a ouvre le catalogue (nam, ir, effets).".to_string(),
                _ => "   Rien à régler ici.".to_string(),
            },
            Style::new().fg(Color::DarkGray),
        )),
    }
    // ce que le parallèle propose en plus, sous ses réglages
    if s.fx.chain.spot(s.fx.cursor) == Spot::Split {
        lines.push(Line::styled(
            "   Entrée pour régler · + ajoute une branche · x défait le parallèle (garde la branche de l'ampli)",
            Style::new().fg(Color::DarkGray),
        ));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(title(&s.fields_title(), s.fx.focus == FxFocus::Params))),
        params,
    );

    render_command_line(s, f, cmd);

    // --- la fenêtre du catalogue, par-dessus tout l'écran ---
    if s.fx.catalog.is_some() {
        render_catalog(s, f.area(), f);
    }
}

/// La ligne de commande équivalente, ou l'invitation à enregistrer un preset.
fn render_command_line(s: &State, f: &mut Frame, cmd: Rect) {
    let (text, style) = match s.fx.chain.fits_command_line() {
        true => {
            let args = s.fx_args();
            (if args.is_empty() { "(aucun effet)".to_string() } else { args }, Style::new())
        }
        false => (
            "chaîne à enregistrer en preset (S) : les branches ne se décrivent pas en ligne de commande"
                .to_string(),
            Style::new().fg(Color::Yellow),
        ),
    };
    f.render_widget(
        Paragraph::new(Line::styled(text, style))
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(title("à copier dans namfoop run", false))),
        cmd,
    );
}

/// Fenêtre du catalogue : titre, filtre tapé au clavier, liste filtrée.
fn render_catalog(s: &State, body: Rect, f: &mut Frame) {
    let Some(c) = s.fx.catalog.as_ref() else { return };
    let visible = c.visible();
    let where_ = if c.before { "avant" } else { "après" };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("ajouter {where_} le curseur"),
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   filtre : "),
            Span::styled(format!("{}▏", c.filter), Style::new().fg(Color::Green)),
        ]),
        Line::raw(""),
    ];
    if visible.is_empty() {
        lines.push(Line::styled("  aucun bloc", Style::new().fg(Color::DarkGray)));
    }
    // la liste défile si le terminal est petit : l'entrée choisie reste visible
    let cur = c.cursor.min(visible.len().saturating_sub(1));
    let shown = super::window(visible.len(), cur, (body.height as usize).saturating_sub(6).max(1));
    let rest = visible.len() - shown.end;
    for i in shown {
        let e = &visible[i];
        let on = i == cur;
        lines.push(Line::from(vec![
            cursor_span(on),
            Span::styled(
                format!("{:<10}", e.kind),
                if on { Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::new() },
            ),
            Span::styled(e.help.to_string(), Style::new().fg(Color::DarkGray)),
        ]));
    }
    if rest > 0 {
        lines.push(Line::styled(format!("   … {rest} de plus"), Style::new().fg(Color::DarkGray)));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "tape pour filtrer · ↑↓ · Entrée ajoute · Échap annule",
        Style::new().fg(Color::DarkGray),
    ));
    let w = (lines.iter().map(|l| l.width()).max().unwrap_or(40) as u16 + 4).min(body.width);
    let h = (lines.len() as u16 + 2).min(body.height);
    let area = Rect {
        x: body.x + (body.width.saturating_sub(w)) / 2,
        y: body.y + (body.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(title("catalogue", true)).style(Style::new().bg(Color::Black))),
        area,
    );
}

/// Touches affichées en bas de l'onglet, selon ce que le curseur désigne (§ 7).
pub fn fx_keys(s: &State) -> String {
    if let Some(p) = s.picker.as_ref() {
        if p.filtering {
            return "tape le filtre   Entrée valider   Échap effacer".into();
        }
        let esc = if p.is_new { "Échap ne rien ajouter" } else { "Échap revenir à l'ancien" };
        return format!(
            "↑↓ essayer dans la chaîne   Entrée garder   {esc}   c actuel / essai                o écouter ce bloc seul   r sans effets   / filtrer"
        );
    }
    if s.fx.catalog.is_some() {
        return "tape pour filtrer   ↑↓ choisir   Entrée ajoute   Échap annule".into();
    }
    if s.fx.focus == FxFocus::Params {
        return "↑↓ réglage   ←→ valeur   Maj+←→ pas fin   Échap retour à la chaîne".into();
    }
    if matches!(s.fx.chain.leaf(s.fx.cursor), Some(super::super::state::chain::Leaf::Nam { .. })) {
        return "←→ série   Entrée choisir le modèle   a/i ajouter   s mettre en parallèle                   [ ] déplacer   x retirer   b couper   r sans effets"
            .into();
    }
    if matches!(s.fx.chain.leaf(s.fx.cursor), Some(super::super::state::chain::Leaf::Ir { .. })) {
        return "←→ série   Entrée choisir l'IR   a/i ajouter   s mettre en parallèle                   [ ] déplacer   x retirer   b couper   r sans effets"
            .into();
    }
    match s.fx.chain.spot(s.fx.cursor) {
        Spot::Empty => "a ajouter un bloc (nam, ir, effets)   Tab onglets   q quitter",
        Spot::Block => "←→ série   a/i ajouter   s mettre en parallèle   [ ] déplacer   x retirer   b couper   v vumètres   Entrée régler",
        Spot::Split => "↓ branches   + branche   Entrée mode et coupures   [ ] déplacer le parallèle   x défaire   a/i ajouter",
        Spot::Header => "↑↓ branche   Entrée gain, phase   m muet   o solo   p phase   + branche   x retirer la branche   a ajouter au début",
        Spot::BranchBlock => "←→ série   ↑↓ branche   { } changer de branche   [ ] déplacer (sort aux extrémités)   a/i ajouter   x retirer   Entrée régler",
    }
    .into()
}
