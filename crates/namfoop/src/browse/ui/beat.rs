//! **La bande de battue** (§ 11 de `docs/design/set.md`), les deux lignes en tête du cadre du
//! morceau : les temps de la mesure, le tempo, où l'on en est, et ce qui vient.
//!
//! Les **faits** viennent de `state::set::beat` ([`State::band`]) : ici il n'y a que la mise en
//! page et les couleurs — `state` décide, `ui` dessine.

use crate::theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::super::state::set::beat::Band;
use super::super::state::State;

// ------------------------------------------------------- la bande de battue (§ 11)

/// La **bande de battue** : deux lignes en tête du cadre du morceau. Les faits viennent de
/// `state::set::beat` ([`State::band`]) ; ici il n'y a que la mise en page et les couleurs.
///
/// Les couleurs sont celles du § 11, et elles sont les mêmes partout dans l'écran : le **temps 1
/// en jaune**, les autres temps en **vert**, les subdivisions en **points gris**. « Allumé » est
/// une **inversion** et non une autre couleur, pour que le temps qui sonne reste reconnaissable
/// comme temps 1 ou comme temps 3.
pub(super) fn band_lines(s: &State, width: usize) -> Vec<Line<'static>> {
    let Some(band) = s.band() else { return vec![Line::raw(""), Line::raw("")] };
    vec![band_beats(&band, width), band_next(s, &band, width)]
}

/// La 1ʳᵉ ligne : les temps de la mesure, le tempo, la mesure, où l'on en est, et `⏵` / `⏸`.
///
/// **Rien n'est tronqué : quelque chose cède.** Sur 80 colonnes il reste moins de 50 caractères à
/// cette ligne, et une bande coupée au milieu de `⏸ Espa` ne se lit pas. Ce qui cède, dans cet
/// ordre : le rappel de la touche après `⏸` (la barre du bas la donne), l'étiquette de la section
/// (le texte en dessous la marque aussi), puis la mesure courante. Ce qui ne cède **jamais** : les
/// temps de la mesure — c'est la **largeur des cases** qui se serre, pour qu'un 7/8 montre ses
/// sept croches même à l'étroit.
fn band_beats(band: &Band, width: usize) -> Line<'static> {
    let starts = band.meter.starts();
    let head = format!("♩{:.0} {}", band.tempo_bpm, meter_label(&band.meter));
    let label = band.label.to_uppercase();
    // la mesure se compte dans le **groupe** de sections de même nom, celui que le prompteur montre
    let bars = band.group_bars.map(|n| format!("/{n}")).unwrap_or_default();
    let place_min = format!("mes. {}{bars}", band.group_bar + 1);
    // **sans étiquette, la mesure reste** : un morceau sans prompteur a sa bande, et ses mesures
    // comptées, c'est tout ce qu'elle a à dire (§ 11)
    let place_full = if label.is_empty() { place_min.clone() } else { format!("{place_min} {label}") };
    let state = if band.running { "⏵" } else { "⏸" };
    let hint = if band.running { "" } else { " Espace" };
    let ticks = band.meter.ticks().max(1) as usize;
    let len = |s: &str| s.chars().count();
    // le tour de la place : un blanc, les cases, le tempo, la position, l'état
    let room = |cell: usize, place: &str, hint: &str| {
        1 + cell * ticks
            + 2
            + len(&head)
            + if place.is_empty() { 0 } else { 3 + len(place) }
            + 2
            + len(state)
            + len(hint)
    };
    let choices = [
        (place_full.as_str(), hint),
        (place_full.as_str(), ""),
        (place_min.as_str(), hint),
        (place_min.as_str(), ""),
        ("", ""),
    ];
    let (cell, place, hint) = choices
        .iter()
        .find_map(|(place, hint)| {
            let cell = [7usize, 5, 4, 3, 2].into_iter().find(|c| room(*c, place, hint) <= width)?;
            Some((cell, *place, *hint))
        })
        // plus rien ne tient : les temps restent, c'est eux qu'on regarde en jouant
        .unwrap_or((2, "", ""));
    let mut spans = vec![Span::raw(" ")];
    for t in 0..band.meter.ticks() {
        let beat = starts.iter().position(|x| *x == t);
        let text = match beat {
            Some(i) => format!("{:<cell$}", format!(" {}", i + 1), cell = cell),
            None => format!("{:<cell$}", " ·", cell = cell),
        };
        let fg = match (t, beat.is_some()) {
            (0, _) => theme::edit(),
            (_, true) => theme::set(),
            (_, false) => theme::dim(),
        };
        let mut style = Style::new().fg(fg);
        if band.running && t == band.tick {
            style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        spans.push(Span::styled(text, style));
    }
    spans.push(Span::styled(format!("  {head}"), Style::new().add_modifier(Modifier::BOLD)));
    if !place.is_empty() {
        spans.push(Span::styled(format!("   {place}"), Style::new().fg(theme::text())));
    }
    spans.push(Span::styled(
        format!("  {state}{hint}"),
        Style::new().fg(if band.running { theme::set() } else { theme::dim() }),
    ));
    Line::from(spans)
}

/// La 2ᵉ ligne : ce qui **vient** — la section suivante, un changement de mesure ou de tempo, et
/// le **compte à rebours de changement de son**, qui passe au décompte des temps sur la dernière
/// mesure. C'est la ligne qui prévient au bon moment de changer de son.
///
/// La section **nommée** est celle qui suit le groupe de sections de même nom (le prompteur n'en
/// montre qu'une) ; un changement de mesure ou de tempo **à l'intérieur** du groupe se dit à part,
/// avec son propre compte (« passe en 4/4 dans 2 mes. »).
fn band_next(s: &State, band: &Band, width: usize) -> Line<'static> {
    let dim = Style::new().fg(theme::dim());
    if !band.sectioned {
        let how = if s.locked { "" } else { " (t pour écrire le prompteur)" };
        return Line::styled(format!(" pas de sections : la battue compte les mesures{how}"), dim);
    }
    let Some(next) = band.next.as_ref() else {
        let left = band.group_bars.map(|b| b.saturating_sub(band.group_bar));
        return Line::styled(
            match left {
                Some(n) => format!(" fin du morceau dans {n} mes."),
                None => " dernière section".to_string(),
            },
            dim,
        );
    };
    let warn = Style::new().fg(theme::warn());
    let lit = Style::new().fg(theme::warn()).add_modifier(Modifier::REVERSED | Modifier::BOLD);
    // la section annoncée par son nom : celle d'après le groupe, ou la fin du morceau
    let ahead = match band.ahead.as_ref() {
        Some(a) => {
            let when = a.bars_left.map(|n| format!(" dans {n} mes.")).unwrap_or_default();
            format!(" {}{when}", a.label.to_uppercase())
        }
        None => match band.group_bars.map(|b| b.saturating_sub(band.group_bar)) {
            Some(n) => format!(" fin du morceau dans {n} mes."),
            None => " dernière section".to_string(),
        },
    };
    // ce qui change à la section suivante ; à l'intérieur du groupe, avec son propre compte
    let when = if next.inside { format!(" dans {} mes.", next.bars_left) } else { String::new() };
    let mut extras: Vec<Span> = Vec::new();
    if let Some(m) = next.meter.as_ref() {
        extras.push(Span::styled(format!("  passe en {}{when}", meter_label(m)), warn));
    }
    if let Some(bpm) = next.tempo_bpm {
        extras.push(Span::styled(format!("  ♩{bpm:.0}{when}"), warn));
    }
    let mut spans: Vec<Span> = Vec::new();
    if band.counting_down() {
        // **la dernière mesure** : le son visé, puis le décompte des temps battus
        let sound = next.sound.clone().unwrap_or_default().to_uppercase();
        spans.push(Span::styled(format!(" → {sound} "), lit));
        spans.push(Span::raw("  "));
        let total = band.meter.beats();
        let left = total.saturating_sub(band.meter.beat_of(band.tick));
        for v in (1..=total).rev() {
            if v < total {
                spans.push(Span::styled(" · ", dim));
            }
            spans.push(Span::styled(format!(" {v} "), if v == left { lit } else { dim }));
        }
        // la section qui suit dans le même groupe n'a pas de nom à dire : le décompte suffit
        if !next.inside {
            spans.push(Span::styled(format!("   {}", next.label.to_uppercase()), warn));
        }
    } else {
        if let Some(sound) = next.sound.as_ref() {
            spans.push(Span::styled(
                format!(" → {sound} dans {} mes.  ", next.bars_left),
                warn.add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::styled(ahead, Style::new().fg(theme::text())));
    }
    spans.extend(extras);
    // la ligne ne doit jamais pousser le cadre : on coupe au dernier segment qui tient
    let mut room = width;
    spans.retain(|sp| {
        let n = sp.content.chars().count();
        let keep = n <= room;
        if keep {
            room -= n;
        }
        keep
    });
    Line::from(spans)
}

/// `7/8 (2+2+3)` : la mesure, et son regroupement quand il n'est pas celui qu'on devine.
fn meter_label(m: &library::bars::Meter) -> String {
    if m.groups.iter().all(|g| *g == m.groups[0]) {
        return m.time.to_string();
    }
    let groups: Vec<String> = m.groups.iter().map(|g| g.to_string()).collect();
    format!("{} ({})", m.time, groups.join("+"))
}
