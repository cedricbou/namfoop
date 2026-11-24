//! **Une ligne de grille d'accords** de ChordPro (`{start_of_grid}`, § 2 de
//! `docs/design/prompteur.md`) : `| Em . . . | G . . . | paroles en repère`.
//!
//! La grille compte des **mesures** : c'est ce qu'un bassiste suit sur scène, et c'est ce que la
//! carte des mesures en tire (une ligne dure ses mesures). Les signes sont gardés **tels
//! qu'écrits** — `.`, `/`, `%`, `Am~C`, les barres `||`, `|:`, `:|`, les voltas — parce que
//! l'écran les montre comme le fichier les écrit : on ne déplie pas une reprise, on ne devine
//! pas un accord.
//!
//! Une ligne bancale se lit quand même : des cases en trop restent dans leur mesure, et ce qui suit
//! la dernière barre est la marge droite, comme ChordPro le veut — une mesure sans barre de
//! fermeture s'y retrouve donc, lisible. Seule une ligne **sans aucune mesure** n'est pas une
//! ligne de grille, et elle reste alors du texte.

/// Une mesure : la barre qui l'ouvre, et ses cases (une par temps), telles qu'écrites.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    pub open: String,
    pub cells: Vec<String>,
}

impl Bar {
    /// Le contenu de la mesure, cases séparées d'un espace : `Em . . .`.
    pub fn text(&self) -> String {
        self.cells.join(" ")
    }
}

/// Une ligne de mesures : sa marge gauche, ses mesures, la barre qui la ferme, et le **repère**
/// écrit après (les paroles chantées sur ces mesures).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub left: String,
    pub bars: Vec<Bar>,
    /// La dernière barre, telle qu'écrite.
    pub close: String,
    pub right: String,
}

impl Row {
    /// Le nombre de mesures de la ligne. `%` compte pour une mesure : il en répète une.
    pub fn bar_count(&self) -> u32 {
        self.bars.len() as u32
    }
}

/// Une barre de mesure, quelle que soit sa forme : `|`, `||`, `|.`, `|:`, `:|`, `:|:`, une volta
/// `|2`. Aucun accord ne contient de `|`.
fn is_barline(token: &str) -> bool {
    token.contains('|')
}

/// Lit une ligne de grille. `None` : la ligne n'a aucune mesure, c'est du texte.
pub fn row(line: &str) -> Option<Row> {
    // les mots de la ligne, avec leur place en octets : les marges se recopient telles quelles
    let mut tokens: Vec<(usize, &str)> = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(s)) => {
                tokens.push((s, &line[s..i]));
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(s) = start {
        tokens.push((s, &line[s..]));
    }
    let first = tokens.iter().position(|(_, t)| is_barline(t))?;
    let last = tokens.iter().rposition(|(_, t)| is_barline(t))?;
    let mut out = Row {
        left: line[..tokens[first].0].trim().to_string(),
        right: {
            let (at, t) = tokens[last];
            line[at + t.len()..].trim().to_string()
        },
        ..Default::default()
    };
    let mut open = tokens[first].1.to_string();
    let mut cells: Vec<String> = Vec::new();
    for (_, t) in &tokens[first + 1..=last] {
        if is_barline(t) {
            if cells.is_empty() {
                // deux barres collées (`|| |`) : une seule barre, rien à compter entre elles
                open.push_str(t);
            } else {
                out.bars.push(Bar { open: std::mem::take(&mut open), cells: std::mem::take(&mut cells) });
                open = t.to_string();
            }
        } else {
            cells.push(t.to_string());
        }
    }
    // le dernier mot lu est une barre : c'est elle qui ferme la ligne
    out.close = open;
    (!out.bars.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(r: &Row) -> Vec<String> {
        r.bars.iter().map(Bar::text).collect()
    }

    /// Une ligne du fichier de référence : quatre mesures, le repère après la dernière barre.
    #[test]
    fn a_row_has_its_bars_and_its_cue() {
        let r = row("| Em . . . | Em . . . | G . . . | D . . . | La lampe tourne / Sur le quai").unwrap();
        assert_eq!(cells(&r), vec!["Em . . .", "Em . . .", "G . . .", "D . . ."]);
        assert_eq!(r.bar_count(), 4);
        assert_eq!(r.right, "La lampe tourne / Sur le quai");
        assert_eq!((r.left.as_str(), r.close.as_str()), ("", "|"));
    }

    /// Les signes de la grille restent tels qu'écrits : `%` compte une mesure, `~` partage une
    /// case, les reprises ne se déplient pas, le texte avant la première barre est en marge.
    #[test]
    fn signs_are_kept_as_written() {
        let r = row("x2 |: Am~C / . . | % | G . D . :| fin").unwrap();
        assert_eq!(r.left, "x2");
        assert_eq!(cells(&r), vec!["Am~C / . .", "%", "G . D ."]);
        assert_eq!(r.bars[0].open, "|:");
        assert_eq!(r.close, ":|");
        assert_eq!(r.right, "fin");
        assert_eq!(r.bar_count(), 3, "% compte pour une mesure");
    }

    /// Une ligne **bancale** se lit quand même ; une ligne sans mesure est du texte.
    #[test]
    fn a_broken_row_still_reads() {
        let r = row("| Em . . . . . | G").unwrap();
        assert_eq!(cells(&r), vec!["Em . . . . ."], "cases en trop");
        assert_eq!(r.right, "G", "après la dernière barre, c'est la marge : la règle de ChordPro");
        let r = row("| Em . . . | G . . .").unwrap();
        assert_eq!(cells(&r), vec!["Em . . ."]);
        assert_eq!(row("| Em . . ."), None, "une seule barre : pas de mesure, la ligne est du texte");
        let r = row("|| G . |").unwrap();
        assert_eq!((r.bars[0].open.as_str(), r.bar_count()), ("||", 1));
        assert_eq!(row("juste des paroles"), None);
        assert_eq!(row("| |"), None, "des barres sans mesure : rien à compter");
        assert_eq!(row("|"), None);
    }
}
