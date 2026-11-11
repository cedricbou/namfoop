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

use super::super::state::chain::{
    Block, Branch, ChainView, MeterAt, MeterDisplay, MeterKind, Role, FLOOR_DB,
};

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

/// Dessine le synoptique de `view`. `cursor` est l'index du bloc choisi dans la série principale
/// (le curseur 2D arrive avec l'éditeur, #109) ; `None` : aucun.
pub fn build(view: &ChainView, meters: &MeterDisplay, show_meters: bool, cursor: Option<usize>) -> Synoptic {
    let h = view.height();
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

    g.add(0, "IN", io, false, m(MeterAt::Input, MeterKind::Peak, 2, false));
    g.plain(0, " ─ ", line_style());
    g.sync();
    if view.blocks.is_empty() {
        g.add(0, "(chaîne vide : a pour ajouter)", Style::new().fg(Color::DarkGray), true, Vec::new());
        g.plain(0, " ─ ", line_style());
        g.sync();
    }
    for (i, b) in view.blocks.iter().enumerate() {
        match b {
            Block::Leaf(l) => {
                let w = l.label.chars().count().max(3);
                let on_cursor = cursor == Some(i);
                let start = g.len[0];
                g.add(
                    0,
                    &l.label,
                    block_style(l.role, l.on, false),
                    on_cursor,
                    m(MeterAt::Block(i), l.meter, w, !l.on),
                );
                if on_cursor {
                    cursor_cols = Some((start, g.len[0]));
                }
                g.plain(0, " ─ ", line_style());
                g.sync();
            }
            Block::Split(s) => {
                let start = g.len[0];
                g.add(
                    0,
                    "◆",
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    cursor == Some(i),
                    Vec::new(),
                );
                g.plain(0, "─", line_style());
                if cursor == Some(i) {
                    cursor_cols = Some((start, g.len[0]));
                }
                for r in 1..h {
                    g.plain(r, "  ", Style::new());
                }
                split(&mut g, s, i, &m, h);
                g.sync();
                g.plain(0, "─ ", line_style());
                g.sync();
            }
        }
    }
    g.add(0, "OUT", io, false, m(MeterAt::Output, MeterKind::Peak, 3, false));
    Synoptic { grid: g, show_meters, cursor_cols }
}

/// Les branches d'un parallèle : toutes de la même largeur (étiquettes complétées, remplissage
/// `─`, gains alignés à droite).
fn split(
    g: &mut Grid,
    s: &super::super::state::chain::Split,
    i: usize,
    m: &impl Fn(MeterAt, MeterKind, usize, bool) -> Vec<Cell>,
    _h: usize,
) {
    let n = s.branches.len();
    let any_solo = s.branches.iter().any(|b| b.solo);
    let label_w = s.branches.iter().map(|b| b.label.chars().count()).max().unwrap_or(0);
    let body_w = s.branches.iter().map(Branch::content_width).max().unwrap_or(0);
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
        let silent = b.mute || (any_solo && !b.solo);
        let dim = |st: Style| if silent { st.fg(Color::DarkGray) } else { st };
        let (enter, leave) = match (n, k) {
            (1, _) => ("─", "─"),
            (_, 0) => ("┬", "┬"),
            (_, k) if k == n - 1 => ("└", "┘"),
            _ => ("├", "┤"),
        };
        g.add(k, enter, line_style(), false, vbar(k));
        g.plain(k, "─ ", line_style());
        g.add(k, &pad_end(&b.label, label_w), dim(hdr), false, Vec::new());
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
            let w = l.label.chars().count().max(3);
            g.add(
                k,
                &l.label,
                dim(block_style(l.role, l.on, silent)),
                false,
                m(MeterAt::BranchBlock(i, k, j), l.meter, w, silent || !l.on),
            );
        }
        let fill = body_w - Branch::content_width(b) + 1;
        g.plain(k, &format!(" {} ", "─".repeat(fill)), dim(line_style()));
        let gain = b.gain_text();
        g.add(
            k,
            &pad_start(&gain, gain_w),
            dim(hdr),
            false,
            m(MeterAt::BranchSum(i, k), MeterKind::Peak, gain_w, silent),
        );
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

/// Titre du cadre : modèles actifs, parallèles, et l'alerte de charge au-delà du seuil.
pub fn title(view: &ChainView) -> (String, Option<String>) {
    let nam = view.active_nam();
    let text = format!("chaîne — NAM actifs : {nam} · parallèles : {}", view.splits());
    // Seuil provisoire : la maquette alerte dès 2 NAM ; #111 le fixe après mesure sur Pi 5.
    let warn = (nam >= 2).then(|| format!(" ⚠ CPU ≈ ×{nam} (à mesurer sur Pi 5)"));
    (text, warn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::state::chain::{Branch, Leaf, Split};

    fn leaf(label: &str) -> Leaf {
        Leaf { label: label.into(), role: Role::Fx, meter: MeterKind::Peak, on: true }
    }

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

    /// Toutes les lignes d'un parallèle ont la même largeur, et les traits verticaux tombent dans
    /// la même colonne (c'est la règle de rendu de la spécification).
    #[test]
    fn every_row_of_a_parallel_has_the_same_width() {
        let branch = |label: &str, gain: f32, blocks: Vec<Leaf>| Branch {
            label: label.into(),
            gain_db: gain,
            invert: false,
            mute: false,
            solo: false,
            blocks,
        };
        let view = ChainView {
            blocks: vec![
                Block::Leaf(leaf("gate -60")),
                Block::Split(Split {
                    branches: vec![
                        branch("propre", 0.0, vec![]),
                        branch("fuzz", -3.0, vec![leaf("nam·fuzz"), leaf("ir·4x10")]),
                        branch("ampli", -4.0, vec![leaf("nam·svt")]),
                    ],
                }),
                Block::Leaf(leaf("reverb")),
            ],
        };
        let syn = build(&view, &MeterDisplay::default(), false, None);
        let lines = syn.lines(400, 0);
        assert_eq!(lines.len(), 3, "trois branches, pas de ligne de vumètres");
        let widths: Vec<usize> = lines.iter().map(|l| l.width()).collect();
        // les branches du milieu et du bas ont exactement la même largeur ; la première ligne
        // porte en plus ce qui suit le parallèle (ici « ─ reverb ─ OUT »)
        assert_eq!(widths[1], widths[2], "largeurs : {widths:?}");
        // seul `OUT`, tout à la fin, dépasse : chaque bloc remet les lignes à égalité
        assert_eq!(widths[0], widths[1] + "OUT".len(), "largeurs : {widths:?}");
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        // les traits verticaux d'entrée et de sortie sont dans les mêmes colonnes
        let col = |row: usize, ch: char| text[row].chars().position(|c| c == ch);
        assert_eq!(col(0, '┬'), col(1, '├'), "entrée du parallèle alignée");
        assert_eq!(col(1, '├'), col(2, '└'), "entrée du parallèle alignée");
        let last =
            |row: usize, ch: char| text[row].chars().collect::<Vec<_>>().iter().rposition(|c| *c == ch);
        assert_eq!(last(1, '┤'), last(2, '┘'), "sortie du parallèle alignée");
        assert!(text[0].contains("IN ─ gate -60 ─ ◆─┬─ propre"), "{}", text[0]);
        assert!(text[0].ends_with("─ reverb ─ OUT"), "{}", text[0]);
        assert!(text[1].contains("fuzz   ─ nam·fuzz ─ ir·4x10"), "{}", text[1]);
        assert!(text[2].contains("ampli  ─ nam·svt ────"), "{}", text[2]);
        assert!(text[1].contains("-3 dB"), "{}", text[1]);
    }
}
