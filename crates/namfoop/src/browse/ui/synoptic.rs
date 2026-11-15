//! Synoptique de la chaîne : une ligne de blocs par branche, une ligne de vumètres au-dessus de
//! chacune (`docs/design/browse-chaine.md` § 2 et 3, maquette validée).
//!
//! La **largeur de toutes les lignes d'un parallèle est la même** : les étiquettes de branche sont
//! complétées à droite, les blocs suivis de `─` jusqu'à la colonne des gains, les gains alignés à
//! droite. Après chaque bloc de la série principale, toutes les lignes sont remises à la même
//! longueur ([`Grid::sync`]) : les traits verticaux tombent donc dans la même colonne.
//!
//! Trop large pour le terminal : défilement horizontal qui garde le bloc choisi visible, sans
//! retour à la ligne.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::super::state::chain::{Block, Chain, Cursor, MeterKind, Role, Split};
use super::super::state::meters::{MeterAt, FLOOR_DB};
use super::super::state::State;

/// Huitièmes de caractère, pour la fin d'une barre.
const EIGHTHS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
/// Réduction de gain qui remplit toute la barre du comp.
const COMP_FULL_DB: f32 = 20.0;

fn line_style() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// Un morceau de ligne : son texte, son style, et s'il porte le curseur.
#[derive(Clone)]
struct Cell {
    text: String,
    style: Style,
    cursor: bool,
}

/// Les lignes du synoptique en construction : `rows[r]` les blocs de la branche `r`,
/// `meters[r]` la ligne de vumètres au-dessus.
struct Grid {
    rows: Vec<Vec<Cell>>,
    meters: Vec<Vec<Cell>>,
    len: Vec<usize>,
}

impl Grid {
    fn new(height: usize) -> Self {
        Self { rows: vec![Vec::new(); height], meters: vec![Vec::new(); height], len: vec![0; height] }
    }

    /// Ajoute du texte à la ligne `r`, et au-dessus soit la barre donnée (tronquée à la largeur du
    /// texte), soit des espaces : les deux lignes restent donc colonne à colonne.
    fn add(&mut self, r: usize, text: &str, style: Style, cursor: bool, meter: Vec<Cell>) {
        let w = text.chars().count();
        self.len[r] += w;
        self.rows[r].push(Cell { text: text.to_string(), style, cursor });
        let mut used = 0;
        for m in meter {
            if used >= w {
                break;
            }
            let t: String = m.text.chars().take(w - used).collect();
            used += t.chars().count();
            self.meters[r].push(Cell { text: t, style: m.style, cursor });
        }
        if used < w {
            self.meters[r].push(Cell { text: " ".repeat(w - used), style: Style::new(), cursor: false });
        }
    }

    fn plain(&mut self, r: usize, text: &str, style: Style) {
        self.add(r, text, style, false, Vec::new());
    }

    /// Remet toutes les lignes à la même longueur.
    fn sync(&mut self) {
        let max = self.len.iter().copied().max().unwrap_or(0);
        for r in 0..self.rows.len() {
            if self.len[r] < max {
                let pad = " ".repeat(max - self.len[r]);
                self.plain(r, &pad, Style::new());
            }
        }
    }
}

/// Barre de vumètre sur `w` caractères, de [`FLOOR_DB`] à 0 dBFS, en huitièmes de caractère.
/// `comp` : la réduction de gain remplit la barre en orange depuis la droite. `gate` fermé : barre
/// pleine rouge sombre. Bloc coupé, branche muette ou hors solo : barre sombre.
fn meter_cells(db: f32, action: f32, kind: MeterKind, w: usize, dim: bool) -> Vec<Cell> {
    let cell = |text: String, style: Style| Cell { text, style, cursor: false };
    if !dim && kind == MeterKind::Gate && action > 0.5 {
        return vec![cell("█".repeat(w), Style::new().fg(Color::Red).add_modifier(Modifier::DIM))];
    }
    let f = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0) * w as f32;
    let full = f.floor() as usize;
    let part = ((f - full as f32) * 8.0).floor() as usize;
    let mut bar = "█".repeat(full);
    if full < w {
        bar += EIGHTHS[part.min(7)];
    }
    let used = bar.chars().count();
    if used < w {
        bar += &" ".repeat(w - used);
    }
    let color = if dim {
        Color::DarkGray
    } else if db > -1.0 {
        Color::Red
    } else if db > -9.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    if !dim && kind == MeterKind::Comp && action > 0.3 {
        let cells = ((action / COMP_FULL_DB * w as f32).ceil() as usize).clamp(1, w);
        let left: String = bar.chars().take(w - cells).collect();
        return vec![
            cell(left, Style::new().fg(color)),
            cell("█".repeat(cells), Style::new().fg(Color::LightRed)),
        ];
    }
    vec![cell(bar, Style::new().fg(color))]
}

fn block_style(role: Role, on: bool, dim: bool) -> Style {
    if !on {
        return Style::new().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT);
    }
    let s = match role {
        Role::Amp => Style::new().fg(Color::Green),
        Role::Fx => Style::new(),
    };
    if dim {
        s.fg(Color::DarkGray)
    } else {
        s
    }
}

/// Ce qu'on vient de dessiner : les lignes, et la plage de colonnes du bloc choisi.
pub struct Synoptic {
    grid: Grid,
    show_meters: bool,
    /// Colonnes du bloc sous le curseur (début, fin), pour le défilement horizontal.
    cursor_cols: Option<(usize, usize)>,
}

/// Dessine le synoptique de la chaîne éditée : une ligne de blocs par branche, une ligne de
/// vumètres au-dessus de chacune.
pub fn build(st: &State) -> Synoptic {
    let shown = st.shown_chain();
    let chain = &shown;
    let meters = &st.meters;
    let show_meters = st.show_meters;
    let cursor = st.fx.cursor;
    let h = chain.height();
    let mut g = Grid::new(h);
    let m = |at: MeterAt, kind: MeterKind, w: usize, dim: bool| -> Vec<Cell> {
        if show_meters {
            meter_cells(meters.db(at), meters.action(at), kind, w, dim)
        } else {
            Vec::new()
        }
    };
    let io = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let mut cursor_cols = None;
    let mark = |g: &Grid, row: usize, start: usize, cols: &mut Option<(usize, usize)>| {
        *cols = Some((start, g.len[row]));
    };

    g.add(0, "IN", io, false, m(MeterAt::Input, MeterKind::Peak, 2, false));
    g.plain(0, " ─ ", line_style());
    g.sync();
    if chain.is_empty() {
        g.add(0, "(chaîne vide : a pour ajouter)", Style::new().fg(Color::DarkGray), true, Vec::new());
        g.plain(0, " ─ ", line_style());
        g.sync();
    }
    for (i, b) in chain.blocks.iter().enumerate() {
        match b {
            Block::Leaf(l) => {
                let label = st.leaf_label(l);
                let w = label.chars().count().max(3);
                let on_cursor = cursor == Cursor::Top(i);
                let start = g.len[0];
                // hors de l'écoute isolée (`o`) : assombri
                let out = st.dimmed(Cursor::Top(i));
                g.add(
                    0,
                    &label,
                    block_style(l.role(), l.on(), out),
                    on_cursor,
                    m(MeterAt::Block(i), l.meter_kind(), w, out || !l.on()),
                );
                if on_cursor {
                    mark(&g, 0, start, &mut cursor_cols);
                }
                g.plain(0, " ─ ", line_style());
                g.sync();
            }
            Block::Split(s) => {
                let start = g.len[0];
                let on_cursor = cursor == Cursor::Top(i);
                g.add(
                    0,
                    "◆",
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    on_cursor,
                    Vec::new(),
                );
                g.plain(0, "─", line_style());
                if on_cursor {
                    mark(&g, 0, start, &mut cursor_cols);
                }
                for r in 1..h {
                    g.plain(r, "  ", Style::new());
                }
                split(&mut g, st, s, i, &m, cursor, &mut cursor_cols);
                g.sync();
                // la **sortie** du parallèle : le `◇` où les branches se somment, et d'où l'on
                // ajoute après lui (#152)
                g.plain(0, "─ ", line_style());
                let on_merge = cursor == Cursor::Merge(i);
                let from = g.len[0];
                g.add(0, "◇", Style::new().fg(Color::Yellow), on_merge, Vec::new());
                if on_merge {
                    cursor_cols = Some((from, g.len[0]));
                }
                g.plain(0, " ─ ", line_style());
                g.sync();
            }
        }
    }
    // L'alignement de niveau sur le DI (`level = "match"`) : ce n'est pas un bloc de la chaîne —
    // ni éditable, ni déplaçable, d'où le gris — mais c'est le **dernier étage réel** avant la
    // sortie, et il annule tout gain posé dans la chaîne. Le cacher laissait croire à un vumètre
    // OUT cassé (#178).
    if let (Some(db), false) = (st.level_align_db(), chain.is_empty()) {
        g.add(0, &format!("niveau DI {db:+.1} dB"), Style::new().fg(Color::DarkGray), false, Vec::new());
        g.plain(0, " ─ ", line_style());
        g.sync();
    }
    // le volume du preset : pas un bloc non plus, mais le réglage qui suit l'alignement (#179)
    if chain.volume_db != 0.0 {
        g.add(
            0,
            &format!("volume {:+.0} dB", chain.volume_db),
            Style::new().fg(Color::DarkGray),
            false,
            Vec::new(),
        );
        g.plain(0, " ─ ", line_style());
        g.sync();
    }
    g.add(0, "OUT", io, false, m(MeterAt::Output, MeterKind::Peak, 3, false));
    Synoptic { grid: g, show_meters, cursor_cols }
}

/// Les branches d'un parallèle : toutes de la même largeur (étiquettes complétées, remplissage
/// `─`, gains alignés à droite).
#[allow(clippy::too_many_arguments)]
fn split(
    g: &mut Grid,
    st: &State,
    s: &Split,
    i: usize,
    m: &impl Fn(MeterAt, MeterKind, usize, bool) -> Vec<Cell>,
    cursor: Cursor,
    cursor_cols: &mut Option<(usize, usize)>,
) {
    let n = s.branches.len();
    let label_w = (0..n).map(|k| s.branch_label(k).chars().count()).max().unwrap_or(0);
    let body_w = s.branches.iter().map(|b| st.branch_width(b)).max().unwrap_or(0);
    let gain_w = s.branches.iter().map(|b| b.gain_text().chars().count()).max().unwrap_or(0);
    let hdr = Style::new().fg(Color::Blue);
    let vbar = |k: usize| -> Vec<Cell> {
        if k == 0 {
            Vec::new()
        } else {
            vec![Cell { text: "│".into(), style: line_style(), cursor: false }]
        }
    };
    for (k, b) in s.branches.iter().enumerate() {
        let silent = !s.audible(k);
        let dim = |x: Style| if silent { x.fg(Color::DarkGray) } else { x };
        let (enter, leave) = match (n, k) {
            (1, _) => ("─", "─"),
            (_, 0) => ("┬", "┬"),
            (_, k) if k == n - 1 => ("└", "┘"),
            _ => ("├", "┤"),
        };
        let on_header = cursor == Cursor::Header { top: i, b: k };
        g.add(k, enter, line_style(), false, vbar(k));
        g.plain(k, "─ ", line_style());
        let start = g.len[k];
        g.add(k, &pad_end(&s.branch_label(k), label_w), dim(hdr), on_header, Vec::new());
        g.plain(k, " ─ ", dim(line_style()));
        if b.blocks.is_empty() {
            g.add(
                k,
                "son direct",
                dim(Style::new().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
                false,
                Vec::new(),
            );
        }
        for (j, l) in b.blocks.iter().enumerate() {
            if j > 0 {
                g.plain(k, " ─ ", dim(line_style()));
            }
            let label = st.leaf_label(l);
            let w = label.chars().count().max(3);
            let on_cursor = cursor == Cursor::Block { top: i, b: k, j };
            let from = g.len[k];
            let out = st.dimmed(Cursor::Block { top: i, b: k, j });
            g.add(
                k,
                &label,
                dim(block_style(l.role(), l.on(), silent || out)),
                on_cursor,
                m(MeterAt::BranchBlock(i, k, j), l.meter_kind(), w, silent || out || !l.on()),
            );
            if on_cursor {
                *cursor_cols = Some((from, g.len[k]));
            }
        }
        let fill = body_w - st.branch_width(b) + 1;
        g.plain(k, &format!(" {} ", "─".repeat(fill)), dim(line_style()));
        g.add(
            k,
            &pad_start(&b.gain_text(), gain_w),
            dim(hdr),
            on_header,
            m(MeterAt::BranchSum(i, k), MeterKind::Peak, gain_w, silent),
        );
        if on_header {
            *cursor_cols = Some((start, g.len[k]));
        }
        g.plain(k, " ─", line_style());
        g.add(k, leave, line_style(), false, vbar(k));
    }
}

fn pad_end(s: &str, w: usize) -> String {
    let n = s.chars().count();
    format!("{s}{}", " ".repeat(w.saturating_sub(n)))
}

fn pad_start(s: &str, w: usize) -> String {
    let n = s.chars().count();
    format!("{}{s}", " ".repeat(w.saturating_sub(n)))
}

impl Synoptic {
    /// Largeur totale du synoptique (avant défilement).
    pub fn width(&self) -> usize {
        self.grid.len.iter().copied().max().unwrap_or(0)
    }

    /// Décalage horizontal qui garde le bloc choisi visible dans `width` colonnes.
    pub fn scroll_for(&self, width: usize) -> usize {
        let total = self.width();
        if total <= width {
            return 0;
        }
        match self.cursor_cols {
            Some((start, end)) => {
                // juste assez pour que la fin du bloc (plus deux colonnes de marge) tienne, sans
                // jamais passer avant son début
                let want = (end + 2).saturating_sub(width);
                want.min(start.saturating_sub(2)).min(total - width)
            }
            None => 0,
        }
    }

    /// Les lignes à afficher : pour chaque branche, sa ligne de vumètres (si affichés) puis ses
    /// blocs, coupées à `width` colonnes à partir de `scroll`.
    pub fn lines(&self, width: usize, scroll: usize) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for r in 0..self.grid.rows.len() {
            if self.show_meters {
                out.push(clip(&self.grid.meters[r], width, scroll));
            }
            out.push(clip(&self.grid.rows[r], width, scroll));
        }
        out
    }
}

/// Coupe une ligne de cellules à la fenêtre `[scroll, scroll + width)`.
fn clip(cells: &[Cell], width: usize, scroll: usize) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut col = 0usize;
    for c in cells {
        let n = c.text.chars().count();
        let (from, to) = (col, col + n);
        col = to;
        if to <= scroll || from >= scroll + width {
            continue;
        }
        let skip = scroll.saturating_sub(from);
        let take = (to.min(scroll + width)) - from.max(scroll);
        let text: String = c.text.chars().skip(skip).take(take).collect();
        let style = if c.cursor { c.style.bg(Color::Cyan).fg(Color::Black) } else { c.style };
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}

/// Hauteur du synoptique en lignes : une ligne de blocs par branche, plus sa ligne de vumètres.
pub fn height(st: &State) -> u16 {
    let h = st.fx.chain.height().max(1) as u16;
    if st.show_meters {
        2 * h
    } else {
        h
    }
}

/// Titre du cadre : modèles actifs, parallèles, et l'alerte de charge au-delà du seuil.
pub fn title(chain: &Chain) -> (String, Option<String>) {
    let nam = chain.active_nam();
    // le bandeau de l'espace Chaîne (§ 2 de `docs/design/browse-espaces.md`)
    let text = format!("♪ CHAÎNE — NAM actifs : {nam} · parallèles : {}", chain.splits());
    // Seuil retenu avec l'utilisateur (#111) : on alerte dès 2 modèles actifs et on ne refuse
    // rien. Le coût est linéaire (×N) et 2 modèles restent confortables sur x86, mais le Pi 5
    // n'est pas mesuré (#44 / #45) : prudence tant qu'on ne connaît pas la machine de scène.
    let warn = (nam >= 2).then(|| format!(" ⚠ CPU ≈ ×{nam} (à mesurer sur Pi 5)"));
    (text, warn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_fills_from_the_floor_to_zero_dbfs() {
        let bar = |db: f32| meter_cells(db, 0.0, MeterKind::Peak, 8, false)[0].text.clone();
        assert_eq!(bar(FLOOR_DB), "        ", "au plancher : vide");
        assert_eq!(bar(0.0), "████████", "à 0 dBFS : pleine");
        assert_eq!(bar(-30.0), "████    ", "à mi-chemin");
        // couleurs : vert, puis jaune au-dessus de -9 dB, rouge au-dessus de -1
        let color = |db: f32| meter_cells(db, 0.0, MeterKind::Peak, 8, false)[0].style.fg;
        assert_eq!(color(-20.0), Some(Color::Green));
        assert_eq!(color(-5.0), Some(Color::Yellow));
        assert_eq!(color(-0.5), Some(Color::Red));
        assert_eq!(meter_cells(-5.0, 0.0, MeterKind::Peak, 8, true)[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn comp_fills_from_the_right_and_a_closed_gate_fills_the_bar() {
        // 10 dB de réduction sur 20 : la moitié de la barre, depuis la droite
        let cells = meter_cells(-6.0, 10.0, MeterKind::Comp, 8, false);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[1].text.chars().count(), 4, "{:?}", cells[1].text);
        assert_eq!(cells[0].text.chars().count(), 4);
        // au moins une case dès 0,3 dB (la spécification), rien en dessous
        assert_eq!(meter_cells(-6.0, 0.4, MeterKind::Comp, 8, false)[1].text, "█");
        assert_eq!(meter_cells(-6.0, 0.2, MeterKind::Comp, 8, false).len(), 1);
        // gate fermé : barre pleine
        let cells = meter_cells(-60.0, 1.0, MeterKind::Gate, 6, false);
        assert_eq!(cells[0].text, "██████");
        assert_eq!(cells[0].style.fg, Some(Color::Red));
        // ouvert : barre normale
        assert_eq!(meter_cells(-6.0, 0.0, MeterKind::Gate, 6, false)[0].text, "█████▍");
    }

    /// L'alignement de niveau sur le DI se voit en queue de synoptique, juste avant `OUT` : sans
    /// lui, un bloc `gain` qui ne change rien à la sortie reste inexplicable (#178).
    #[test]
    fn the_di_alignment_is_shown_just_before_out() {
        use crate::browse::state::tests::{fx_block, with_chain};
        let mut s = with_chain(vec![fx_block("gain:db=-40")]);
        s.show_meters = false;
        let text = |s: &State| build(s).lines(400, 0)[0].to_string();
        // tant qu'aucune chaîne n'est installée, il n'y a rien de mesuré à montrer
        assert!(text(&s).ends_with("gain ─ OUT"), "{}", text(&s));
        s.set_chain_meters(dsp::meter::ChainMeters { level_db: Some(40.9), ..Default::default() });
        assert!(text(&s).ends_with("gain ─ niveau DI +40.9 dB ─ OUT"), "{}", text(&s));
        // chaîne vidée : plus d'alignement, et surtout pas celui de la chaîne précédente
        s.fx.chain.blocks.clear();
        assert!(!text(&s).contains("niveau DI"), "{}", text(&s));
    }

    /// Le **volume du preset** se voit lui aussi en bout de synoptique, **après** l'alignement :
    /// c'est l'ordre du signal, et c'est ce qui le distingue d'un bloc `gain` (#179).
    #[test]
    fn the_preset_volume_is_shown_after_the_alignment() {
        use crate::browse::state::tests::{fx_block, with_chain};
        let mut s = with_chain(vec![fx_block("gain:db=-40")]);
        s.show_meters = false;
        let text = |s: &State| build(s).lines(400, 0)[0].to_string();
        assert!(!text(&s).contains("volume"), "au neutre, rien à montrer : {}", text(&s));
        s.fx.chain.volume_db = -6.0;
        s.set_chain_meters(dsp::meter::ChainMeters { level_db: Some(40.9), ..Default::default() });
        assert!(text(&s).ends_with("gain ─ niveau DI +40.9 dB ─ volume -6 dB ─ OUT"), "{}", text(&s));
    }

    /// Toutes les lignes d'un parallèle ont la même largeur, et les traits verticaux tombent dans
    /// la même colonne (c'est la règle de rendu de la spécification).
    #[test]
    fn every_row_of_a_parallel_has_the_same_width() {
        use crate::browse::state::chain::{Block, Branch, Leaf};
        use crate::browse::state::tests::{branch, fx, fx_block, with_chain};
        let amp = |m: usize| Leaf::Nam { model: Some(m), on: true };
        let mut s = with_chain(vec![
            fx_block("gate:threshold=-60"),
            Block::Split(Split {
                mode: crate::browse::state::chain::Mode::Copy,
                freqs: vec![],
                branches: vec![
                    branch("propre", 0.0, vec![]),
                    branch("fuzz", -3.0, vec![amp(0), fx("lpf:freq=3000")]),
                    Branch { blocks: vec![amp(1)], ..branch("ampli", -4.0, vec![]) },
                ],
            }),
            fx_block("reverb"),
        ]);
        s.show_meters = false;
        let syn = build(&s);
        let lines = syn.lines(400, 0);
        assert_eq!(lines.len(), 3, "trois branches, pas de ligne de vumètres");
        let widths: Vec<usize> = lines.iter().map(|l| l.width()).collect();
        assert_eq!(widths[1], widths[2], "largeurs : {widths:?}");
        // seul `OUT`, tout à la fin, dépasse : chaque bloc remet les lignes à égalité
        assert_eq!(widths[0], widths[1] + "OUT".len(), "largeurs : {widths:?}");
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let col = |row: usize, ch: char| text[row].chars().position(|c| c == ch);
        assert_eq!(col(0, '┬'), col(1, '├'), "entrée du parallèle alignée");
        assert_eq!(col(1, '├'), col(2, '└'), "entrée du parallèle alignée");
        let last =
            |row: usize, ch: char| text[row].chars().collect::<Vec<_>>().iter().rposition(|c| *c == ch);
        assert_eq!(last(1, '┤'), last(2, '┘'), "sortie du parallèle alignée");
        assert!(text[0].contains("IN ─ gate -60 ─ ◆─┬─ propre"), "{}", text[0]);
        assert!(text[0].ends_with("─ reverb ─ OUT"), "{}", text[0]);
        assert!(text[1].contains("fuzz   ─ nam·B7K Ultra ─ lpf 3000"), "{}", text[1]);
        assert!(text[1].contains("-3 dB"), "{}", text[1]);
    }
}
