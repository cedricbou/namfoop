//! **Ce qui arrive** sur le port (`docs/design/midi.md` § 8) : un message reçu, ce que namfoop en
//! ferait d'après le set du moment, et la zone « reçu ». La capture par la fenêtre « Régler » est
//! dans `input.rs`, avec la fenêtre.

use super::*;

impl State {
    /// Un message du port, venu de l'appareil `device`, à l'instant `now_ms`.
    pub fn on_midi(&mut self, device: &str, m: &Message, now_ms: u64) {
        if m.event == Event::ClockTick {
            self.beating.insert(device.to_string(), now_ms);
        }
        if matches!(self.modal, Some(Modal::Input(_))) {
            if m.event != Event::ClockTick {
                self.capture(device, m);
            }
            return;
        }
        if self.is_ignored(device) {
            *self.ignored.entry(device.to_string()).or_default() += 1;
            return;
        }
        if let Some(c) = m.channel {
            self.seen.entry(device.to_string()).or_default().insert(c);
        }
        match m.event {
            Event::ClockTick => self.tick(device, now_ms),
            Event::ClockStart => self.push(device, m, "→ la battue repart du temps 1 ⏵".into(), Tone::Plain),
            Event::ClockStop => self.push(device, m, "→ la battue s'arrête".into(), Tone::Plain),
            Event::ClockContinue | Event::SongPosition(_) => {}
            Event::ProgramChange(_) | Event::Control { .. } => self.gesture(device, m, now_ms),
        }
    }

    /// Le tempo de l'horloge suivie, mesuré noire par noire.
    fn tick(&mut self, device: &str, now_ms: u64) {
        self.ticks += 1;
        if self.ticks < 24 {
            return;
        }
        self.ticks = 0;
        let Some(t0) = self.beat_start_ms.replace(now_ms).filter(|t0| now_ms > *t0) else { return };
        let bpm = 60_000.0 / (now_ms - t0) as f64;
        self.tempo = Some(bpm);
        // la zone « reçu » montre l'horloge par son tempo, une ligne par changement (§ 8) : la
        // gigue d'un câble (quelques dixièmes de BPM) n'en est pas un
        if self.logged_tempo.is_none_or(|l| (l - bpm).abs() >= 1.0) {
            self.logged_tempo = Some(bpm);
            let ligne = LogLine {
                device: device.to_string(),
                channel: None,
                message: "horloge".into(),
                effect: format!("→ ♩ {bpm:.1} BPM").replace('.', ","),
                tone: Tone::Plain,
            };
            self.log.push_back(ligne);
            while self.log.len() > LOG_LINES {
                self.log.pop_front();
            }
        }
    }

    /// Ce que namfoop ferait du morceau ou du son demandé, d'après le set du moment
    /// (`crate::midi_rules`, les règles de `browse`).
    fn meaning(&mut self, g: Gesture, event: Event, now_ms: u64) -> (String, Tone) {
        use crate::midi_rules::{song_for_pc, sound_of, title};
        let preset = |spec: &library::set::SetSpec, song: usize, k: usize| {
            spec.songs.get(song).and_then(|s| s.sounds.get(k)).map(|s| s.preset.clone()).unwrap_or_default()
        };
        let spec = self.set.as_ref().map(|f| f.spec.clone());
        match (g, event, spec.as_ref()) {
            (Gesture::Pc, Event::ProgramChange(p), Some(spec)) => match song_for_pc(spec, p) {
                Ok(song) => {
                    self.playing = Some((song, 0));
                    self.heard.insert((song, None), now_ms);
                    self.heard.insert((song, Some(0)), now_ms);
                    (
                        format!("→ morceau « {} », son 1 « {} »", title(spec, song), preset(spec, song, 0)),
                        Tone::Plain,
                    )
                }
                Err(r) => (format!("hors accord : {}", r.reason()), Tone::Warn),
            },
            (Gesture::Pc, Event::ProgramChange(p), None) => {
                (format!("→ preset n° {p} de la banque"), Tone::Plain)
            }
            (Gesture::Sound, Event::Control { value, .. }, Some(spec)) => {
                match sound_of(spec, self.playing.map(|(s, _)| s), value) {
                    Ok((song, k)) => {
                        self.playing = Some((song, k));
                        self.heard.insert((song, Some(k)), now_ms);
                        (format!("→ son {} « {} »", k + 1, preset(spec, song, k)), Tone::Plain)
                    }
                    Err(r) => (format!("hors accord : {}", r.reason()), Tone::Warn),
                }
            }
            (Gesture::Sound, _, None) => ("sans set, le son du morceau ne désigne rien".into(), Tone::Dim),
            (Gesture::Tap, ..) => ("→ tap tempo".into(), Tone::Plain),
            (Gesture::PrevSong | Gesture::NextSong, Event::Control { value, .. }, _) if value < 64 => {
                ("relâché : ignoré".into(), Tone::Dim)
            }
            (Gesture::PrevSong | Gesture::NextSong, _, None) => ("aucun set ouvert : rien".into(), Tone::Dim),
            (Gesture::PrevSong | Gesture::NextSong, _, Some(spec)) => {
                let down = g == Gesture::NextSong;
                let from = self.playing.map(|(s, _)| s);
                let to = match from {
                    None => Some(0),
                    Some(s) if down => (s + 1 < spec.songs.len()).then_some(s + 1),
                    Some(s) => s.checked_sub(1),
                };
                match to {
                    Some(song) => {
                        self.playing = Some((song, 0));
                        self.heard.insert((song, None), now_ms);
                        self.heard.insert((song, Some(0)), now_ms);
                        (
                            format!(
                                "→ morceau « {} », son 1 « {} »",
                                title(spec, song),
                                preset(spec, song, 0)
                            ),
                            Tone::Plain,
                        )
                    }
                    None => ((if down { "dernier morceau" } else { "premier morceau" }).into(), Tone::Dim),
                }
            }
            (Gesture::Tuner, Event::Control { value, .. }, _) if value >= 64 => {
                ("→ basse coupée, accordeur".into(), Tone::Cut)
            }
            _ => ("→ son rendu".into(), Tone::Plain),
        }
    }

    fn gesture(&mut self, device: &str, m: &Message, now_ms: u64) {
        let listen = self.listen();
        if !listen.hears(m.channel) {
            self.other.0 += 1;
            if let Some(c) = m.channel {
                self.other.1.insert(c);
            }
            return;
        }
        let bloc = if self.draft.channels.is_empty() { None } else { m.channel };
        let n = listen.numbers(m.channel);
        let last = |text: String| Last { at_ms: now_ms, text, device: device.to_string() };
        self.last.insert(Row::Channel(bloc), last(String::new()));
        let (g, text) = match m.event {
            Event::ProgramChange(p) => (Some(Gesture::Pc), format!("PC {p}")),
            Event::Control { cc, value } => {
                let g = GESTURES[1..].iter().copied().find(|g| cc_of(&n, *g) == cc);
                (g, format!("valeur {value}"))
            }
            _ => (None, String::new()),
        };
        let Some(g) = g else {
            self.push(device, m, "relié à aucun geste".into(), Tone::Warn);
            return;
        };
        if n.muted.has(g) {
            self.push(
                device,
                m,
                format!("coupé : {} de ce canal est ignoré (b)", gesture_name(g)),
                Tone::Dim,
            );
            return;
        }
        self.last.insert(Row::Gesture(bloc, g), last(if g == Gesture::Tap { String::new() } else { text }));
        let (effect, tone) = self.meaning(g, m.event, now_ms);
        self.push(device, m, effect, tone);
    }

    fn push(&mut self, device: &str, m: &Message, effect: String, tone: Tone) {
        self.log.push_back(LogLine {
            device: device.to_string(),
            channel: m.channel,
            message: midi_io::describe(&m.event, None),
            effect,
            tone,
        });
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }
}
