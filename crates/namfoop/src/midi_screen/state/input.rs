//! **La fenêtre « Régler »** (`docs/design/midi.md` § 5) : la saisie d'un canal ou d'un numéro,
//! sa validation, et la **capture** d'un appui, qui remplit la saisie sans aucun autre effet.

use super::*;

impl State {
    pub(super) fn input_key(&mut self, key: Key) {
        let Some(Modal::Input(w)) = &mut self.modal else { return };
        match key {
            Key::Esc => {
                self.modal = None;
                self.say("rien de changé", Tone::Plain);
            }
            Key::Backspace => {
                w.buf.pop();
                w.err = None;
                w.add_channel = None;
                w.fresh = false;
            }
            Key::Char(c) if c.is_ascii_digit() && (w.fresh || w.buf.len() < 3) => {
                if w.fresh {
                    w.buf.clear();
                    w.fresh = false;
                }
                w.buf.push(c);
                w.err = None;
                w.add_channel = None;
                if w.learned.as_ref().is_some_and(|(_, t)| *t == Tone::Warn) {
                    w.learned = None;
                }
            }
            Key::Enter => self.commit(),
            // une saisie est un mode : h, q et le reste y sont inertes (ADR 0009)
            _ => {}
        }
    }

    pub(super) fn commit(&mut self) {
        let Some(Modal::Input(w)) = self.modal.clone() else { return };
        let v: Option<u16> = w.buf.parse().ok();
        let refuse = |s: &mut Self, e: String| {
            if let Some(Modal::Input(w)) = &mut s.modal {
                w.err = Some(e);
            }
        };
        match w.target {
            Target::SongPc(i) => {
                if let Err(e) = self.set_song_pc(i, v) {
                    return refuse(self, e);
                }
            }
            Target::AddChannel | Target::ChangeChannel(_) => {
                let Some(c) = v else {
                    return refuse(self, "un canal va de 1 à 16 : x retire un canal".into());
                };
                if !(1..=16).contains(&c) {
                    return refuse(self, "un canal va de 1 à 16".into());
                }
                let c = c as u8;
                let from = match w.target {
                    Target::ChangeChannel(f) => Some(f),
                    _ => None,
                };
                if from != Some(c) && self.draft.channels.iter().any(|x| x.channel == c) {
                    return refuse(self, format!("le canal {c} est déjà réglé"));
                }
                match from {
                    Some(f) => {
                        if let Some(x) = self.channel_mut(f) {
                            x.channel = c;
                        }
                        self.say(format!("canal {f} → canal {c}"), Tone::Ok);
                    }
                    None => {
                        let new = self.new_channel(c);
                        self.draft.channels.push(new);
                        self.say(
                            format!("canal {c} ajouté : les canaux non réglés sont ignorés sans bruit"),
                            Tone::Ok,
                        );
                    }
                }
                self.draft.channels.sort_by_key(|x| x.channel);
                self.cursor = Row::Channel(Some(c));
            }
            Target::Number(ch, g) => {
                let v = v.unwrap_or(default_cc(g) as u16);
                if v == 0 {
                    return refuse(self, "CC 0 est réservé au choix du set".into());
                }
                if v > 127 {
                    return refuse(self, "un numéro de CC va de 1 à 127".into());
                }
                let v = v as u8;
                let place = w.add_channel.or(ch);
                let numbers = match w.add_channel {
                    Some(c) => self.numbers_of(&self.new_channel(c)),
                    None => self.numbers(ch),
                };
                if let Some(other) = GESTURES
                    .iter()
                    .filter(|x| **x != g && **x != Gesture::Pc)
                    .find(|x| cc_of(&numbers, **x) == v)
                {
                    let ou = place.map(|c| format!(" du canal {c}")).unwrap_or_default();
                    return refuse(
                        self,
                        format!("CC {v} appelle déjà {}{ou} : un bouton, un geste", gesture_name(*other)),
                    );
                }
                if let Some(c) = w.add_channel {
                    let new = self.new_channel(c);
                    self.draft.channels.push(new);
                    self.draft.channels.sort_by_key(|x| x.channel);
                    self.say(format!("canal {c} ajouté, {} : CC {v}", gesture_name(g)), Tone::Ok);
                } else {
                    let ou = ch.map(|c| format!(", canal {c}")).unwrap_or_default();
                    self.say(format!("{}{ou} : CC {v}", gesture_name(g)), Tone::Ok);
                }
                self.set_cc(place, g, v);
            }
        }
        self.modal = None;
    }

    /// Un canal ajouté : le premier reprend les numéros de « Tous les canaux » (ce qu'on y avait
    /// tapé n'est pas perdu), les suivants partent de ceux de l'ADR 0011 (§ 4).
    pub(super) fn new_channel(&self, c: u8) -> MidiChannel {
        if self.draft.channels.is_empty() {
            return MidiChannel { muted: self.draft.muted.clone(), ..channel(c) };
        }
        let d = Midi::default();
        let base = &self.draft;
        let keep = |v: u8, def: u8| (v != def).then_some(def);
        MidiChannel {
            cc_sound: keep(base.cc_sound, d.cc_sound),
            cc_tap: keep(base.cc_tap, d.cc_tap),
            cc_tuner: keep(base.cc_tuner, d.cc_tuner),
            ..channel(c)
        }
    }

    pub(super) fn numbers_of(&self, c: &MidiChannel) -> Numbers {
        let mut draft = self.draft.clone();
        draft.channels = vec![c.clone()];
        Listen::from_config(&draft, None).numbers(Some(c.channel))
    }

    pub(super) fn set_cc(&mut self, ch: Option<u8>, g: Gesture, v: u8) {
        match ch {
            None => match g {
                Gesture::Sound => self.draft.cc_sound = v,
                Gesture::Tap => self.draft.cc_tap = v,
                Gesture::Tuner => self.draft.cc_tuner = v,
                Gesture::Pc => {}
            },
            Some(c) => {
                if let Some(x) = self.channel_mut(c) {
                    match g {
                        Gesture::Sound => x.cc_sound = Some(v),
                        Gesture::Tap => x.cc_tap = Some(v),
                        Gesture::Tuner => x.cc_tuner = Some(v),
                        Gesture::Pc => {}
                    }
                }
            }
        }
    }

    /// Un message reçu pendant la fenêtre « Régler » : il remplit la saisie, et rien d'autre.
    pub(super) fn capture(&mut self, device: &str, m: &Message) {
        let channels: Vec<u8> = self.draft.channels.iter().map(|c| c.channel).collect();
        let ignored = self.is_ignored(device);
        let Some(Modal::Input(w)) = &mut self.modal else { return };
        w.err = None;
        w.add_channel = None;
        if ignored {
            w.learned = None;
            w.err = Some(format!("{device} est décoché : b sur sa ligne pour l'écouter"));
            return;
        }
        if !matches!(m.event, Event::ProgramChange(_) | Event::Control { .. }) {
            w.learned = None;
            w.err = Some("l'horloge n'a ni canal ni numéro : rien à prendre".into());
            return;
        }
        // un message sans canal (une ligne de port simulé écrite sans « ch ») vaut pour la ligne
        // qu'on règle ; seul l'ajout d'un canal a besoin d'en lire un
        let canal = m.channel.map(|c| format!(" · canal {c}")).unwrap_or_default();
        match w.target {
            Target::SongPc(_) => {
                let Event::ProgramChange(n) = m.event else {
                    w.learned = None;
                    w.err = Some("c'est un CC : un morceau s'appelle par Program Change".into());
                    return;
                };
                w.buf = n.to_string();
                w.fresh = true;
                w.learned = Some((format!("● reçu : PC {n}{canal} · {device}"), Tone::Ok));
            }
            Target::AddChannel | Target::ChangeChannel(_) => {
                let Some(ch) = m.channel else {
                    w.learned = None;
                    w.err = Some("ce message ne dit pas son canal : tape le numéro".into());
                    return;
                };
                w.buf = ch.to_string();
                w.fresh = true;
                w.learned = Some((format!("● reçu sur le canal {ch} · {device}"), Tone::Ok));
            }
            Target::Number(line_ch, g) => {
                let Event::Control { cc, value } = m.event else {
                    w.learned = None;
                    w.err = Some("c'est un Program Change : cette ligne attend un CC".into());
                    return;
                };
                if let (Some(lc), Some(ch)) = (line_ch, m.channel.filter(|c| Some(*c) != line_ch)) {
                    if channels.contains(&ch) {
                        w.learned = None;
                        w.err = Some(format!(
                            "ce message arrive sur le canal {ch} : cette ligne règle le canal {lc}"
                        ));
                        return;
                    }
                    w.buf = cc.to_string();
                    w.add_channel = Some(ch);
                    w.learned = Some((
                        format!("canal {ch} non réglé : Entrée l'ajoute, CC {cc} pour {}", gesture_name(g)),
                        Tone::Warn,
                    ));
                    return;
                }
                w.buf = cc.to_string();
                w.fresh = true;
                w.learned = Some((format!("● reçu : CC {cc} · valeur {value}{canal} · {device}"), Tone::Ok));
            }
        }
    }
}
