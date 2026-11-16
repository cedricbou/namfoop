//! Rendu du navigateur (ratatui). Lecture seule de l'état.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use super::state::preset::Pending;
use super::state::space::Space;
use super::state::{AbState, Editor, Field, IrChoice, IrRow, ModelRow, State, Tab, FIELDS};
use crate::tui::{meter_marked, meter_width};
use library::recommend::NeedsIr;

mod bands;
mod editor;
mod footer;
mod fx;
mod picker;
mod rows;
mod set;
mod spectrum;
pub mod synoptic;
mod windows;

use editor::*;
use spectrum::render_spectrum;
pub use spectrum::SPECTRUM_MIN_WIDTH;

/// Mesures affichées en bas (fournies par la boucle).
#[derive(Default, Clone)]
pub struct Status {
    pub in_peak: f32,
    pub out_peak: f32,
    pub load: f32,
    /// Crête attendue à l'entrée de la carte d'après la calibration (repère sur `in`).
    pub in_mark: Option<f32>,
    /// Trim de calibration (dB) : sert à afficher ce que reçoit le modèle.
    pub trim_db: f32,
    /// Réduction de gain du limiteur (dB).
    pub limiter: f32,
    pub xruns: u64,
    pub loading: bool,
    /// Où en est le préchargement de la banque de presets (places prêtes, places en tout, #6) :
    /// affiché tant qu'il reste des places à bâtir.
    pub bank: Option<(usize, usize)>,
    pub device: String,
}

pub(super) fn cursor_span(on: bool) -> Span<'static> {
    if on {
        Span::styled(" ▶ ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("   ")
    }
}

fn badge(needs: NeedsIr) -> Span<'static> {
    match needs {
        NeedsIr::No => Span::styled(" IR incluse ", Style::new().fg(Color::DarkGray)),
        NeedsIr::Yes => Span::styled(" IR requise ", Style::new().fg(Color::Yellow)),
        NeedsIr::Maybe => Span::styled(" IR ?      ", Style::new().fg(Color::DarkGray)),
    }
}

/// Indicateur A/B permanent (en-tête de la Bibliothèque) : quel modèle, quelle IR on entend au
/// banc d'essai.
fn ab_badge(s: &State) -> Vec<Span<'static>> {
    let on = |t: String, bg: Color| {
        Span::styled(t, Style::new().fg(Color::Black).bg(bg).add_modifier(Modifier::BOLD))
    };
    let off = |t: String| Span::styled(t, Style::new().fg(Color::DarkGray));
    let name = |i: usize| s.irs[i].label.chars().take(24).collect::<String>();
    let mname = |i: usize| s.models[i].label.chars().take(28).collect::<String>();
    if let (Some(rm), false) = (s.ref_model, s.bypass) {
        let mut v = vec![Span::styled("A/B modèles ", Style::new().add_modifier(Modifier::BOLD))];
        let a = s.model.map_or("son direct".to_string(), mname);
        let b = format!("◆ {}", mname(rm));
        match s.ab_state() {
            AbState::Same => v.push(Span::styled(
                format!("{b} = modèle en cours : choisis-en un autre"),
                Style::new().fg(Color::Yellow),
            )),
            AbState::B => v.extend([
                off(format!("A {a}  ")),
                on(format!(" B {b} "), Color::Yellow),
                off("  (r)".into()),
            ]),
            _ => v.extend([on(format!(" A {a} "), Color::Cyan), off(format!("  B {b}  (r)"))]),
        }
        return v;
    }
    let mut v = vec![Span::styled("A/B ", Style::new().add_modifier(Modifier::BOLD))];
    match s.ab_state() {
        AbState::NoReference => {
            v.push(off("c : épingler un modèle ou une IR de référence · b : avec/sans IR".into()))
        }
        AbState::Same => v.push(Span::styled(
            format!("◆ {} = IR en cours : choisis-en une autre", name(s.reference.expect("épinglée"))),
            Style::new().fg(Color::Yellow),
        )),
        AbState::Bypass => v.push(on(" IR COUPÉE — b ".into(), Color::Red)),
        st => {
            let a = match s.ir {
                IrChoice::Item(i) => name(i),
                IrChoice::Blend => "mélange".into(),
                IrChoice::None => "aucune IR".into(),
            };
            let b = format!("◆ {}", name(s.reference.expect("épinglée")));
            if st == AbState::A {
                v.extend([on(format!(" A {a} "), Color::Cyan), off(format!("  B {b}  (r)"))]);
            } else {
                v.extend([
                    off(format!("A {a}  ")),
                    on(format!(" B {b} "), Color::Yellow),
                    off("  (r)".into()),
                ]);
            }
        }
    }
    v
}

/// Couleur du grain : clair vert, crunch jaune, saturé rouge.
fn grain_style(g: &str) -> Style {
    let c = if g.starts_with("clair") {
        Color::Green
    } else if g.starts_with("crunch") {
        Color::Yellow
    } else {
        Color::Red
    };
    Style::new().fg(c)
}

/// Ce que le **banc** joue : `◀ écouté` (un modèle) / `◀ écoutée` (une IR), § 3.
fn playing() -> Span<'static> {
    Span::styled("  ◀ écouté", Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD))
}

fn playing_ir() -> Span<'static> {
    Span::styled("  ◀ écoutée", Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD))
}

/// Le cadre de la Bibliothèque, avec le bandeau du banc d'essai (§ 2).
fn bench_block() -> Block<'static> {
    Block::bordered().border_style(Style::new().fg(Color::Magenta)).title(Span::styled(
        " ♪ BANC D'ESSAI · basse › modèle › IR · sans effets, la chaîne n'est pas touchée ",
        Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
    ))
}

/// Fenêtre de `height` lignes autour du curseur.
pub(super) fn window(len: usize, cursor: usize, height: usize) -> std::ops::Range<usize> {
    if len <= height {
        return 0..len;
    }
    let start = cursor.saturating_sub(height / 2).min(len - height);
    start..start + height
}

pub fn render(s: &State, st: &Status, f: &mut Frame) {
    let lib = s.space == Space::Library;
    // l'en-tête : les deux espaces, puis les onglets de la Bibliothèque (ou l'A/B de la Chaîne),
    // puis le filtre — qui n'a de sens que dans la Bibliothèque
    let [spaces, tabs, filter, body, now, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        // le filtre n'a de sens que dans la Bibliothèque, mais la **saisie d'un nom** (mélange ou
        // preset) doit se voir depuis les deux espaces (#110)
        Constraint::Length(u16::from(lib || s.naming.is_some())),
        Constraint::Min(5),
        Constraint::Length(4),
        Constraint::Length(2),
    ])
    .areas(f.area());

    let n = s.fx.len();
    let space_span = |on: bool, text: String, color: Color| {
        Span::styled(
            format!(" {text} "),
            if on {
                Style::new().fg(Color::Black).bg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(Color::Gray)
            },
        )
    };
    // à droite, l'état d'enregistrement du preset (#110) — ou, dans la vue scène, celui du **set**
    // (§ 6). Rien ne peut encore le modifier : l'édition est l'US 4 (#220).
    let (what, up_to_date) = match (s.space, s.set.as_ref()) {
        (Space::Set, Some(set)) => (format!("set : {}.set.toml", set.stem), true),
        _ => s.preset_label(),
    };
    // `run --set` (§ 9) : on ne montre pas trois espaces qu'on ne peut pas ouvrir. Le cadenas
    // dit en un regard qu'on est en concert et que rien ne s'édite.
    let mut head = if s.locked {
        vec![
            Span::styled(" namfoop run  ", Style::new().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" 🔒 SET  {} ", set_title(s)),
                Style::new().fg(Color::Black).bg(set::ACCENT).add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![
            Span::styled(" namfoop browse  ", Style::new().add_modifier(Modifier::BOLD)),
            space_span(lib, "1 Bibliothèque".into(), Color::Magenta),
            Span::raw(" "),
            space_span(
                s.space == Space::Chain,
                format!("2 Chaîne ● {n} bloc{}", if n > 1 { "s" } else { "" }),
                Color::Cyan,
            ),
            Span::raw(" "),
            space_span(s.space == Space::Set, set_badge(s), set::ACCENT),
        ]
    };
    // le tempo et **d'où il vient** (ADR 0008) : sans la source, on ne saurait pas pourquoi le
    // delay ne suit pas ce qu'on croit avoir réglé
    {
        use engine::tempo::TempoSource;
        let src = s.tempo_source();
        head.push(Span::raw("  "));
        head.push(Span::styled(
            format!(
                "♩ {:.0} ({}){}",
                s.heard_tempo_bpm(),
                src.label(),
                if s.metronome { " ·clic" } else { "" }
            ),
            match src {
                // une horloge perdue doit sauter aux yeux : le son ne suit plus le groupe
                TempoSource::ClockLost => Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                TempoSource::MidiClock => Style::new().fg(Color::Green),
                TempoSource::Tap => Style::new().fg(Color::Cyan),
                TempoSource::Preset => Style::new().fg(Color::DarkGray),
            },
        ));
    }
    if !what.is_empty() {
        let used: usize = head.iter().map(|sp| sp.content.chars().count()).sum();
        let right =
            if up_to_date { format!("{what} ✓") } else { format!("{what} ● non enregistré (S)") };
        let pad = (spaces.width as usize).saturating_sub(used + right.chars().count() + 1);
        head.push(Span::raw(" ".repeat(pad)));
        head.push(Span::styled(
            right,
            if up_to_date { Style::new().fg(Color::Green) } else { Style::new().fg(Color::Yellow) },
        ));
    }
    f.render_widget(Paragraph::new(Line::from(head)), spaces);

    // les onglets de la Bibliothèque, et l'indicateur A/B à leur droite
    let tab = |t: Tab, label: String| {
        let style = if s.tab == t {
            Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray)
        };
        Span::styled(format!("  {label}"), style)
    };
    let tab_line = if s.space == Space::Set {
        // la vue scène n'a pas d'onglets : la maquette met les deux cadres juste sous l'en-tête
        Line::raw("")
    } else if lib {
        Line::from(
            vec![
                tab(Tab::Models, format!("Modèles ({})", s.models.len())),
                tab(Tab::Irs, format!("IR ({})", s.irs.len())),
                tab(Tab::Blend, format!("Mélange ({})", s.blend.len())),
                Span::raw("    "),
            ]
            .into_iter()
            .chain(ab_badge(s))
            .collect::<Vec<_>>(),
        )
    } else if s.fx.bypass_all {
        Line::from(Span::styled(
            " SANS EFFETS — r ",
            Style::new().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled("  r : avec / sans effets", Style::new().fg(Color::DarkGray)))
    };
    f.render_widget(Paragraph::new(tab_line), tabs);

    if lib || s.naming.is_some() {
        let filter_line = if let Some(name) = &s.naming {
            Line::from(vec![
                Span::styled(
                    if s.naming_preset { " nom du preset : " } else { " nom du mélange : " },
                    Style::new().fg(Color::Yellow),
                ),
                Span::raw(format!("{name}█")),
            ])
        } else if s.filtering || !s.filter.is_empty() {
            Line::from(vec![
                Span::styled(" filtre : ", Style::new().fg(Color::Cyan)),
                Span::raw(format!("{}{}", s.filter, if s.filtering { "█" } else { "" })),
            ])
        } else {
            Line::from(Span::styled(" / pour filtrer", Style::new().fg(Color::DarkGray)))
        };
        f.render_widget(Paragraph::new(filter_line), filter);
    }

    let height = body.height.saturating_sub(2) as usize;
    let cur = s.cursor();
    let mut lines: Vec<Line> = Vec::new();
    match s.tab {
        _ if !lib => {}
        Tab::Models => {
            let rows = s.model_list();
            let label_w = rows::label_width(
                rows.iter().filter_map(|r| match r {
                    ModelRow::Model(m) => Some(s.models[*m].label.chars().count()),
                    ModelRow::Direct => None,
                }),
                36,
            );
            for i in window(rows.len(), cur, height) {
                let idx = match rows[i] {
                    ModelRow::Direct => {
                        let mut spans = vec![
                            cursor_span(i == cur),
                            Span::raw("       son direct (DI, sans modèle)"),
                            Span::styled("  pour écouter une IR seule", Style::new().fg(Color::DarkGray)),
                        ];
                        if s.model.is_none() {
                            spans.push(playing());
                        }
                        lines.push(Line::from(spans));
                        continue;
                    }
                    ModelRow::Model(m) => m,
                };
                let m = &s.models[idx];
                let mark = if s.ref_model == Some(idx) {
                    Span::styled("◆", Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                } else if s.used_by_chain(true, idx) {
                    // rappel : la chaîne utilise ce modèle (§ 3) ; le curseur n'y touche pas
                    Span::styled("●", Style::new().fg(Color::Green))
                } else if m.corrected() {
                    Span::styled("✎", Style::new().fg(Color::Magenta))
                } else {
                    Span::raw(" ")
                };
                let (chosen, refd) = (s.model == Some(idx), s.ref_model == Some(idx));
                let mut tail = Vec::new();
                match (s.ref_model.is_some(), s.listen_ref) {
                    (true, true) if refd => {
                        tail.push(playing());
                        tail.push(Span::styled(" (B)", Style::new().fg(Color::Yellow)));
                    }
                    (true, true) if chosen => {
                        tail.push(Span::styled("  A (r pour revenir)", Style::new().fg(Color::DarkGray)))
                    }
                    (true, false) if chosen && !refd => {
                        tail.push(playing());
                        tail.push(Span::styled(" (A)", Style::new().fg(Color::Cyan)));
                    }
                    _ if chosen => tail.push(playing()),
                    _ => {}
                }
                lines.push(rows::model_line(s, idx, i == cur, mark, label_w, tail));
            }
        }
        Tab::Irs => {
            let rows = s.ir_rows();
            // noms alignés pour que les mini-spectres forment une colonne
            let label_w = rows::label_width(
                rows.iter().filter_map(|r| match r {
                    IrRow::Ir { index, .. } => Some(s.irs[*index].label.chars().count()),
                    IrRow::NoIr => None,
                }),
                40,
            );
            for i in window(rows.len(), cur, height) {
                match rows[i] {
                    IrRow::NoIr => {
                        let mut spans = vec![cursor_span(i == cur), Span::raw("       aucune IR")];
                        if s.effective_ir() == IrChoice::None && s.ir == IrChoice::None {
                            spans.push(playing_ir());
                        }
                        lines.push(Line::from(spans));
                    }
                    IrRow::Ir { index, recommended } => {
                        let in_blend = s.blend.iter().any(|p| p.ir == index);
                        let mark = if s.reference == Some(index) {
                            Span::styled(" ◆ ", Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                        } else if s.used_by_chain(false, index) {
                            Span::styled(" ● ", Style::new().fg(Color::Green))
                        } else if recommended {
                            Span::styled(" ★ ", Style::new().fg(Color::Yellow))
                        } else if in_blend {
                            Span::styled(" ● ", Style::new().fg(Color::Magenta))
                        } else {
                            Span::raw("   ")
                        };
                        let (chosen, refd) = (s.ir == IrChoice::Item(index), s.reference == Some(index));
                        // marques A/B propres aux IR (pas pendant un A/B de modèles)
                        let ab = if s.ref_model.is_some() && !s.bypass {
                            AbState::NoReference
                        } else {
                            s.ab_state()
                        };
                        let mut tail = Vec::new();
                        match ab {
                            AbState::Bypass if chosen => tail.push(Span::styled(
                                "  ◀ choisie, IR coupée (b)",
                                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
                            )),
                            AbState::B if refd => {
                                tail.push(playing_ir());
                                tail.push(Span::styled(" (B)", Style::new().fg(Color::Yellow)));
                            }
                            AbState::B if chosen => tail
                                .push(Span::styled("  A (r pour revenir)", Style::new().fg(Color::DarkGray))),
                            AbState::A if chosen => {
                                tail.push(playing_ir());
                                tail.push(Span::styled(" (A)", Style::new().fg(Color::Cyan)));
                            }
                            _ if chosen => tail.push(playing_ir()),
                            _ => {}
                        }
                        lines.push(rows::ir_line(s, index, i == cur, mark, label_w, tail));
                    }
                }
            }
        }
        Tab::Blend => {
            if s.blend.is_empty() {
                lines.push(Line::raw("   Mélange vide. Onglet IR : espace sur une IR pour l'ajouter."));
            }
            for (i, p) in s.blend.iter().enumerate() {
                let it = &s.irs[p.ir];
                lines.push(Line::from(vec![
                    cursor_span(i == cur),
                    Span::styled(format!("{:+5.0} dB ", p.gain_db), Style::new().fg(Color::Magenta)),
                    Span::raw(if p.invert { "ø " } else { "  " }),
                    Span::raw(it.label.clone()),
                    Span::styled(format!("  [{}]", it.pack), Style::new().fg(Color::DarkGray)),
                ]));
            }
            lines.push(Line::raw(""));
            lines.push(Line::raw(format!(
                "   alignement automatique : {}",
                if s.align { "oui" } else { "non" }
            )));
        }
    }
    if let Some(ed) = &s.editor {
        render_editor(s, ed, f, body);
    } else if s.space == Space::Set {
        set::render(s, f, body);
    } else if !lib {
        fx::render_fx(s, f, body);
    } else if body.width >= SPECTRUM_MIN_WIDTH {
        let [list, spectrum] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(body);
        f.render_widget(Paragraph::new(lines).block(bench_block()), list);
        render_spectrum(s, f, spectrum);
    } else {
        // écran étroit : pas de colonne de droite, le panneau Bandes (z) prend des lignes à la
        // liste — et seulement s'il en reste assez pour lire la liste (#183)
        let room = body.height >= bands::HEIGHT + 4;
        if s.show_bands() && room {
            let [list, panel] =
                Layout::vertical([Constraint::Min(4), Constraint::Length(bands::HEIGHT)]).areas(body);
            f.render_widget(Paragraph::new(lines).block(bench_block()), list);
            bands::render(s, f, panel);
        } else {
            f.render_widget(Paragraph::new(lines).block(bench_block()), body);
        }
    }

    let mut meters = vec![Span::raw(" in ")];
    meters.extend(meter_marked(st.in_peak, 12, st.in_mark));
    if let Some(m) = s.model {
        // ce que reçoit le modèle : carte + trim + entrée propre (+ essai dans l'éditeur)
        let model_db = s.saved_meta(m).input_db + s.input_offset();
        let text = if st.in_peak > 1e-6 {
            format!(" → modèle {:.0} dB", dsp::gain_to_db(st.in_peak) + st.trim_db + model_db)
        } else {
            " → modèle  -inf".to_string()
        };
        meters.push(Span::styled(text, Style::new().fg(Color::DarkGray)));
    }
    meters.push(Span::raw("   out "));
    meters.extend(meter_width(st.out_peak, 12));
    meters.push(Span::raw(if st.limiter > 0.05 {
        format!("   lim -{:.1} dB", st.limiter)
    } else {
        "   lim    -   ".to_string()
    }));
    meters.push(Span::raw(format!("   dsp {:>4.0} %  xruns {}", st.load * 100.0, st.xruns)));
    if st.loading {
        meters.push(Span::styled("  chargement…", Style::new().fg(Color::Yellow)));
    }
    // préchargement de la banque : visible seulement tant qu'il reste des places (#6)
    if let Some((done, total)) = st.bank.filter(|(d, t)| d < t) {
        meters.push(Span::styled(format!("  banque {done}/{total}"), Style::new().fg(Color::DarkGray)));
    }
    // la ligne d'écoute : on sait toujours ce qu'on entend (§ 2, règle 2)
    let (heard, second) = if s.space == Space::Set {
        // § 6 : morceau › son › la chaîne en une ligne, et le volume du morceau s'il y en a un
        let where_ = match (s.set.as_ref(), s.set.as_ref().and_then(|x| x.at)) {
            (Some(set), Some(at)) => format!("{} › ", set.label(at)),
            _ => String::new(),
        };
        let volume = match s.fx.chain.volume_db {
            0.0 => String::new(),
            v => format!("   volume {v:+.1} dB"),
        };
        (
            Line::from(vec![
                Span::styled(" ♪ set : ", Style::new().fg(set::ACCENT).add_modifier(Modifier::BOLD)),
                Span::raw(format!("{where_}basse › {}", s.chain_text())),
            ]),
            Line::from(Span::styled(
                format!("   ♩ {:.0} ({}){volume}", s.heard_tempo_bpm(), s.tempo_source().label()),
                Style::new().fg(Color::DarkGray),
            )),
        )
    } else if lib {
        (
            Line::from(vec![
                Span::styled(" ♪ banc : ", Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                Span::raw(s.heard_bench()),
            ]),
            Line::from(Span::styled(
                format!("   chaîne (non écoutée) : {}", s.chain_text()),
                Style::new().fg(Color::DarkGray),
            )),
        )
    } else {
        (
            Line::from(vec![
                Span::styled(" ♪ chaîne : ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(format!("basse › {}", s.chain_text())),
            ]),
            Line::from(Span::styled(
                trial_line(s).unwrap_or_default(),
                Style::new().fg(if s.picker.as_ref().is_some_and(|p| p.ab) {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            )),
        )
    };
    f.render_widget(
        Paragraph::new(vec![
            heard,
            second,
            Line::from(meters),
            Line::from(Span::styled(
                s.message.clone().unwrap_or_else(|| format!(" {}", st.device)),
                Style::new().fg(Color::Yellow),
            )),
        ]),
        now,
    );

    let keys = footer::keys(s, lib);
    f.render_widget(Paragraph::new(help_bar(&keys, foot.width)), foot);

    if s.confirm_quit {
        windows::render_confirm_quit(s, body, f);
    }
    if s.send.is_some() {
        windows::render_send(s, body, f);
    }
    if s.bank.is_some() {
        windows::render_bank(s, f.area(), f);
    }
    if s.confirm_switch {
        windows::render_confirm_switch(s, body, f);
    }
    if s.set_picker.is_some() {
        windows::render_set_picker(s, f, f.area());
    }
    // l'aide passe par-dessus tout (y compris le catalogue et le sélecteur)
    if s.help {
        windows::render_help(s, f.area(), f);
    }
}

/// La barre d'aide, sur les **deux lignes** du bas. Elle est coupée **aux séparateurs de touches**
/// (trois espaces) et non au milieu d'un mot, et quand le terminal est trop étroit les touches du
/// milieu sautent (remplacées par `…`) : les **touches de sortie** — les trois dernières, `h aide`,
/// l'autre espace et `q quitter` — restent toujours lisibles (#150). L'overlay `h` donne le détail.
fn help_bar(keys: &str, width: u16) -> Vec<Line<'static>> {
    let w = (width as usize).max(24) - 1;
    let grey = Style::new().fg(Color::DarkGray);
    let seg: Vec<&str> = keys.split("   ").filter(|p| !p.is_empty()).collect();
    let cut = seg.len().saturating_sub(3);
    // range les segments sur deux lignes au plus ; `None` : ça ne tient pas
    let lay = |parts: &[&str]| -> Option<Vec<String>> {
        let mut lines: Vec<String> = vec![String::new()];
        for p in parts {
            let n = p.chars().count();
            let line = lines.last_mut().expect("une ligne");
            if line.is_empty() {
                if n + 1 > w {
                    return None;
                }
                *line = format!(" {p}");
            } else if line.chars().count() + 3 + n <= w {
                *line = format!("{line}   {p}");
            } else {
                if lines.len() == 2 || n + 1 > w {
                    return None;
                }
                lines.push(format!(" {p}"));
            }
        }
        Some(lines)
    };
    for n in (0..=cut).rev() {
        let mut parts: Vec<&str> = seg[..n].to_vec();
        if n < cut {
            parts.push("…");
        }
        parts.extend_from_slice(&seg[cut..]);
        if let Some(lines) = lay(&parts) {
            return lines.into_iter().map(|l| Line::styled(l, grey)).collect();
        }
    }
    // terminal minuscule : on coupe net
    vec![Line::styled(format!(" {}", keys.chars().take(w).collect::<String>()), grey)]
}

/// Pendant un essai du sélecteur : ce qu'on entend n'est pas encore gardé (§ 5).
fn trial_line(s: &State) -> Option<String> {
    let p = s.picker.as_ref()?;
    if !p.trying() {
        return None;
    }
    Some(if p.ab {
        "   tu réentends l'actuel : c pour revenir à l'essai".into()
    } else {
        "   à l'essai : rien n'est gardé tant que tu ne valides pas (Entrée) · c pour réentendre l'actuel"
            .into()
    })
}

#[cfg(test)]
mod tests;

/// Ce que l'onglet `3 Set` affiche : le nom du set actif, ou rien quand il n'y en a pas (§ 6).
fn set_badge(s: &State) -> String {
    match s.set.as_ref() {
        Some(_) => format!("3 Set {}", set_title(s)),
        None => "3 Set".into(),
    }
}

/// Le nom du set tel qu'on l'affiche : son titre, ou le nom de son fichier s'il n'en a pas (le show
/// à trois sons bricolé sur place n'a pas de titre, et il doit quand même se reconnaître).
fn set_title(s: &State) -> String {
    match s.set.as_ref() {
        Some(set) if !set.spec.name.trim().is_empty() => set.spec.name.clone(),
        Some(set) => set.stem.clone(),
        None => String::new(),
    }
}
