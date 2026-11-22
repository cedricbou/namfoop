//! Panneau **Bandes** du banc d'essai : un égaliseur graphique des sept zones nommées
//! (`library::profile::ZONES`), pour lire d'un coup d'œil ce que la chaîne écoutée fait à la basse.
//!
//! Les valeurs viennent de `State::bands` (profils déjà mesurés et mis en cache) : rien n'est
//! calculé ici, et rien ne coûte au thread audio. Ici, seulement le dessin.
//!
//! Échelle **symétrique ±12 dB sur 5 lignes** : 6 dB par ligne, donc 3 dB par demi-bloc — la même
//! résolution que le mini-spectre des listes. Au-delà, la colonne est écrêtée et marquée `▲` / `▼`.

use super::*;
use crate::browse::state::bands::N;

/// Les lignes du graphique, du haut vers le bas : [+7,+12], [+1,+6], l'axe, [-1,-6], [-7,-12].
pub(super) const ROWS: usize = 5;
/// Ce que vaut une ligne, en dB.
const PER_ROW: f32 = 6.0;
/// Au-delà, la colonne est écrêtée.
const MAX_DB: f32 = PER_ROW * 2.0;
/// Hauteur du panneau, bordures comprises : 5 lignes de graphique, les noms, les valeurs.
pub(super) const HEIGHT: u16 = ROWS as u16 + 4;
/// Largeur d'une colonne : deux caractères de barre, deux de blanc — sinon « pr » colle à « air »
/// et les valeurs se touchent.
const COL: usize = 4;
/// Largeur du panneau, bordures comprises : l'axe (6) et sept colonnes.
#[cfg(test)]
const WIDTH: u16 = 6 + COL as u16 * N as u16 + 2;

/// Noms des zones sous l'axe, dans l'ordre de `library::profile::ZONES`, alignés à droite sur
/// [`COL`] caractères comme les valeurs.
const SHORT: [&str; N] = ["sub", "gr", "bm", "md", "hm", "pr", "air"];
/// Étiquettes de l'axe vertical, du haut vers le bas.
const SCALE: [&str; ROWS] = [" +12 ┤", "  +6 ┤", "   0 ┼", "  -6 ┤", " -12 ┤"];

/// Une colonne, du haut vers le bas : le remplissage de chaque ligne pour `db`.
///
/// La barre pousse depuis l'axe : vers le haut elle se remplit par le bas (`▄`), vers le bas par le
/// haut (`▀`). L'axe lui-même porte `█` dès que la zone sort du neutre, et le trait `─` sinon.
pub(super) fn column(db: f32) -> [char; ROWS] {
    let mut col = [' '; ROWS];
    let full = if db >= 0.0 { '▄' } else { '▀' };
    let clipped = db.abs() > MAX_DB;
    let rows = (db.abs() / PER_ROW).min(2.0);
    for k in 0..2 {
        let fill = (rows - k as f32).clamp(0.0, 1.0);
        let glyph = match fill {
            f if f >= 0.75 => '█',
            f if f >= 0.25 => full,
            _ => continue,
        };
        // k = 0 : la ligne collée à l'axe ; k = 1 : celle du bord
        let row = if db >= 0.0 { 1 - k } else { 3 + k };
        col[row] = if k == 1 && clipped {
            if db >= 0.0 {
                '▲'
            } else {
                '▼'
            }
        } else {
            glyph
        };
    }
    col[2] = if db.abs() >= 1.0 { '█' } else { '─' };
    col
}

/// La couleur d'une zone : le même code que les vumètres (vert, jaune, rouge).
fn tint(db: f32) -> Color {
    match db.abs() {
        d if d <= 3.0 => theme::delta_ok(),
        d if d <= 8.0 => theme::delta_warn(),
        _ => theme::delta_far(),
    }
}

/// La valeur d'une zone (`0` plutôt que `+0`), alignée à droite par l'appelant.
fn value(db: f32) -> String {
    if db.round() == 0.0 {
        "0".into()
    } else {
        format!("{db:+.0}")
    }
}

/// Les lignes du panneau, titre non compris : utilisable sans terminal (tests).
pub(super) fn lines(view: &crate::browse::state::bands::BandView) -> Vec<Line<'static>> {
    let cols: Vec<[char; ROWS]> = view.db.iter().map(|d| column(*d)).collect();
    let mut out = Vec::with_capacity(ROWS + 2);
    let bar = COL / 2;
    for r in 0..ROWS {
        let mut spans = vec![Span::styled(SCALE[r], Style::new().fg(theme::dim()))];
        for (k, c) in cols.iter().enumerate() {
            // la ligne de l'axe se prolonge entre les colonnes, les autres restent vides
            let (glyph, style) = match c[r] {
                '─' => ("─".repeat(bar), Style::new().fg(theme::dim())),
                ' ' => (" ".repeat(bar), Style::new()),
                g => (g.to_string().repeat(bar), Style::new().fg(tint(view.db[k]))),
            };
            let gap = if r == 2 { "─" } else { " " }.repeat(COL - bar);
            spans.push(Span::styled(glyph, style));
            spans.push(Span::styled(gap, Style::new().fg(theme::dim())));
        }
        out.push(Line::from(spans));
    }
    let mut names = vec![Span::raw("      ")];
    let mut vals = vec![Span::raw("      ")];
    for (k, z) in SHORT.iter().enumerate() {
        names.push(Span::styled(format!("{z:^COL$}"), Style::new().fg(theme::dim())));
        vals.push(Span::styled(format!("{:^COL$}", value(view.db[k])), Style::new().fg(tint(view.db[k]))));
    }
    vals.push(Span::styled("  dB".to_string(), Style::new().fg(theme::dim())));
    out.push(Line::from(names));
    out.push(Line::from(vals));
    out
}

/// Dessine le panneau dans `area` (il a sa bordure : il vit aussi bien sous la courbe du Spectre
/// que sous la liste du banc).
pub(super) fn render(s: &State, f: &mut Frame, area: ratatui::layout::Rect) {
    let view = s.bands();
    let title = match &view {
        Some(v) => format!(" Bandes — {} ", v.title),
        None => " Bandes ".to_string(),
    };
    let body = match &view {
        Some(v) => lines(v),
        None => vec![Line::styled(
            " rien à montrer : son direct, ou profil non mesuré",
            Style::new().fg(theme::dim()),
        )],
    };
    f.render_widget(
        Paragraph::new(body).block(Block::bordered().title(Span::styled(
            title,
            Style::new().fg(if view.as_ref().is_some_and(|v| v.diff) {
                theme::edit()
            } else {
                theme::inherit()
            }),
        ))),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La barre pousse depuis l'axe, se remplit par demi-blocs de 3 dB, et s'écrête à ±12 dB.
    #[test]
    fn a_column_grows_from_the_axis_and_is_clipped_at_12_db() {
        // neutre : l'axe seul
        assert_eq!(column(0.0), [' ', ' ', '─', ' ', ' ']);
        assert_eq!(column(0.4), [' ', ' ', '─', ' ', ' ']);
        // +6 dB : l'axe s'allume et la première ligne au-dessus est pleine
        assert_eq!(column(6.0), [' ', '█', '█', ' ', ' ']);
        // +3 et +4 dB : un demi-bloc, posé par le bas (3 dB par demi-bloc)
        assert_eq!(column(3.0), [' ', '▄', '█', ' ', ' ']);
        assert_eq!(column(4.0), [' ', '▄', '█', ' ', ' ']);
        // -3 dB : un demi-bloc, posé par le haut
        assert_eq!(column(-3.0), [' ', ' ', '█', '▀', ' ']);
        // -9 dB : la première ligne pleine, la deuxième à moitié
        assert_eq!(column(-9.0), [' ', ' ', '█', '█', '▀']);
        // au-delà de ±12 dB : la flèche le dit
        assert_eq!(column(18.0), ['▲', '█', '█', ' ', ' ']);
        assert_eq!(column(-18.0), [' ', ' ', '█', '█', '▼']);
        // juste à la limite : plein, sans flèche
        assert_eq!(column(12.0), ['█', '█', '█', ' ', ' ']);
    }

    /// Le panneau rend 7 lignes : 5 de graphique, les noms, les valeurs.
    #[test]
    fn the_panel_prints_seven_zones_with_their_values() {
        let view = crate::browse::state::bands::BandView {
            db: [4.2, 1.8, 0.4, -3.9, 0.6, 2.9, -6.8],
            title: "ce que la chaîne fait à ta basse".into(),
            diff: false,
        };
        let text: Vec<String> = lines(&view).iter().map(|l| l.to_string()).collect();
        assert_eq!(text.len(), ROWS + 2);
        assert!(text[2].starts_with("   0 ┼"), "l'axe : {}", text[2]);
        assert!(text[ROWS].contains("sub") && text[ROWS].contains("air"), "{}", text[ROWS]);
        assert!(text[ROWS + 1].contains(" +4 ") && text[ROWS + 1].contains(" -7 "), "{}", text[ROWS + 1]);
        assert!(text[ROWS + 1].contains(" 0  "), "0,4 dB s'écrit 0 : {}", text[ROWS + 1]);
        // la largeur annoncée est celle qu'on dessine
        assert_eq!(text[2].chars().count() + 2, WIDTH as usize, "{}", text[2]);
    }
}
