//! Le dessin des onglets du set : le Plan (§ 6), la Fiche (§ 7) et la liste à choisir (`L`).

use super::*;

/// Le bandeau du Plan du set (§ 6).
pub(super) fn plan_title(s: &State) -> String {
    let ch = s.plan_channel().map_or("tous les canaux".to_string(), |c| format!("canal {c}"));
    match &s.set {
        None => " MIDI · PLAN DU SET — aucun set : L pour en choisir un ".into(),
        Some(f) => {
            let sans = f.spec.songs.iter().filter(|x| x.pc.is_none()).count();
            let sans = if sans > 0 { format!(" · {sans} sans numéro") } else { String::new() };
            format!(
                " MIDI · PLAN DU SET — {} · {} morceaux{sans} · sons du {ch} ",
                f.stem,
                f.spec.songs.len()
            )
        }
    }
}

/// Le corps du Plan du set (§ 6) : chaque morceau, son tempo, son numéro, et s'il est arrivé.
pub(super) fn plan(s: &State, now_ms: u64) -> Vec<Item> {
    let Some(f) = &s.set else {
        return vec![item(
            None,
            vec![("Aucun set : L en choisit un dans la bibliothèque.".into(), tone(Tone::Dim))],
        )];
    };
    let cc = s.numbers(s.plan_channel()).cc_sound;
    let mut out = vec![item(
        None,
        vec![(
            format!("    {}{}{}Reçu", pad("Morceau", 26), pad("Tempo", 8), pad("Program Change", 20)),
            tone(Tone::Dim),
        )],
    )];
    for (i, song) in f.spec.songs.iter().enumerate() {
        let joue = s.playing.map(|(x, _)| x) == Some(i);
        let titre = format!("{}{}", if joue { "♪ " } else { "" }, crate::midi_rules::title(&f.spec, i));
        let tempo = song.tempo_bpm.map(|b| format!("♩{b:.0}")).unwrap_or_default();
        let pc = song.pc.map_or("PC —".to_string(), |p| format!("PC {p}"));
        let gauche = format!("  {} {}{}{}", i + 1, pad(&titre, 26), pad(&tempo, 8), pad(&pc, 20));
        let (recu, st) = match (song.pc, s.heard.get(&(i, None))) {
            (None, _) => ("pas de numéro : on ne peut pas l'appeler".to_string(), tone(Tone::Warn)),
            (_, Some(t)) => (format!("● {}", ago(*t, now_ms)), tone(Tone::Ok)),
            _ => ("○ pas encore".to_string(), tone(Tone::Dim)),
        };
        let base = if song.pc.is_none() {
            tone(Tone::Warn)
        } else if joue {
            strong()
        } else {
            tone(Tone::Plain)
        };
        out.push(Item { row: None, song: Some(i), spans: vec![(gauche, base), (recu, st)] });
        if s.open.contains(&i) {
            for (k, sound) in song.sounds.iter().enumerate() {
                let cur = s.playing == Some((i, k));
                let nom = format!("      {} son {} · {}", if cur { "●" } else { "○" }, k + 1, sound.preset);
                let r = match s.heard.get(&(i, Some(k))) {
                    Some(t) => (format!("● {}", ago(*t, now_ms)), tone(Tone::Ok)),
                    None => ("○ pas encore".to_string(), tone(Tone::Dim)),
                };
                out.push(item(
                    None,
                    vec![
                        (pad(&nom, 38), if cur { tone(Tone::Ok) } else { tone(Tone::Dim) }),
                        (pad(&format!("CC {cc} = {k}"), 20), tone(Tone::Plain)),
                        r,
                    ],
                ));
            }
        }
    }
    out.push(item(None, vec![]));
    out.push(item(
        None,
        vec![(
            "Les sons et leur ordre se changent dans le Set. Ici, seul le numéro du morceau se règle.".into(),
            tone(Tone::Dim),
        )],
    ));
    out
}

/// La Fiche (§ 7), à partir de la ligne `fiche_scroll` : le texte tel qu'il partira.
pub(super) fn fiche(s: &State, h: usize) -> Vec<Item> {
    let all = s.fiche();
    let room = h.saturating_sub(1);
    let start = s.fiche_scroll.min(all.len().saturating_sub(1));
    let mut out: Vec<Item> = all
        .iter()
        .skip(start)
        .take(room)
        .map(|l| {
            let st = if l.starts_with("Basse (") || l.starts_with("Set «") {
                strong()
            } else if l.starts_with("Canal :") {
                tone(Tone::Warn)
            } else if l.starts_with("  ") {
                tone(Tone::Dim)
            } else {
                tone(Tone::Plain)
            };
            item(None, vec![(l.clone(), st)])
        })
        .collect();
    let below = all.len().saturating_sub(start + room);
    if below > 0 {
        out.push(item(None, vec![(format!("… {below} ligne(s) plus bas (↓)"), tone(Tone::Dim))]));
    }
    out
}

/// Une fenêtre de liste (le canal, le set) : pas une saisie, donc neutre.
pub(super) fn pick(w: &ListWin, area: Rect, f: &mut Frame) {
    let title = match w.kind {
        super::super::state::ListKind::Channel(_) => "Choisir le canal · Plan du set et Fiche",
        super::super::state::ListKind::Set(_) => "Choisir le set",
        super::super::state::ListKind::Template(_) => {
            "Choisir un gabarit · ses numéros remplacent ceux du canal"
        }
    };
    let mut lines: Vec<Line> = w
        .items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            if i == w.idx {
                Line::styled(pad(&format!("▶ {it}"), 66), inverse())
            } else {
                Line::styled(format!("  {it}"), tone(Tone::Plain))
            }
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(bar_line(&table::bar(Ctx::Pick)[0]));
    let h = (w.items.len() as u16 + 4).min(area.height);
    window(title.into(), Style::new().fg(theme::neutral()), centered(area, 72, h), lines, f);
}
