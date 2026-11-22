//! **La vue scène** (§ 6 de `docs/design/set.md`) : la set list à gauche, le cadre du
//! morceau à droite, tous deux sur toute la hauteur — la hauteur sert à **voir venir**, la largeur
//! au texte du prompteur (son contenu est l'US 6a, sa bande de battue l'US 6b).
//!
//! Trois largeurs (§ 6) : au-delà de 100 colonnes la set list fait 44 colonnes ; en dessous elle
//! se réduit à 30 ; sous 70 elle disparaît au profit d'une seule ligne au-dessus du cadre, qui
//! prend alors toute la largeur. Sur scène, un écran étroit ne doit pas cacher ce qu'on joue.
//!
//! **L'édition** (`e`, § 8) remplace les deux colonnes par **une** table pleine largeur :
//! on y voit les PC, les tempos, les sons et leurs volumes de **tous** les morceaux à la fois,
//! parce que bâtir une set list, c'est comparer des lignes entre elles. Le prompteur est masqué.

use crate::theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::super::state::bindings;
use super::super::state::set::edit::Edit;
use super::super::state::set::ActiveSet;
use super::super::state::State;
use super::beat::band_lines;

/// En dessous, la set list se réduit (§ 6).
const WIDE: u16 = 100;
/// En dessous, elle disparaît au profit d'une ligne compacte.
const NARROW: u16 = 70;
const LIST_WIDE: u16 = 44;
const LIST_NARROW: u16 = 30;

pub fn render(s: &State, f: &mut Frame, area: Rect) {
    let Some(set) = s.set.as_ref() else {
        let lines = vec![
            Line::raw(""),
            Line::styled("   Aucun set actif.", Style::new().fg(theme::text())),
            Line::raw(""),
            Line::styled("   L : choisir un set de concert.", Style::new().fg(theme::dim())),
            Line::styled(
                "   Un set liste les morceaux d'un concert et leurs sons (guide, § 11).",
                Style::new().fg(theme::dim()),
            ),
        ];
        f.render_widget(Paragraph::new(lines).block(frame_block(" ♪ Set ")), area);
        return;
    };
    // l'édition prend toute la largeur : c'est un autre écran, pas une décoration (§ 8)
    if let Some(edit) = s.set_edit.as_ref() {
        render_edit(set, edit, f, area);
        return;
    }
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
        return Line::styled(" aucun son joué — Entrée joue le morceau visé", Style::new().fg(theme::text()));
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
        Span::styled(format!("● {sound}"), Style::new().fg(theme::set())),
        Span::styled(next, Style::new().fg(theme::dim())),
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
        .border_style(Style::new().fg(theme::dim()))
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
                        (_, true) => Style::new().fg(theme::warn()),
                        (true, _) => {
                            Style::new().fg(theme::on_accent()).bg(theme::set()).add_modifier(Modifier::BOLD)
                        }
                        _ => Style::new().fg(theme::text()),
                    },
                ));
            }
            if set.is_expanded(i) {
                let pc = song.pc.map(|p| format!("PC {p}")).unwrap_or_else(|| "sans PC".into());
                let prompt = match song.prompt.as_deref() {
                    Some(_) => " · prompteur",
                    None => "",
                };
                lines.push(Line::styled(format!("      {pc}{prompt}"), Style::new().fg(theme::dim())));
            }
        }
    }
    // ce qui déborde : on le dit, plutôt que de le couper en silence
    let room = area.height.saturating_sub(2) as usize;
    if lines.len() > room {
        let hidden = lines.len() + 1 - room;
        lines.truncate(room.saturating_sub(1));
        lines.push(Line::styled(format!("   … {hidden} lignes plus bas"), Style::new().fg(theme::dim())));
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
            (true, _) => Style::new().fg(theme::set()).add_modifier(Modifier::BOLD),
            (_, true) => Style::new().fg(theme::on_accent()).bg(theme::dim()),
            _ => Style::new().fg(theme::text()),
        },
    )
}

/// Le **cadre du morceau** : ce qu'on joue, ce que fera le prochain appui, la place du prompteur,
/// puis **le morceau suivant**. Le texte du prompteur est l'US 6a, la bande de battue l'US 6b.
fn render_song(s: &State, set: &ActiveSet, f: &mut Frame, area: Rect) {
    let Some(at) = set.at else {
        let lines = vec![
            Line::raw(""),
            Line::styled("   ↑↓ viser un morceau, Entrée le jouer.", Style::new().fg(theme::text())),
            Line::styled("   PgDn / PgUp : son suivant / précédent.", Style::new().fg(theme::dim())),
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
    // **la bande de battue** (§ 11), en tête de la colonne : c'est elle qu'on regarde en jouant
    let w = area.width.saturating_sub(4) as usize;
    lines.extend(band_lines(s, w));
    lines.push(Line::raw(""));
    // le texte du morceau (§ 10) : ce qui reste de hauteur lui revient, moins la place du
    // morceau suivant (4 lignes) qu'on garde pour voir venir
    let room = (area.height as usize).saturating_sub(8);
    lines.extend(prompt_lines(s, set, at, area.width.saturating_sub(4) as usize, room));
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
            Style::new().fg(theme::dim()),
        ));
        for sound in &song.sounds {
            lines.push(Line::styled(format!("     {}", sound.preset), Style::new().fg(theme::dim())));
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

// ------------------------------------------------------------- l'édition (`e`, § 8)

/// La table d'édition, **pleine largeur** : PC, tempo, sons et volumes de **tous** les morceaux
/// (§ 8). Le prompteur est masqué — on ne lit pas un texte pendant qu'on bâtit une set list, et
/// c'est cette largeur-là qui permet de voir les sons d'un morceau sur une ligne.
fn render_edit(set: &ActiveSet, edit: &Edit, f: &mut Frame, area: Rect) {
    let w = area.width.saturating_sub(2) as usize;
    let untitled = set.spec.songs.len() == 1 && set.spec.songs[0].title.is_none();
    let title_w = if untitled { 0 } else { (w / 5).clamp(10, 24) };
    let mut lines: Vec<Line> = Vec::new();
    if set.spec.songs.is_empty() {
        lines.push(Line::raw(""));
        lines
            .push(Line::styled("   Set vide : a ajoute un premier morceau.", Style::new().fg(theme::edit())));
    }
    for (i, song) in set.spec.songs.iter().enumerate() {
        lines.push(edit_row(set, edit, i, song, title_w, untitled));
    }
    // le rappel des touches, groupé comme l'écran : ce qui agit sur le morceau, puis sur le son.
    // Le texte vient de la table : la barre du bas, l'aide `h` et ces deux lignes ne
    // peuvent pas se contredire.
    let room = area.height as usize;
    if lines.len() + 4 <= room {
        lines.push(Line::raw(""));
        for part in [bindings::set_edit::SONG, bindings::set_edit::SOUND] {
            let text: Vec<&str> = part.iter().map(|b| b.bar).filter(|b| !b.is_empty()).collect();
            // coupé à la largeur, avec son « … » : un rappel tronqué au milieu d'un mot ferait
            // croire à une touche qui n'existe pas (l'aide `h` donne la liste entière)
            let text = clip(&format!("   {}", text.join("   ")), w);
            lines.push(Line::styled(text, Style::new().fg(theme::dim())));
        }
    }
    let songs = set.spec.songs.len();
    let sounds: usize = set.spec.songs.iter().map(|x| x.sounds.len()).sum();
    let dirty = if set.unsaved() { "  ● non enregistré (S)" } else { "" };
    let title = format!(
        " ✎ ÉDITION — {} · {songs} morceau{} · {sounds} son{}{dirty} ",
        set_title(set),
        if songs > 1 { "x" } else { "" },
        if sounds > 1 { "s" } else { "" },
    );
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().border_style(Style::new().fg(theme::edit())).title(
            Span::styled(clip(&title, w), Style::new().fg(theme::edit()).add_modifier(Modifier::BOLD)),
        )),
        area,
    );
}

/// Une ligne de la table : `▶ 2  Le Phare   PC 12  ♩ 92  ≡  [Fuzz] +0.5 dB · Néon`.
///
/// Trois marques, et elles ne disent pas la même chose : `▶` le son **joué** (ce qu'on entend),
/// le fond jaune ce qui est **visé** (ce que la prochaine touche changera), et `●` du titre le
/// fait que rien n'est encore écrit.
fn edit_row(
    set: &ActiveSet,
    edit: &Edit,
    i: usize,
    song: &library::set::Song,
    title_w: usize,
    untitled: bool,
) -> Line<'static> {
    let aimed_song = edit.aim.song() == i;
    let aimed_sound = if aimed_song { edit.aim.sound() } else { None };
    let playing = set.at.map(|(j, _)| j) == Some(i);
    let mut spans = vec![Span::styled(
        format!(" {} ", if playing { "▶" } else { " " }),
        Style::new().fg(theme::set()).add_modifier(Modifier::BOLD),
    )];
    // la tête du morceau : n°, titre, PC, tempo, prompteur. Visée (et pas au niveau d'un son) :
    // elle est en vidéo inverse, parce que c'est elle que `r`, `T`, `p`, `X` changent
    let head = if untitled {
        String::new()
    } else {
        format!("{:>2}  {:<title_w$} ", i + 1, clip(song.label(), title_w), title_w = title_w)
    };
    spans.push(Span::styled(
        head,
        match (aimed_song && aimed_sound.is_none(), playing) {
            (true, _) => Style::new().fg(theme::on_accent()).bg(theme::edit()).add_modifier(Modifier::BOLD),
            (_, true) => Style::new().add_modifier(Modifier::BOLD),
            _ => Style::new(),
        },
    ));
    // le PC : sans lui, le morceau n'est pas joignable au pied (§ 7), donc son absence est jaune
    spans.push(match song.pc {
        Some(pc) => Span::styled(format!("PC {pc:<3} "), Style::new().fg(theme::value())),
        None => Span::styled("PC —   ", Style::new().fg(theme::warn())),
    });
    spans.push(match song.tempo_bpm {
        Some(b) => Span::styled(format!("♩{b:<4.0}"), Style::new().fg(theme::value())),
        // `♩---` : le morceau suit le tempo de ses presets (§ 8), ce n'est pas un trou
        None => Span::styled("♩--- ", Style::new().fg(theme::dim())),
    });
    spans.push(Span::styled(if song.prompt.is_some() { "≡ " } else { "  " }, Style::new().fg(theme::dim())));
    if song.sounds.is_empty() {
        spans.push(Span::styled("(aucun son : + pour en ajouter)", Style::new().fg(theme::edit())));
        return Line::from(spans);
    }
    for (k, sound) in song.sounds.iter().enumerate() {
        if k > 0 {
            spans.push(Span::styled(" · ", Style::new().fg(theme::dim())));
        }
        let here = set.at == Some((i, k));
        let missing = set.slot((i, k)).is_none();
        let name = if sound.preset.is_empty() { "(à choisir)".to_string() } else { sound.preset.clone() };
        // le son **joué** est entre crochets : la couleur le dit déjà, mais pas sur une capture
        // d'écran ni pour un œil qui la distingue mal — et c'est l'information la plus importante
        let name = if here { format!("[{name}]") } else { name };
        spans.push(Span::styled(
            name,
            match (aimed_sound == Some(k), here, missing) {
                (true, _, _) => {
                    Style::new().fg(theme::on_accent()).bg(theme::edit()).add_modifier(Modifier::BOLD)
                }
                (_, _, true) => Style::new().fg(theme::error()),
                (_, true, _) => Style::new().fg(theme::set()).add_modifier(Modifier::BOLD),
                _ => Style::new().fg(theme::text()),
            },
        ));
        if sound.volume_db != 0.0 {
            spans.push(Span::styled(format!(" {:+.1} dB", sound.volume_db), Style::new().fg(theme::dim())));
        }
    }
    Line::from(spans)
}

/// La **saisie** d'un titre, d'un tempo ou d'un PC : une petite fenêtre jaune, parce qu'elle prend
/// le clavier entier et que cela doit se voir.
pub(super) fn render_typing(s: &State, f: &mut Frame, area: Rect) {
    let Some(t) = s.set_edit.as_ref().and_then(|e| e.typing.as_ref()) else { return };
    let lines = vec![
        Line::from(vec![
            Span::styled(format!(" {} : ", t.field.label()), Style::new().fg(theme::text())),
            Span::styled(t.text.clone(), Style::new().add_modifier(Modifier::BOLD)),
            Span::styled("▏", Style::new().fg(theme::set())),
        ]),
        Line::raw(""),
        Line::styled(" Entrée valider · Échap annuler", Style::new().fg(theme::dim())),
    ];
    window(f, area, " ✎ saisie ", lines, 54);
}

/// La **liste des presets** de `+` et d'`Entrée` : `↑↓` **essaie** (on l'entend), Échap rend le son
/// qu'on jouait. C'est le patron du sélecteur de la Chaîne (§ 5 de `browse-espaces.md`).
pub(super) fn render_picking(s: &State, f: &mut Frame, area: Rect) {
    let Some(p) = s.set_edit.as_ref().and_then(|e| e.picking.as_ref()) else { return };
    let room = (area.height.saturating_sub(6) as usize).max(3);
    let from = p.cursor.saturating_sub(room.saturating_sub(1));
    let mut lines: Vec<Line> = Vec::new();
    for (i, preset) in s.presets.iter().enumerate().skip(from).take(room) {
        let on = i == p.cursor;
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} {:<24}", if on { "▶" } else { " " }, clip(&preset.name, 24)),
                if on {
                    Style::new().fg(theme::on_accent()).bg(theme::edit()).add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                },
            ),
            Span::styled(
                format!("  {}", preset.broken.clone().unwrap_or_default()),
                Style::new().fg(theme::error()),
            ),
        ]));
    }
    if s.presets.len() > room {
        lines.push(Line::styled(
            format!("   … {} presets de plus", s.presets.len() - room),
            Style::new().fg(theme::dim()),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        " ↑↓ essayer (on l'entend) · Entrée garder · Échap annuler",
        Style::new().fg(theme::dim()),
    ));
    let what = if p.is_new { "ajouter un son" } else { "remplacer le preset du son" };
    window(f, area, &format!(" {what} — morceau {} ", p.at.0 + 1), lines, 60);
}

/// `X` : la confirmation de retrait d'un morceau. Un morceau retiré emmène ses sons et son
/// prompteur, donc on demande (§ 8).
pub(super) fn render_confirm_remove(s: &State, f: &mut Frame, area: Rect) {
    let Some(e) = s.set_edit.as_ref().filter(|e| e.confirm_remove) else { return };
    let who = s
        .set
        .as_ref()
        .and_then(|x| x.spec.songs.get(e.aim.song()))
        .map(|x| x.label().to_string())
        .unwrap_or_default();
    let lines = vec![
        Line::from(Span::styled(format!(" Retirer « {who} » du set ?"), Style::new().fg(theme::edit()))),
        Line::raw(""),
        Line::styled(" X confirmer · Échap le garder", Style::new().fg(theme::dim())),
    ];
    window(f, area, " ✎ retirer un morceau ", lines, 50);
}

/// Une fenêtre jaune centrée, de la largeur du texte.
fn window(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'static>>, min_w: u16) {
    let w = (lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16 + 4).max(min_w).min(area.width);
    let h = (lines.len() as u16 + 2).min(area.height);
    let win = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, win);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_style(Style::new().fg(theme::edit()))
                .title(Span::styled(
                    title.to_string(),
                    Style::new().fg(theme::edit()).add_modifier(Modifier::BOLD),
                ))
                .style(Style::new().bg(theme::popup_bg())),
        ),
        win,
    );
}

// -------------------------------------------------- le prompteur (§ 10)

/// Le **texte du morceau joué** : sections étiquetées, accords, cues, et la position.
///
/// Trois états de section, et c'est ce qui permet de se repérer d'un coup d'œil sur scène : la
/// **courante** en vert avec `▸`, les **passées** en gris, les **suivantes** en gras. La ligne
/// courante porte `›`. Le texte part **deux lignes avant** la position : on ne lit pas la première
/// ligne de l'écran, on a besoin de voir d'où l'on vient.
fn prompt_lines(s: &State, set: &ActiveSet, at: (usize, usize), w: usize, room: usize) -> Vec<Line<'static>> {
    use crate::browse::state::set::prompt::Loaded;
    let song = &set.spec.songs[at.0];
    let Some(loaded) = s.prompt_of(at.0) else {
        // pas de `prompt =` dans le set : on dit **comment** en avoir un, pas « rien »
        let mut v = vec![Line::styled("  pas de prompteur pour ce morceau", Style::new().fg(theme::dim()))];
        if !s.locked {
            let file = crate::browse::state::set::prompt::prompt_file_name(song.label());
            v.push(Line::styled(
                format!("  t l'écrit dans ton éditeur (ChordPro : prompts/{file})"),
                Style::new().fg(theme::dim()),
            ));
        }
        return v;
    };
    let prompt = match loaded {
        Loaded::Ok(p) => p,
        // fichier absent ou illisible : **jamais** d'erreur bloquante, un message dans le cadre
        Loaded::Failed(why) => {
            return vec![Line::styled(format!("  ⚠ {why}"), Style::new().fg(theme::warn()))];
        }
    };
    if prompt.sections.is_empty() {
        return vec![Line::styled(
            format!("  prompteur vide{}", if s.locked { "" } else { " : t pour l'écrire" }),
            Style::new().fg(theme::dim()),
        )];
    }
    // on met tout à plat une fois : c'est la seule façon de faire défiler le texte **et** ses
    // en-têtes de section ensemble
    let mut flat: Vec<Line<'static>> = Vec::new();
    let mut here = 0usize;
    for (i, sec) in prompt.sections.iter().enumerate() {
        let state = match i.cmp(&set.prompt_section) {
            std::cmp::Ordering::Less => Was::Past,
            std::cmp::Ordering::Equal => Was::Now,
            std::cmp::Ordering::Greater => Was::Next,
        };
        if !sec.label.is_empty() {
            flat.push(section_head(sec, state, prompt));
        }
        for (k, line) in sec.lines.iter().enumerate() {
            let cur = state == Was::Now && k == set.prompt_line;
            if cur {
                here = flat.len();
            }
            flat.extend(lyric_lines(line, w, cur, state == Was::Past));
        }
        if i + 1 < prompt.sections.len() {
            flat.push(Line::raw(""));
        }
    }
    // la fenêtre : deux lignes avant la position (§ 10)
    let from = here.saturating_sub(2).min(flat.len().saturating_sub(room.max(1)));
    flat.into_iter().skip(from).take(room.max(1)).collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Was {
    Past,
    Now,
    Next,
}

/// L'en-tête d'une section : son étiquette, sa durée en mesures, ce qu'elle change (mesure,
/// tempo), et **le son auquel elle est liée** — c'est ce qui fait le lien avec la set list.
fn section_head(
    sec: &library::prompt::Section,
    state: Was,
    prompt: &library::prompt::Prompt,
) -> Line<'static> {
    let mark = if state == Was::Now { "▸" } else { " " };
    let bars = match sec.bars {
        Some(b) => format!("{b} mes."),
        None => String::new(),
    };
    let mut extra = String::new();
    // une mesure ou un tempo ne se dit que s'il **change** : l'écrire partout serait du bruit
    if sec.time != prompt.time {
        if let Some(t) = sec.time.as_deref() {
            extra.push_str(&format!(" {t}"));
        }
    }
    if sec.tempo_bpm != prompt.tempo_bpm {
        if let Some(t) = sec.tempo_bpm {
            extra.push_str(&format!(" ♩{t:.0}"));
        }
    }
    let mut spans = vec![Span::styled(
        format!(" {mark} {:<14}{:<8}{extra}", sec.label.to_uppercase(), bars),
        match state {
            Was::Now => Style::new().fg(theme::set()).add_modifier(Modifier::BOLD),
            Was::Past => Style::new().fg(theme::dim()),
            Was::Next => Style::new().add_modifier(Modifier::BOLD),
        },
    )];
    if let Some(sound) = sec.sound.as_deref() {
        spans.push(Span::styled(
            format!("  → {sound}"),
            Style::new().fg(if state == Was::Now { theme::set() } else { theme::dim() }),
        ));
    }
    Line::from(spans)
}

/// Une ligne de paroles : les accords **au-dessus** quand la largeur le permet (§ 10), sinon
/// entre crochets dans le texte, comme dans le fichier.
fn lyric_lines(line: &library::prompt::Line, w: usize, cur: bool, past: bool) -> Vec<Line<'static>> {
    let text_style = match (cur, past, line.cue) {
        // une cue n'est pas du texte à chanter : elle est en jaune (§ 10)
        (_, _, true) => Style::new().fg(theme::cue()),
        (true, _, _) => Style::new().fg(theme::on_accent()).bg(theme::set()).add_modifier(Modifier::BOLD),
        (_, true, _) => Style::new().fg(theme::dim()),
        _ => Style::new(),
    };
    let mark = if cur { "›" } else { " " };
    if line.chords.is_empty() {
        return vec![Line::from(Span::styled(format!("   {mark} {}", clip(&line.text, w)), text_style))];
    }
    let chord_line = chord_row(line);
    // les accords ne tiennent pas au-dessus : on les remet dans le texte, lisible à coup sûr
    if chord_line.chars().count() + 5 > w {
        return vec![Line::from(Span::styled(format!("   {mark} {}", clip(&inline(line), w)), text_style))];
    }
    vec![
        Line::from(Span::styled(
            format!("     {chord_line}"),
            Style::new().fg(theme::chord()).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(format!("   {mark} {}", clip(&line.text, w)), text_style)),
    ]
}

/// Les accords placés **au-dessus** de leur syllabe.
fn chord_row(line: &library::prompt::Line) -> String {
    let mut out = String::new();
    for c in &line.chords {
        // deux accords trop proches : on garde un espace, plutôt que de les coller
        while out.chars().count() < c.at {
            out.push(' ');
        }
        if !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(&c.name);
    }
    out
}

/// La ligne telle qu'elle est écrite dans le fichier : `[Am]paroles`.
fn inline(line: &library::prompt::Line) -> String {
    let mut out = String::new();
    let chars: Vec<char> = line.text.chars().collect();
    let mut k = 0usize;
    for c in &line.chords {
        while k < c.at.min(chars.len()) {
            out.push(chars[k]);
            k += 1;
        }
        out.push('[');
        out.push_str(&c.name);
        out.push(']');
    }
    out.extend(chars[k.min(chars.len())..].iter());
    out
}
