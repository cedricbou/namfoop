//! Le dessin de l'écran MIDI (`docs/design/midi.md` § 3 à 5, 8). Lecture seule de l'état.
//!
//! **Un écran neutre** (charte, « Dans le terminal ») : l'écran MIDI n'est pas un espace de
//! `browse`, il n'a donc pas de couleur à lui. Texte blanc, secondaire gris acier, onglet actif et
//! ligne visée en inverse blanc ; le jaune dit la saisie et l'attention, le rouge le refus, le vert
//! le reçu, le violet la seule « basse coupée ».

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use model::Gesture;

use super::state::{cc_of, default_cc, InputWin, ListWin, Modal, Row, State, Tab, Target, Tone};
use super::table::{self, Ctx};
use crate::theme;

/// Largeur des colonnes d'une ligne de geste : l'étiquette, la valeur.
const LABEL: usize = 19;
const VALUE: usize = 27;

fn tone(t: Tone) -> Style {
    let c = match t {
        Tone::Plain => theme::text(),
        Tone::Dim => theme::dim(),
        Tone::Ok => theme::ok(),
        Tone::Warn => theme::warn(),
        Tone::Error => theme::error(),
        Tone::Cut => theme::cut(),
    };
    Style::new().fg(c)
}

fn strong() -> Style {
    Style::new().fg(theme::neutral()).add_modifier(Modifier::BOLD)
}

fn inverse() -> Style {
    Style::new().fg(theme::on_accent()).bg(theme::neutral()).add_modifier(Modifier::BOLD)
}

fn pad(s: &str, n: usize) -> String {
    let len = s.chars().count();
    if len > n {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        format!("{s}{}", " ".repeat(n - len))
    }
}

fn ago(at_ms: u64, now_ms: u64) -> String {
    let s = now_ms.saturating_sub(at_ms) / 1000;
    match s {
        0..=2 => "à l'instant".into(),
        3..=59 => format!("il y a {s} s"),
        _ => format!("il y a {} min", s / 60),
    }
}

pub fn gesture_label(g: Gesture) -> &'static str {
    match g {
        Gesture::Pc => "Morceau",
        Gesture::Sound => "Son du morceau",
        Gesture::Tap => "Tap tempo",
        Gesture::Tuner => "Accordeur",
        Gesture::PrevSong => "Morceau précédent",
        Gesture::NextSong => "Morceau suivant",
    }
}

/// Une ligne de la liste : des segments, et la ligne visée qu'elle représente.
struct Item {
    row: Option<Row>,
    /// Dans le Plan du set : le morceau que cette ligne représente.
    song: Option<usize>,
    spans: Vec<(String, Style)>,
}

fn item(row: Option<Row>, spans: Vec<(String, Style)>) -> Item {
    Item { row, song: None, spans }
}

fn head(text: String) -> Item {
    item(None, vec![(text, strong())])
}

fn list(s: &State, now_ms: u64, width: usize) -> Vec<Item> {
    let mut out = match &s.pinned {
        None => vec![head("APPAREILS · tous écoutés, sauf ceux décochés".into())],
        Some(p) => {
            let (by, fix) = if p.by_option {
                (format!("--midi-in « {} », pour cette fois", p.asked), "sans l'option, tous sont écoutés")
            } else {
                (
                    format!("in = « {} » dans [midi] de la config", p.asked),
                    "retire cette ligne pour les écouter tous",
                )
            };
            vec![
                head("APPAREILS · un seul écouté".into()),
                item(None, vec![(format!("  imposé par {by}"), tone(Tone::Warn))]),
                item(None, vec![(format!("  {fix}"), tone(Tone::Warn))]),
            ]
        }
    };
    for d in s.device_rows() {
        out.push(device(s, &d, now_ms));
    }
    if s.device_rows().is_empty() {
        out.push(item(
            None,
            vec![("  aucun appareil pour l'instant : branché, il sera écouté".into(), tone(Tone::Dim))],
        ));
    }
    out.push(item(None, vec![]));
    if s.draft.channels.is_empty() {
        out.push(head("CANAUX · aucun réglé : tous écoutés, avec les mêmes numéros".into()));
        block(s, None, now_ms, width, &mut out);
    } else {
        out.push(head("CANAUX · seuls les canaux réglés sont écoutés".into()));
        for c in s.draft.channels.iter().map(|c| c.channel) {
            block(s, Some(c), now_ms, width, &mut out);
        }
        let (n, chans) = &s.other;
        let what = if *n == 0 {
            "rien reçu".to_string()
        } else {
            let l: Vec<String> = chans.iter().map(u8::to_string).collect();
            format!(
                "{n} message(s) sur {} {}",
                if l.len() > 1 { "les canaux" } else { "le canal" },
                l.join(" · ")
            )
        };
        out.push(item(
            None,
            vec![
                (format!("  {}", pad("Autres canaux", LABEL)), tone(Tone::Dim)),
                (pad("ignorés", VALUE), tone(Tone::Dim)),
                (what, tone(Tone::Dim)),
            ],
        ));
    }
    out
}

fn device(s: &State, d: &super::state::DeviceLive, now_ms: u64) -> Item {
    let off = s.is_ignored(&d.name);
    let head = format!(
        "  {} {} {}",
        if off { "☐" } else { "☑" },
        pad(&d.name, 18),
        pad(if d.present { "branché" } else { "absent" }, 10)
    );
    let row = Some(Row::Device(d.name.clone()));
    if off {
        let n = s.ignored.get(&d.name).copied().unwrap_or(0);
        let t = if n > 0 { format!("décoché · {n} message(s) ignoré(s)") } else { "décoché".into() };
        return item(row, vec![(head, tone(Tone::Dim)), (t, tone(Tone::Dim))]);
    }
    let seen =
        s.seen.get(&d.name).map(|c| c.iter().map(u8::to_string).collect::<Vec<_>>()).unwrap_or_default();
    let seen = match seen.len() {
        0 => String::new(),
        1 => format!("canal {}", seen[0]),
        _ => format!("canaux {}", seen.join(" · ")),
    };
    let mut spans = vec![
        (head, if d.present { tone(Tone::Plain) } else { tone(Tone::Dim) }),
        (pad(&seen, 17), tone(Tone::Dim)),
    ];
    spans.extend(clock(s, &d.name, now_ms));
    item(row, spans)
}

/// La colonne horloge d'un appareil (§ 4).
fn clock(s: &State, name: &str, now_ms: u64) -> Option<(String, Style)> {
    let bat = s.beats(name, now_ms);
    let suivi = s.following.as_deref() == Some(name);
    let bpm = s.tempo.filter(|_| suivi).map(|t| format!("♩ {t:.0} ")).unwrap_or_else(|| "♩ ".into());
    if s.is_clock(name) {
        return Some(if bat {
            (format!("{bpm}⏵ horloge · choisie"), tone(Tone::Ok))
        } else {
            ("horloge choisie · muette : le tap reprend".into(), tone(Tone::Warn))
        });
    }
    if !bat {
        return None;
    }
    match (&s.draft.clock_from, &s.following) {
        (Some(c), _) => Some((format!("horloge ignorée : {c} est choisie"), tone(Tone::Warn))),
        (None, Some(f)) if f == name => {
            Some((format!("{bpm}⏵ horloge · le premier qui bat"), tone(Tone::Ok)))
        }
        (None, Some(f)) => Some((format!("horloge ignorée : {f} la donne"), tone(Tone::Warn))),
        (None, None) => None,
    }
}

fn block(s: &State, ch: Option<u8>, now_ms: u64, width: usize, out: &mut Vec<Item>) {
    // en 80 colonnes, la ligne du canal garde sa phrase entière plutôt que l'alignement (§ 11)
    let col = if width >= 100 { LABEL + VALUE + 2 } else { 18 };
    let title = ch.map_or("Tous les canaux".to_string(), |c| format!("Canal {c}"));
    let right = match ch {
        None => {
            let all: std::collections::BTreeSet<u8> = s.seen.values().flatten().copied().collect();
            let l: Vec<String> = all.iter().map(u8::to_string).collect();
            match l.len() {
                0 => (String::new(), tone(Tone::Dim)),
                1 => (format!("vu : canal {}", l[0]), tone(Tone::Dim)),
                _ => (format!("vus : {} · plusieurs canaux parlent", l.join(" · ")), tone(Tone::Warn)),
            }
        }
        Some(_) => match s.last.get(&Row::Channel(ch)) {
            Some(l) => (format!("dernier : {} · {}", l.device, ago(l.at_ms, now_ms)), tone(Tone::Dim)),
            None => ("○ rien reçu".into(), tone(Tone::Dim)),
        },
    };
    out.push(item(Some(Row::Channel(ch)), vec![(format!("  {}", pad(&title, col)), strong()), right]));
    let gabarit = s.template_of(ch).map_or("personnalisé".to_string(), |t| t.name.clone());
    out.push(item(
        Some(Row::Template(ch)),
        vec![
            (format!("    {}", pad("Gabarit", LABEL - 2)), tone(Tone::Dim)),
            (pad(&gabarit, VALUE), tone(Tone::Plain)),
        ],
    ));
    let n = s.numbers(ch);
    for g in super::state::GESTURES {
        let muted = n.muted.has(g);
        let value = match g {
            Gesture::Pc => "Program Change".to_string(),
            _ => {
                let v = cc_of(&n, g);
                let d = default_cc(g);
                if v == d {
                    format!("CC {v}")
                } else {
                    format!("CC {v} · défaut : {d}")
                }
            }
        };
        let recu = if muted {
            ("coupé (b pour écouter)".to_string(), tone(Tone::Dim))
        } else {
            match s.last.get(&Row::Gesture(ch, g)) {
                Some(l) => {
                    let t = if l.text.is_empty() { String::new() } else { format!("{} · ", l.text) };
                    (format!("● {t}{}", ago(l.at_ms, now_ms)), tone(Tone::Ok))
                }
                None => ("○ jamais reçu".into(), tone(Tone::Dim)),
            }
        };
        out.push(item(
            Some(Row::Gesture(ch, g)),
            vec![
                (format!("    {}", pad(gesture_label(g), LABEL - 2)), tone(Tone::Dim)),
                (pad(&value, VALUE), if muted { tone(Tone::Dim) } else { tone(Tone::Plain) }),
                recu,
            ],
        ));
    }
}

fn line_of(it: &Item, selected: bool, width: usize) -> Line<'static> {
    if selected {
        let mut text: String = it.spans.iter().map(|(t, _)| t.as_str()).collect();
        text.replace_range(..text.char_indices().nth(1).map_or(text.len(), |(i, _)| i), "▶");
        return Line::from(Span::styled(pad(&text, width), inverse()));
    }
    Line::from(it.spans.iter().map(|(t, st)| Span::styled(t.clone(), *st)).collect::<Vec<_>>())
}

pub fn render(s: &State, port: &str, now_ms: u64, f: &mut Frame) {
    let [top, tabs, body, msg, recu, _, bar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(6),
        Constraint::Length(1),
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .areas(f.area());

    // l'en-tête : ce qu'on écoute, et l'état d'enregistrement de ce que l'onglet règle
    let state = if s.tab == Tab::Fiche && s.set_dirty() && !s.dirty() {
        ("set ● non enregistré (S)".to_string(), tone(Tone::Warn))
    } else if s.tab == Tab::Plan {
        match &s.set {
            None => ("aucun set (L)".to_string(), tone(Tone::Dim)),
            Some(f) if f.dirty() => (format!("set : {} ● non enregistré (S)", f.stem), tone(Tone::Warn)),
            Some(f) => (format!("set : {} ✓", f.stem), tone(Tone::Ok)),
        }
    } else if !s.can_save {
        ("--no-config : rien n'est enregistré".to_string(), tone(Tone::Dim))
    } else if s.dirty() {
        ("config ● non enregistrée (S)".into(), tone(Tone::Warn))
    } else {
        ("config ✓".into(), tone(Tone::Ok))
    };
    let [left, right] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(state.0.chars().count() as u16 + 1)])
            .areas(top);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" namfoop midi ", strong()),
            Span::styled(port.to_string(), tone(Tone::Dim)),
        ])),
        left,
    );
    f.render_widget(Paragraph::new(Span::styled(state.0, state.1)), right);
    let songs = s.set.as_ref().map_or(0, |f| f.spec.songs.len());
    let names = [
        ("Appareils et canaux".to_string(), Tab::Devices, s.dirty()),
        (format!("Plan du set ({songs})"), Tab::Plan, s.set_dirty()),
        ("Fiche".to_string(), Tab::Fiche, false),
    ];
    let mut t = vec![Span::raw("  ")];
    for (name, tab, dirty) in names {
        let style = if s.tab == tab { inverse() } else { tone(Tone::Dim) };
        t.push(Span::styled(format!(" {name} "), style));
        t.push(if dirty { Span::styled("●", tone(Tone::Warn)) } else { Span::raw(" ") });
        t.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(t)), tabs);

    // le cadre à bandeau
    let devices = s.device_rows().len();
    let clock_by =
        s.draft.clock_from.clone().or_else(|| s.following.clone()).unwrap_or_else(|| "aucune".into());
    let chans = match s.draft.channels.len() {
        0 => "aucun canal réglé".to_string(),
        1 => "1 canal réglé".to_string(),
        n => format!("{n} canaux réglés"),
    };
    let title = if s.tab == Tab::Plan {
        plan_title(s)
    } else if s.tab == Tab::Fiche {
        let ch = s.plan_channel().map_or("tous les canaux".to_string(), |c| format!("canal {c}"));
        format!(" MIDI · FICHE — {ch} · tirée des réglages et du set ")
    } else {
        format!(" MIDI · APPAREILS ET CANAUX — {devices} appareil(s) · horloge : {clock_by} · {chans} ")
    };
    let frame = Block::bordered().border_style(tone(Tone::Dim)).title(Span::styled(title, strong()));
    let inner = frame.inner(body);
    f.render_widget(frame, body);
    let items = match s.tab {
        Tab::Plan => plan(s, now_ms),
        Tab::Fiche => fiche(s, inner.height as usize),
        Tab::Devices => list(s, now_ms, inner.width as usize),
    };
    let h = inner.height as usize;
    let cur = if s.tab == Tab::Fiche {
        usize::MAX
    } else if s.tab == Tab::Plan {
        items
            .iter()
            .position(|i| i.song == Some(s.plan_cursor) && i.row.is_none() && !i.spans.is_empty())
            .unwrap_or(usize::MAX)
    } else {
        items.iter().position(|i| i.row.as_ref() == Some(&s.cursor)).unwrap_or(0)
    };
    let selectable = |i: &Item| i.row.is_some() || i.song.is_some();
    let start = if cur != usize::MAX && cur >= h { cur + 1 - h } else { 0 };
    let mut lines: Vec<Line> = items
        .iter()
        .enumerate()
        .skip(start)
        .take(h)
        .map(|(i, it)| line_of(it, i == cur && selectable(it), inner.width as usize))
        .collect();
    if items.len() > start + h && h > 0 {
        let below = items.len() - start - h + 1;
        lines[h - 1] = Line::from(Span::styled(format!("… {below} ligne(s) plus bas (↓)"), tone(Tone::Dim)));
    }
    f.render_widget(Paragraph::new(lines), inner);

    if let Some((m, t)) = &s.message {
        f.render_widget(Paragraph::new(Span::styled(format!(" {m}"), tone(*t))), msg);
    }
    f.render_widget(Paragraph::new(zone_recu(s)), recu);
    let [l1, l2] = table::bar(table::context(s));
    f.render_widget(Paragraph::new(vec![bar_line(&l1), bar_line(&l2)]), bar);

    match &s.modal {
        Some(Modal::Input(w)) => input(s, w, body, f),
        Some(Modal::Quit) => quit(s, body, f),
        Some(Modal::Help) => help(s.tab, body, f),
        Some(Modal::List(w)) => pick(w, body, f),
        None => {}
    }
}

/// Une ligne de barre : la touche en blanc, ce qu'elle fait en gris. Une ligne sans segments
/// (« toute autre touche : rester ») est une note, toute grise.
fn bar_line(text: &str) -> Line<'static> {
    if !text.contains("   ") {
        return Line::styled(format!(" {text}"), tone(Tone::Dim));
    }
    let mut spans = vec![Span::raw(" ")];
    for (i, seg) in text.split("   ").enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        let (k, w) = seg.split_once(' ').unwrap_or((seg, ""));
        spans.push(Span::styled(k.to_string(), strong()));
        spans.push(Span::styled(format!(" {w}"), tone(Tone::Dim)));
    }
    Line::from(spans)
}

/// La zone « reçu » (§ 8) : les trois derniers messages, puis le compte des canaux non réglés.
fn zone_recu(s: &State) -> Vec<Line<'static>> {
    let mut out: Vec<Line> = Vec::new();
    if s.log.is_empty() {
        out.push(Line::from(vec![
            Span::styled(" reçu  ", strong()),
            Span::styled("rien pour l'instant", tone(Tone::Dim)),
        ]));
    }
    for (i, l) in s.log.iter().enumerate() {
        let ch = l.channel.map_or("—".to_string(), |c| format!("can. {c}"));
        out.push(Line::from(vec![
            Span::styled(if i == 0 { " reçu  " } else { "       " }, strong()),
            Span::styled(format!("{}{}", pad(&l.device, 14), pad(&ch, 9)), tone(Tone::Dim)),
            Span::styled(pad(&l.message, 11), tone(Tone::Plain)),
            Span::styled(l.effect.clone(), tone(l.tone)),
        ]));
    }
    while out.len() < 3 {
        out.push(Line::raw(""));
    }
    let (n, chans) = &s.other;
    if *n > 0 {
        let l: Vec<String> = chans.iter().map(u8::to_string).collect();
        out.push(Line::from(Span::styled(
            format!(
                "       canaux non réglés : {n} message(s) sur {} {}, ignorés sans bruit",
                if l.len() > 1 { "les canaux" } else { "le canal" },
                l.join(" · ")
            ),
            tone(Tone::Dim),
        )));
    }
    out
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn window(title: String, color: Style, area: Rect, lines: Vec<Line<'static>>, f: &mut Frame) {
    f.render_widget(Clear, area);
    let b = Block::bordered()
        .border_style(color)
        .title(Span::styled(format!(" {title} "), color.add_modifier(Modifier::BOLD)))
        .style(Style::new().bg(theme::popup_bg()));
    let inner = b.inner(area);
    f.render_widget(b, area);
    f.render_widget(Paragraph::new(lines), inner);
}

/// La fenêtre « Régler » (§ 5) : une saisie, donc en jaune.
fn input(s: &State, w: &InputWin, area: Rect, f: &mut Frame) {
    let (title, label, hint) = match w.target {
        Target::AddChannel if s.draft.channels.is_empty() => {
            ("Régler · Tous les canaux › premier canal réglé".to_string(), "Canal :", "de 1 à 16".to_string())
        }
        Target::AddChannel => ("Ajouter un canal".to_string(), "Canal :", "de 1 à 16".to_string()),
        Target::ChangeChannel(c) => (format!("Régler · Canal {c}"), "Canal :", "de 1 à 16".to_string()),
        Target::SongPc(i) => (
            format!(
                "Régler · Plan du set › {}",
                s.set.as_ref().map_or(String::new(), |f| crate::midi_rules::title(&f.spec, i))
            ),
            "N° de PC :",
            "de 1 à 128 · vide = pas de numéro".to_string(),
        ),
        Target::Number(ch, g) => (
            format!(
                "Régler · {} › {}",
                ch.map_or("Tous les canaux".to_string(), |c| format!("Canal {c}")),
                gesture_label(g)
            ),
            "Numéro de CC :",
            format!("de 1 à 127 · vide = {}, le défaut", default_cc(g)),
        ),
    };
    let edit = Style::new().fg(theme::warn()).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{label} "), tone(Tone::Dim)),
            Span::styled(format!(" {}▏", w.buf), edit),
            Span::styled(format!("   {hint}"), tone(Tone::Dim)),
        ]),
        Line::raw(""),
        Line::styled(
            if matches!(w.target, Target::SongPc(_)) {
                "Ou fais envoyer le Program Change :"
            } else {
                "Ou appuie sur le bouton, si l'appareil est branché :"
            },
            tone(Tone::Plain),
        ),
        match &w.learned {
            Some((t, k)) => Line::styled(t.clone(), tone(*k)),
            None => Line::styled("○ rien reçu pour l'instant", tone(Tone::Dim)),
        },
        match &w.err {
            Some(e) => Line::styled(e.clone(), tone(Tone::Error)),
            None => Line::styled(
                "Tant que cette fenêtre est ouverte, aucun message ne change le son.",
                tone(Tone::Dim),
            ),
        },
        Line::raw(""),
    ];
    lines.push(bar_line(&table::bar(Ctx::Input)[0]));
    window(title, Style::new().fg(theme::warn()), centered(area, 74, 9), lines, f);
}

/// « Modifications non enregistrées » (§ 9), avec les mots de `browse`.
fn quit(_s: &State, area: Rect, f: &mut Frame) {
    let lines = vec![
        Line::styled("La config MIDI a changé depuis le dernier enregistrement.", tone(Tone::Plain)),
        Line::raw(""),
        Line::from(vec![Span::styled("S       ", strong()), Span::raw("enregistrer puis quitter")]),
        Line::from(vec![Span::styled("Q       ", strong()), Span::raw("quitter sans enregistrer")]),
        Line::from(vec![Span::styled("Échap   ", strong()), Span::raw("rester (par défaut)")]),
    ];
    window(
        "Modifications non enregistrées".into(),
        Style::new().fg(theme::neutral()),
        centered(area, 64, 7),
        lines,
        f,
    );
}

fn help(tab: Tab, area: Rect, f: &mut Frame) {
    let mut lines: Vec<Line> = table::help(tab)
        .map(|b| {
            Line::from(vec![
                Span::styled(pad(b.label, 10), strong()),
                Span::styled(b.what.to_string(), tone(Tone::Plain)),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled("N'importe quelle touche referme l'aide.", tone(Tone::Dim)));
    window(format!("Aide · {}", tab.name()), Style::new().fg(theme::neutral()), area, lines, f);
}

mod set;
use set::{fiche, pick, plan, plan_title};

#[cfg(test)]
mod tests;
