//! **L'onglet Plan du set** (`docs/design/midi.md` § 6) et ce qui vaut pour tous les onglets : en
//! changer, choisir le canal (`C`) ou le set (`L`) par une liste.
//!
//! C'est la **seule entorse** à « un espace n'agit pas sur l'objet d'un autre » : le numéro d'un
//! morceau fait partie de l'accord avec le groupe, donc il se règle ici. Rien d'autre du set ne se
//! modifie dans cet écran.

use library::set::{PC_MAX, PC_MIN};

use super::*;

impl State {
    pub fn tab_index(&self) -> usize {
        Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0)
    }

    /// Le set a-t-il changé depuis son enregistrement ?
    pub fn set_dirty(&self) -> bool {
        self.set.as_ref().is_some_and(SetFile::dirty)
    }

    /// Changer d'onglet ne demande rien, mais prévient de ce qui n'est pas enregistré (§ 9).
    pub(super) fn go_tab(&mut self, i: usize) -> Action {
        match self.tab {
            Tab::Devices if self.dirty() => {
                self.say("config non enregistrée : S sur Appareils et canaux pour la garder", Tone::Warn)
            }
            Tab::Plan if self.set_dirty() => {
                self.say("set non enregistré : S sur Plan du set pour le garder", Tone::Warn)
            }
            _ => {}
        }
        self.tab = Tab::ALL[i];
        Action::None
    }

    /// Le canal dont le Plan et la Fiche montrent les CC : celui choisi par `C` s'il est encore
    /// réglé, sinon le premier ; `None` : tous les canaux (aucun n'est réglé).
    pub fn plan_channel(&self) -> Option<u8> {
        let chans: Vec<u8> = self.draft.channels.iter().map(|c| c.channel).collect();
        self.plan_channel.filter(|c| chans.contains(c)).or_else(|| chans.first().copied())
    }

    /// Les touches propres au Plan du set ; `None` : la touche descend aux communes.
    pub(super) fn plan_key(&mut self, key: Key) -> Option<Action> {
        let n = self.set.as_ref().map_or(0, |s| s.spec.songs.len());
        match key {
            Key::Up => self.plan_cursor = self.plan_cursor.saturating_sub(1),
            Key::Down => self.plan_cursor = (self.plan_cursor + 1).min(n.saturating_sub(1)),
            Key::Char('i') if n > 0 => {
                if !self.open.remove(&self.plan_cursor) {
                    self.open.insert(self.plan_cursor);
                }
            }
            Key::Enter if n > 0 => self.open(Target::SongPc(self.plan_cursor)),
            Key::Enter | Key::Char('i') => self.say("aucun set : L pour en choisir un", Tone::Plain),
            Key::Char('C') => self.choose_channel(),
            Key::Char('L') => self.choose_set(),
            Key::Char('S') => return Some(self.save_set()),
            // les touches de l'autre onglet ne visent rien ici
            Key::Char('a') | Key::Char('x') | Key::Char('b') => {
                self.say("cette touche règle les appareils et les canaux : ← pour y aller", Tone::Plain)
            }
            _ => return None,
        }
        Some(Action::None)
    }

    fn save_set(&mut self) -> Action {
        match &self.set {
            None => {
                self.say("aucun set : L pour en choisir un", Tone::Plain);
                Action::None
            }
            Some(s) if !s.dirty() => {
                self.say("set déjà enregistré", Tone::Plain);
                Action::None
            }
            Some(_) => Action::SaveSet,
        }
    }

    pub fn mark_set_saved(&mut self) {
        if let Some(s) = &mut self.set {
            s.saved = s.spec.clone();
            self.message = Some((format!("set enregistré : {}", s.stem), Tone::Ok));
        }
    }

    /// Un set lu par la boucle (`L`) : il remplace le précédent, et ses marques « reçu » partent.
    pub fn open_set(&mut self, spec: library::set::SetSpec, path: std::path::PathBuf) {
        let set = SetFile::new(spec, path);
        self.message = Some((format!("set « {} » : {} morceaux", set.stem, set.spec.songs.len()), Tone::Ok));
        self.set = Some(set);
        self.plan_cursor = 0;
        self.open.clear();
        self.heard.clear();
        self.playing = None;
    }

    /// `C` : le canal dont on voit les CC, parmi les canaux réglés.
    fn choose_channel(&mut self) {
        let chans: Vec<u8> = self.draft.channels.iter().map(|c| c.channel).collect();
        if chans.is_empty() {
            self.say("aucun canal réglé : le plan et la fiche valent pour tous les canaux", Tone::Plain);
            return;
        }
        let idx = self.plan_channel().and_then(|c| chans.iter().position(|x| *x == c)).unwrap_or(0);
        let items = chans.iter().map(|c| format!("canal {c}")).collect();
        self.modal = Some(Modal::List(ListWin { kind: ListKind::Channel(chans), items, idx }));
    }

    /// `L` : un autre set. Refusé tant que celui-ci a des modifications : elles se perdraient.
    fn choose_set(&mut self) {
        if self.set_dirty() {
            self.say("set non enregistré : S pour le garder avant d'en choisir un autre", Tone::Warn);
            return;
        }
        if self.sets.is_empty() {
            self.say("aucun set dans la bibliothèque : `browse`, puis e sur la set list", Tone::Plain);
            return;
        }
        let current = self.set.as_ref().map(|s| s.path.clone());
        let idx = self.sets.iter().position(|(_, p)| Some(p) == current.as_ref()).unwrap_or(0);
        let items = self.sets.iter().map(|(n, _)| n.clone()).collect();
        let paths = self.sets.iter().map(|(_, p)| p.clone()).collect();
        self.modal = Some(Modal::List(ListWin { kind: ListKind::Set(paths), items, idx }));
    }

    pub(super) fn list_key(&mut self, key: Key) -> Action {
        let Some(Modal::List(w)) = &mut self.modal else { return Action::None };
        match key {
            Key::Up => w.idx = w.idx.saturating_sub(1),
            Key::Down => w.idx = (w.idx + 1).min(w.items.len().saturating_sub(1)),
            Key::Esc => {
                self.modal = None;
                self.say("rien de changé", Tone::Plain);
            }
            Key::Enter => {
                let w = w.clone();
                self.modal = None;
                match w.kind {
                    ListKind::Channel(chans) => {
                        self.plan_channel = chans.get(w.idx).copied();
                        self.say(format!("plan et fiche : {}", w.items[w.idx]), Tone::Ok);
                    }
                    ListKind::Set(paths) => {
                        if let Some(p) = paths.get(w.idx) {
                            return Action::OpenSet(p.clone());
                        }
                    }
                }
            }
            _ => {}
        }
        Action::None
    }

    /// Le numéro de Program Change d'un morceau : vide = pas de numéro ; de 1 à 128, unique dans le
    /// set (`library::set`, la règle de l'édition du set).
    pub(super) fn set_song_pc(&mut self, i: usize, v: Option<u16>) -> Result<(), String> {
        let Some(set) = &mut self.set else { return Ok(()) };
        let pc = match v {
            None => None,
            Some(n) if n < PC_MIN as u16 || n > PC_MAX as u16 => {
                return Err(format!("un numéro de PC va de {PC_MIN} à {PC_MAX}"))
            }
            Some(n) => Some(n as u8),
        };
        if let Some(owner) = pc.and_then(|n| set.spec.pc_owner(n, i)) {
            let titre = crate::midi_rules::title(&set.spec, owner);
            return Err(format!("PC {} est déjà pris par « {titre} »", pc.unwrap_or_default()));
        }
        let titre = crate::midi_rules::title(&set.spec, i);
        if let Some(song) = set.spec.songs.get_mut(i) {
            song.pc = pc;
        }
        self.message = Some((
            match pc {
                Some(n) => format!("« {titre} » : PC {n}"),
                None => format!("« {titre} » : pas de numéro"),
            },
            Tone::Ok,
        ));
        Ok(())
    }
}
