//! **La vue scène** (§ 6 de `docs/design/set.md`, #219) : la set list à gauche, le cadre du
//! morceau à droite, tous deux sur toute la hauteur — la hauteur sert à **voir venir**, la largeur
//! au texte du prompteur (son contenu est l'US 6a, sa bande de battue l'US 6b).
//!
//! Trois largeurs (§ 6) : au-delà de 100 colonnes la set list fait 44 colonnes ; en dessous elle
//! se réduit à 30 ; sous 70 elle disparaît au profit d'une seule ligne au-dessus du cadre, qui
//! prend alors toute la largeur. Sur scène, un écran étroit ne doit pas cacher ce qu'on joue.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::super::state::set::ActiveSet;
use super::super::state::State;

/// En dessous, la set list se réduit (§ 6).
const WIDE: u16 = 100;
/// En dessous, elle disparaît au profit d'une ligne compacte.
const NARROW: u16 = 70;
const LIST_WIDE: u16 = 44;
const LIST_NARROW: u16 = 30;

/// La couleur d'accent de l'espace Set (Bibliothèque magenta, Chaîne cyan).
pub const ACCENT: Color = Color::Green;

pub fn render(s: &State, f: &mut Frame, area: Rect) {
    let Some(set) = s.set.as_ref() else {
        let lines = vec![
            Line::raw(""),
            Line::styled("   Aucun set actif.", Style::new().fg(Color::Gray)),
            Line::raw(""),
            Line::styled("   L : choisir un set de concert.", Style::new().fg(Color::DarkGray)),
            Line::styled(
                "   Un set liste les morceaux d'un concert et leurs sons (guide, § 11).",
                Style::new().fg(Color::DarkGray),
            ),
        ];
        f.render_widget(Paragraph::new(lines).block(frame_block(" ♪ Set ")), area);
        return;
    };
    if area.width < NARROW {
        // trop étroit pour deux colonnes : une ligne dit l'essentiel, le cadre prend le reste
        let [line, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
        f.render_widget(Paragraph::new(compact_line(set)), line);
        render_song(s, set, f, rest);
        return;
    }
    let list_w = if area.width >= WIDE { LIST_WIDE } else { LIST_NARROW };
    let [left, right] = Layout::horizontal([Constraint::Length(list_w), Constraint::Min(20)]).areas(area);
    render_list(s, set, f, left);
    render_song(s, set, f, right);
}

/// `2/6 Le Phare ● Fuzz Face → 3 Néon` : ce que montre un terminal trop étroit pour la set list.
fn compact_line(set: &ActiveSet) -> Line<'static> {
    let Some(at) = set.at else {
        return Line::styled(" aucun son joué — Entrée joue le morceau visé", Style::new().fg(Color::Gray));
    };
    let songs = set.spec.songs.len();
    let title = set.spec.songs.get(at.0).map(|x| x.label()).unwrap_or("?");
    let sound = sound_name(set, at);
    let next = set
        .step(true)
        .map(|n| {
            if n.0 == at.0 {
                format!(" → {}", sound_name(set, n))
            } else {
                format!(" → {} {}", n.0 + 1, set.spec.songs[n.0].label())
            }
        })
        .unwrap_or_else(|| " → fin du set".into());
    Line::from(vec![
        Span::styled(format!(" {}/{songs} {title} ", at.0 + 1,), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(format!("● {sound}"), Style::new().fg(ACCENT)),
        Span::styled(next, Style::new().fg(Color::DarkGray)),
    ])
}

/// Le nom du preset d'un son, tel que la bibliothèque le connaît.
fn sound_name(set: &ActiveSet, at: (usize, usize)) -> String {
    set.spec.songs.get(at.0).and_then(|s| s.sounds.get(at.1)).map(|s| s.preset.clone()).unwrap_or_default()
}

fn frame_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .border_style(Style::new().fg(Color::DarkGray))
}

/// La **colonne set list** : une ligne par morceau, l'essentiel seulement. Le morceau joué porte
/// `▶` et **déplie ses sons** ; la fenêtre commence **un morceau avant** celui qu'on joue, pour
/// que tout le reste de la hauteur montre ce qui vient (§ 6).
fn render_list(s: &State, set: &ActiveSet, f: &mut Frame, area: Rect) {
    let w = area.width.saturating_sub(4) as usize;
    let mut lines: Vec<Line> = Vec::new();
    let playing = set.at.map(|(i, _)| i);
    let untitled = set.spec.songs.len() == 1 && set.spec.songs[0].title.is_none();
    // un set sans titres : la colonne liste directement les sons (§ 6)
    let from = if untitled { 0 } else { playing.unwrap_or(set.cursor).saturating_sub(1) };
    for (i, song) in set.spec.songs.iter().enumerate().skip(from) {
        if !untitled {
            lines.push(song_line(s, set, i, song, w, playing == Some(i)));
        }
        let open = playing == Some(i) || set.is_expanded(i) || untitled;
        if open {
            for (k, sound) in song.sounds.iter().enumerate() {
                let here = set.at == Some((i, k));
                let missing = set.slot((i, k)).is_none();
                let mark = if here { "●" } else { "○" };
                let detail = if set.is_expanded(i) {
                    let vol = if sound.volume_db != 0.0 {
                        format!(" {:+.1} dB", sound.volume_db)
                    } else {
                        String::new()
                    };
                    format!("    CC69:{k}{vol}")
                } else {
                    String::new()
                };
                let text = format!("      {mark} {}{detail}", clip(&sound.preset, w));
                lines.push(Line::styled(
                    text,
                    match (here, missing) {
                        (_, true) => Style::new().fg(Color::Yellow),
                        (true, _) => Style::new().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
                        _ => Style::new().fg(Color::Gray),
                    },
                ));
            }
            if set.is_expanded(i) {
                let pc = song.pc.map(|p| format!("PC {p}")).unwrap_or_else(|| "sans PC".into());
                let prompt = match song.prompt.as_deref() {
                    Some(_) => " · prompteur",
                    None => "",
                };
                lines.push(Line::styled(format!("      {pc}{prompt}"), Style::new().fg(Color::DarkGray)));
            }
        }
    }
    // ce qui déborde : on le dit, plutôt que de le couper en silence
    let room = area.height.saturating_sub(2) as usize;
    if lines.len() > room {
        let hidden = lines.len() + 1 - room;
        lines.truncate(room.saturating_sub(1));
        lines.push(Line::styled(format!("   … {hidden} lignes plus bas"), Style::new().fg(Color::DarkGray)));
    }
    let place = match set.at {
        Some((i, _)) => format!(" · {}/{}", i + 1, set.spec.songs.len()),
        None => String::new(),
    };
    let title = format!(" ♪ {}{place} ", clip(&set_title(set), area.width.saturating_sub(8) as usize));
    f.render_widget(Paragraph::new(lines).block(frame_block(&title)), area);
}

fn set_title(set: &ActiveSet) -> String {
    if set.spec.name.trim().is_empty() {
        set.stem.clone()
    } else {
        set.spec.name.clone()
    }
}

/// Une ligne de morceau : n°, titre, tempo (`♩---` s'il suit les presets), la mesure si elle n'est
/// pas 4/4, `≡` s'il a un prompteur.
fn song_line(
    s: &State,
    set: &ActiveSet,
    i: usize,
    song: &library::set::Song,
    w: usize,
    playing: bool,
) -> Line<'static> {
    let tempo = match song.tempo_bpm {
        Some(b) => format!("♩{b:.0}"),
        None => "♩---".into(),
    };
    let time = match song.time_sig() {
        t if t == library::set::TimeSig::COMMON => String::new(),
        t => format!(" {t}"),
    };
    let prompt = if song.prompt.is_some() { " ≡" } else { "" };
    let mark = if playing { "▶" } else { " " };
    let title_w = w.saturating_sub(16);
    let text = format!(
        " {mark} {} {:<title_w$} {tempo:>5}{time}{prompt}",
        i + 1,
        clip(song.label(), title_w),
        title_w = title_w
    );
    let aimed = s.space == super::super::state::space::Space::Set && set.cursor == i;
    Line::styled(
        text,
        match (playing, aimed) {
            (true, _) => Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            (_, true) => Style::new().fg(Color::Black).bg(Color::DarkGray),
            _ => Style::new().fg(Color::Gray),
        },
    )
}

/// Le **cadre du morceau** : ce qu'on joue, ce que fera le prochain appui, la place du prompteur,
/// puis **le morceau suivant**. Le texte du prompteur est l'US 6a, la bande de battue l'US 6b.
fn render_song(s: &State, set: &ActiveSet, f: &mut Frame, area: Rect) {
    let Some(at) = set.at else {
        let lines = vec![
            Line::raw(""),
            Line::styled("   ↑↓ viser un morceau, Entrée le jouer.", Style::new().fg(Color::Gray)),
            Line::styled("   PgDn / PgUp : son suivant / précédent.", Style::new().fg(Color::DarkGray)),
        ];
        f.render_widget(Paragraph::new(lines).block(frame_block(" aucun son joué ")), area);
        return;
    };
    let songs = set.spec.songs.len();
    let sounds = set.spec.songs.get(at.0).map(|x| x.sounds.len()).unwrap_or(0);
    let next = set.step(true).map(|n| sound_name(set, n)).unwrap_or_else(|| "fin du set".into());
    // un set sans titres (le show à sons) n'a pas de morceau à nommer : on ne met pas de titre
    // fantôme à la place (§ 2, § 6)
    let untitled = songs == 1 && set.spec.songs[0].title.is_none();
    let who = if untitled {
        String::new()
    } else {
        format!("{} · {} · ", at.0 + 1, set.spec.songs.get(at.0).map(|x| x.label()).unwrap_or("?"))
    };
    let title = format!(" {who}son {}/{sounds} {} · PgDn : ensuite {next} ", at.1 + 1, sound_name(set, at),);
    let mut lines: Vec<Line> = Vec::new();
    // la place de la bande de battue (US 6b) et du texte (US 6a) : dite, pas simulée
    lines.push(Line::styled(
        format!("  ♩{:.0} {}   (battue : #223)", s.heard_tempo_bpm(), set.spec.songs[at.0].time_sig()),
        Style::new().fg(Color::DarkGray),
    ));
    lines.push(Line::raw(""));
    match set.spec.songs[at.0].prompt.as_deref() {
        Some(p) => lines.push(Line::styled(
            format!("  ≡ {} — le texte s'affichera ici (#222)", p.display()),
            Style::new().fg(Color::DarkGray),
        )),
        None => {
            lines.push(Line::styled("  (aucun prompteur pour ce morceau)", Style::new().fg(Color::DarkGray)))
        }
    }
    // le morceau suivant, pour voir venir (§ 6)
    if let Some(n) = (at.0 + 1 < songs).then(|| at.0 + 1) {
        let song = &set.spec.songs[n];
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(
                " ── ensuite (→) : {} · {}  {}",
                n + 1,
                song.label(),
                song.tempo_bpm.map(|b| format!("♩{b:.0}")).unwrap_or_default()
            ),
            Style::new().fg(Color::DarkGray),
        ));
        for sound in &song.sounds {
            lines.push(Line::styled(format!("     {}", sound.preset), Style::new().fg(Color::DarkGray)));
        }
    }
    f.render_widget(
        Paragraph::new(lines).block(frame_block(&clip(&title, area.width.saturating_sub(4) as usize))),
        area,
    );
}

/// Tronque à `w` caractères, en comptant des caractères et non des octets.
fn clip(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        return s.to_string();
    }
    s.chars().take(w.saturating_sub(1)).collect::<String>() + "…"
}
