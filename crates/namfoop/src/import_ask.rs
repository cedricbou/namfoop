//! La question d'un élément changé des deux côtés : les deux versions (la machine, la date, le
//! résumé), ce qui diffère, puis une touche — `i` garder celui d'ici, `v` prendre celui de la
//! valise, `I` / `V` pour tous les suivants, `a` abandonner l'import. **Toutes** les questions
//! passent avant la première écriture : `a` à la dernière n'a rien écrit, comme à la première.

use std::collections::BTreeMap;

use crate::exchange::{self as xchg, Ui};
use crate::theme;
use library::exchange::describe::{differs, summary, Version};
use library::exchange::import::{Line, Side};
use library::exchange::{Element, Family};
use library::Library;

/// Les deux bibliothèques, pour relire chaque version, et les machines qui les ont écrites.
pub struct Sides<'a> {
    pub here: Library,
    pub there: Library,
    /// Le nom de cette machine.
    pub machine: &'a str,
    /// La machine qui a écrit la valise en dernier.
    pub writer: &'a str,
}

/// Les réponses, élément par élément ; `None` : l'import est abandonné.
pub fn ask(ui: &mut dyn Ui, conflicts: &[&Line], s: &Sides) -> Option<BTreeMap<String, Side>> {
    let mut out = BTreeMap::new();
    let mut all: Option<Side> = None;
    for (i, l) in conflicts.iter().enumerate() {
        if let Some(side) = all {
            out.insert(l.path.clone(), side);
            continue;
        }
        for line in card(l, i + 1, conflicts.len(), s) {
            println!("{line}");
        }
        let side = loop {
            match ui.key("   ") {
                Some('i') => break Side::Here,
                Some('v') => break Side::Suitcase,
                Some('I') => {
                    all = Some(Side::Here);
                    break Side::Here;
                }
                Some('V') => {
                    all = Some(Side::Suitcase);
                    break Side::Suitcase;
                }
                Some('a' | 'A') | None => return None,
                Some(_) => continue,
            }
        };
        out.insert(l.path.clone(), side);
    }
    Some(out)
}

/// La date d'une version d'ici : celle d'une calibration, sinon celle du fichier.
fn here_date(e: &Element, lib: &Library) -> Option<toml::value::Datetime> {
    if e.kind == Family::Calibration {
        let text = std::fs::read_to_string(lib.root.join(e.file())).ok()?;
        let i: library::instrument::Instrument = toml::from_str(&text).ok()?;
        let card = e.path.split_once('#')?.1;
        return i.calibration.get(card)?.date;
    }
    let m = std::fs::metadata(lib.root.join(e.file())).and_then(|m| m.modified()).ok()?;
    Some(xchg::local(m))
}

/// Ce que la question montre d'un élément.
pub fn card(l: &Line, n: usize, of: usize, s: &Sides) -> Vec<String> {
    let e = l.element();
    let mut out = vec![
        String::new(),
        format!(" {} {n}/{of} · {}  {}", theme::paint("à trancher", theme::warn()), e.kind.noun(1), e.name),
        String::new(),
    ];
    let date = |d: Option<toml::value::Datetime>| d.map(|d| xchg::short_date(&d, true)).unwrap_or_default();
    let here = l.here.as_ref().map(|e| Version { element: e, library: &s.here });
    let there = l.there.as_ref().map(|e| Version { element: e, library: &s.there });
    let row = |who: &str, machine: &str, when: String, what: String| {
        format!("   {who:<9} {machine:<10} {when:<18} {what}").trim_end().to_string()
    };
    out.push(match &here {
        Some(v) => row("ici", s.machine, date(here_date(v.element, &s.here)), summary(v)),
        None => row("ici", s.machine, String::new(), format!("supprimé sur {}", s.machine)),
    });
    out.push(match &there {
        Some(v) => {
            let who = v.element.machine.as_deref().unwrap_or(s.writer);
            row("valise", who, date(v.element.date), summary(v))
        }
        None => row("valise", s.writer, String::new(), format!("supprimé sur {}", s.writer)),
    });
    if let (Some(h), Some(t)) = (&here, &there) {
        out.push(format!("   {:<9} {}", "diffère", differs(h, t)));
    }
    out.push(String::new());
    out.push(format!("   {:<33}{}", "i  garder celui d'ici", "I  celui d'ici pour tous les suivants"));
    out.push(format!(
        "   {:<33}{}",
        "v  prendre celui de la valise", "V  celui de la valise pour tous les suivants"
    ));
    out.push(format!("   {}", theme::paint("a  abandonner l'import : rien n'est écrit", theme::danger())));
    out
}
