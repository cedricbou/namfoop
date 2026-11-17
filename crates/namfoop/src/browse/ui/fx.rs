//! Espace **Chaîne** : le **synoptique** de la chaîne (l'éditeur, `ui::synoptic`), les **réglages** de
//! l'élément sous le curseur — ou son contenu et le sélecteur pour un bloc `nam` / `ir`
//! (`ui::picker`) —, la **ligne de commande équivalente**, et la fenêtre du **catalogue** quand on
//! ajoute un bloc (`a` / `i`). Plages, unités et aides viennent de `ParamInfo`.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::super::state::chain::{Leaf, Spot};
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
    // `t` déplie le TOML du preset : le panneau du bas prend la place qu'il faut (#110)
    let bas = if s.show_toml { 14 } else { 4 };
    let [chain, params, cmd] = if s.picker.is_some() {
        let syn = synoptic::height(s).min(area.height / 2).max(3) + 2;
        Layout::vertical([Constraint::Length(syn), Constraint::Min(7), Constraint::Length(0)]).areas(area)
    } else {
        Layout::vertical([Constraint::Min(6), Constraint::Length(9), Constraint::Length(bas)]).areas(area)
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
    let head = match (&s.place, s.fx.bypass_all) {
        // mode pose : le cadre dit ce qu'on place (§ 4)
        (Some(p), _) => format!("♪ CHAÎNE · où poser {} ?", p.label),
        (None, true) => "♪ CHAÎNE — SANS EFFETS (r)".to_string(),
        (None, false) => head,
    };
    // on arrive ici depuis l'espace Set (`2`) : modifier ce son, c'est modifier son preset, donc
    // **tous** les morceaux du set qui s'en servent. `S` enregistre partout (§ 8, #220).
    let head = match s.set_preset_note() {
        Some(note) if s.place.is_none() => format!("{head} · {note}"),
        _ => head,
    };
    let warn = if s.place.is_some() { None } else { warn };
    let mut block = Block::bordered().title(title(&head, s.fx.focus == FxFocus::Chain));
    if let Some(w) = warn.filter(|_| !s.fx.bypass_all) {
        block = block.title(Span::styled(w, Style::new().fg(Color::Yellow)));
    }
    f.render_widget(Paragraph::new(lines).block(block), chain);

    // --- le panneau du bas : le sélecteur, le contenu du bloc, ou les réglages ---
    render_panel(s, f, params, cmd);

    // --- la fenêtre du catalogue, par-dessus tout l'écran : **toujours** dessinée quand elle est
    // ouverte, quel que soit le panneau du bas (#148 : un `return` la sautait sur un bloc nam / ir)
    if s.fx.catalog.is_some() {
        render_catalog(s, f.area(), f);
    }
}

/// Le panneau sous le synoptique : le sélecteur sur place (§ 5), le contenu entier du bloc `nam` /
/// `ir` sous le curseur, ou les réglages de l'effet — et la ligne de commande équivalente.
fn render_panel(s: &State, f: &mut Frame, params: Rect, cmd: Rect) {
    if s.picker.is_some() {
        // le panneau Spectre à droite de la liste, si le terminal est assez large (§ 5)
        let (list, spectrum) = if params.width >= super::SPECTRUM_MIN_WIDTH {
            let [a, b] =
                Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(params);
            (a, Some(b))
        } else {
            (params, None)
        };
        let inner = list.height.saturating_sub(3) as usize; // bordures + ligne de filtre
        let lines = super::picker::picker_lines(s, inner);
        f.render_widget(
            Paragraph::new(lines).block(
                Block::bordered().border_style(Style::new().fg(Color::Yellow)).title(Span::styled(
                    format!(" {} ", s.pick_title()),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )),
            ),
            list,
        );
        if let Some(area) = spectrum {
            super::spectrum::render_picker_spectrum(s, f, area);
        }
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
            lines.push(Line::styled(format!("   {}", s.field_help(*field)), Style::new().fg(Color::DarkGray)))
        }
        None => lines.push(Line::styled(
            match s.fx.chain.spot(s.fx.cursor) {
                Spot::Empty => "   Chaîne vide : a ouvre le catalogue (nam, ir, effets).".to_string(),
                _ => "   Rien à régler ici.".to_string(),
            },
            Style::new().fg(Color::DarkGray),
        )),
    }
    // un `gain` sous le curseur dans une chaîne alignée sur le DI : dire pourquoi la valeur qu'on
    // tourne ne change rien à la sortie (#178)
    if s.gain_cancelled_by_alignment() {
        lines.push(Line::styled(
            "   ⚠ volume aligné sur le DI : ce gain ne change pas la sortie, seulement la saturation",
            Style::new().fg(Color::Yellow),
        ));
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
}

/// Le panneau du bas : la ligne de commande équivalente (et le TOML du preset, replié par défaut
/// — une ligne `▸ preset TOML (t)`, #110).
fn render_command_line(s: &State, f: &mut Frame, cmd: Rect) {
    if s.show_toml {
        let text = match s.preset_toml() {
            Ok(t) => t,
            Err(e) => format!("# {e}"),
        };
        f.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .block(Block::bordered().title(title("▾ preset TOML (t)", true))),
            cmd,
        );
        return;
    }
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
        Paragraph::new(vec![
            Line::styled(text, style),
            Line::styled(" ▸ preset TOML (t)", Style::new().fg(Color::DarkGray)),
        ])
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

/// Touches affichées en bas de l'espace Chaîne, selon ce que le curseur désigne (§ 7). Chaque
/// barre finit par les touches qui permettent d'en **sortir** (`h`, `1`, `q`) : c'est l'aide
/// permanente, l'overlay `h` donne le détail (#150).
/// **La tête de la barre du bas de la Chaîne** : où l'on est dans le synoptique, et donc ce que
/// les flèches vont faire. C'est la seule partie qui dépend du curseur ; tout le reste vient de la
/// table (`state/bindings.rs`) depuis #243 — avant, cette fonction portait aussi une constante
/// `SORTIE` recopiée à la main, sixième copie des mêmes touches.
pub fn chain_head(s: &State) -> String {
    let leaf = s.fx.chain.leaf(s.fx.cursor);
    match (leaf, s.fx.chain.spot(s.fx.cursor)) {
        (Some(Leaf::Nam { .. }), _) => {
            "←→↑↓ se déplacer   Entrée choisir le modèle   a/i ajouter   s parallèle   [ ] déplacer   x retirer   b couper".to_string()
        }
        (Some(Leaf::Ir { .. }), _) => {
            "←→↑↓ se déplacer   Entrée choisir l'IR   a/i ajouter   s parallèle   [ ] déplacer   x retirer   b couper".to_string()
        }
        (_, Spot::Empty) => "a ajouter un bloc (nam, ir, effets)".to_string(),
        (_, Spot::Block) => {
            "←→ série   Entrée régler   a/i ajouter   s parallèle   [ ] déplacer   x retirer   b couper".to_string()
        }
        (_, Spot::Split) => {
            "↓ branches   Entrée mode et coupures   + branche   [ ] déplacer   x défaire   a/i ajouter".to_string()
        }
        (_, Spot::Header) => {
            "↑↓ branche   Entrée gain et phase   m muet   o solo   p phase   + branche   x retirer la branche".to_string()
        }
        (_, Spot::BranchBlock) => {
            "←→↑↓ se déplacer   { } changer de branche   [ ] déplacer   Entrée régler   a/i ajouter   x retirer".to_string()
        }
        (_, Spot::Merge) => {
            "←→ se déplacer   a ajouter après le parallèle   i à la fin de la 1re branche   ↑ le parallèle (◆)".to_string()
        }
    }
}
