//! **La battue** (§ 11 de `docs/design/set.md`) : où l'on en est dans le morceau — la section, la
//! mesure, le temps —, et ce que la bande en tête du prompteur doit montrer.
//!
//! L'**horloge est fournie** par l'appelant, comme partout dans `browse::state` : la position
//! avance dans [`State::tick`], et les tests donnent les millisecondes qu'ils veulent. Mais dès
//! qu'une **horloge MIDI** tourne, ce n'est plus le temps qui la fait avancer : ce sont les
//! **tops** (`State::clock_elapsed_ticks`), parce qu'un comptage de tops est une position absolue
//! et ne peut donc pas dériver. Aucun fil,
//! aucun `Instant`, et **rien dans le fil audio** — la bande est dessinée par le fil de
//! l'interface, et le tempo de section entre dans l'arbitre comme celui d'un preset.
//!
//! La carte des mesures, elle, est dans `library::bars` (pure) : ici il n'y a que la position et
//! ce qui la déplace.
//!
//! **La règle qui tient tout** (§ 10, § 14) : *en marche, la position suit le **temps*** — changer
//! de son ne la déplace pas, sinon un son joué un temps trop tôt décalerait tout le texte ; *à
//! l'arrêt, elle suit le **son***.

use engine::tempo::TICKS_PER_BEAT;
use library::bars::{BarMap, Meter};
use library::set::TimeSig;

use super::super::{Action, State};

/// Nombre maximal de ticks rattrapés en un tour de boucle. Une image en retard (un modèle qui se
/// charge, un `$EDITOR` qui rend la main) ne doit pas perdre de temps ; une pause **longue** ne
/// doit pas non plus faire défiler le morceau en accéléré, alors au-delà on se **recale** sur
/// l'horloge et on garde la position atteinte.
const CATCH_UP_TICKS: u32 = 8;

/// La position de la battue dans le morceau joué. La **section** n'est pas ici : c'est
/// `ActiveSet::prompt_section`, parce que le texte et la battue avancent ensemble (§ 10) et que
/// deux champs se désaligneraient.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Beat {
    /// La battue avance-t-elle ? À l'arrêt, rien ne s'allume et la bande dit `⏸ Espace`.
    pub running: bool,
    /// La mesure **dans la section** (0 = la première ; la bande affiche `mes. bar + 1`).
    pub bar: u32,
    /// Le tick dans la mesure : une croche en x/8, une noire en x/4.
    pub tick: u32,
    /// L'instant (ms) où ce tick a commencé. C'est lui qu'un recalage remet à l'horloge : la
    /// battue ne dérive donc pas d'un appui à l'autre, chaque tick partant du précédent.
    at_ms: u64,
    /// Le même repère, mais compté en **tops d'horloge MIDI** depuis l'ancre de l'arbitre. C'est
    /// lui qui compte dès qu'une horloge tourne : une position absolue ne dérive pas, alors
    /// qu'intégrer un tempo mesuré transforme la moindre erreur de période en décalage définitif
    /// — le défaut que le clic a eu, pour la même raison.
    at_ticks: u64,
}

impl Beat {
    /// **Recaler sur le 1** d'une mesure, à cet instant : `Fin` / `Début` (le 1 de la section) et
    /// un changement de morceau s'en servent.
    pub fn restart(&mut self, now_ms: u64, now_ticks: u64) {
        self.bar = 0;
        self.tick = 0;
        self.at_ms = now_ms;
        self.at_ticks = now_ticks;
    }

    /// **Le tap recale le temps, pas la mesure** (§ 11) : on repart sur un 1 de temps sans toucher
    /// au numéro de la mesure en cours.
    pub fn realign(&mut self, now_ms: u64, now_ticks: u64) {
        self.tick = 0;
        self.at_ms = now_ms;
        self.at_ticks = now_ticks;
    }

    /// **Se poser** à une mesure et un temps donnés : la conversion d'un Song Position Pointer.
    pub fn seek(&mut self, bar: u32, tick: u32, now_ms: u64, now_ticks: u64) {
        self.bar = bar;
        self.tick = tick;
        self.at_ms = now_ms;
        self.at_ticks = now_ticks;
    }

    /// `Espace` : l'appui **est** le 1 de la mesure en cours.
    fn start(&mut self, now_ms: u64, now_ticks: u64) {
        self.running = true;
        self.realign(now_ms, now_ticks);
    }
}

/// La **section suivante**, telle que la 2ᵉ ligne de la bande l'annonce.
#[derive(Clone, Debug, PartialEq)]
pub struct Next {
    pub label: String,
    /// Combien de mesures avant d'y être.
    pub bars_left: u32,
    /// Le son auquel elle est liée, **s'il n'est pas celui qu'on joue** : c'est ce qui déclenche
    /// le compte à rebours de changement de son (§ 11).
    pub sound: Option<String>,
    /// Sa mesure, seulement si elle **change** (`passe en 7/8`).
    pub meter: Option<Meter>,
    /// Son tempo, seulement s'il **change** (`♩92`).
    pub tempo_bpm: Option<f32>,
    /// Elle porte le **même nom** que la section en cours : le prompteur les montre comme une
    /// seule (§ 4 de `prompteur.md`), donc la bande n'annonce pas son nom, seulement ce qui y
    /// change — la mesure, le tempo, le son —, avec son propre compte.
    pub inside: bool,
}

/// Ce qui vient **après** les sections voisines de même nom : la section que le prompteur montre
/// comme la suivante.
#[derive(Clone, Debug, PartialEq)]
pub struct Ahead {
    pub label: String,
    /// Combien de mesures avant d'y être. `None` : une section de durée inconnue est en travers.
    pub bars_left: Option<u32>,
}

/// Ce que la **bande de battue** montre : des faits, pas du texte. Le dessin et les couleurs sont
/// à `ui/set.rs`, et c'est voulu — `state` décide, `ui` dessine.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub running: bool,
    /// Le tempo **entendu** (celui de l'arbitre) : c'est lui qu'on bat, pas celui du fichier.
    pub tempo_bpm: f32,
    pub meter: Meter,
    /// La mesure dans la section, et sa durée si elle est connue.
    pub bar: u32,
    pub bars: Option<u32>,
    pub tick: u32,
    pub label: String,
    /// La mesure et la durée **dans le groupe** de sections voisines de même nom : c'est ce que la
    /// bande affiche (« mes. 11/22 REFRAIN »), parce que le prompteur ne montre qu'un refrain là
    /// où la carte en compte cinq.
    pub group_bar: u32,
    pub group_bars: Option<u32>,
    /// La section qui suit la section en cours, telle que la carte la voit : c'est elle qui porte
    /// le changement de son, de mesure ou de tempo.
    pub next: Option<Next>,
    /// La section qui suit le **groupe** : celle que la bande annonce par son nom.
    pub ahead: Option<Ahead>,
    /// Le morceau a-t-il des sections ? Sinon la battue compte seulement les mesures (§ 11).
    pub sectioned: bool,
}

impl Band {
    /// Le compte à rebours des temps (`4 · 3 · 2 · 1`) a-t-il lieu ? Il remplace l'annonce sur la
    /// **dernière** mesure avant un changement de son, et seulement en marche : à l'arrêt, un
    /// décompte figé annoncerait un départ qui ne vient pas.
    pub fn counting_down(&self) -> bool {
        self.running && self.next.as_ref().is_some_and(|n| n.sound.is_some() && n.bars_left == 1)
    }
}

impl State {
    /// La **carte des mesures** du morceau joué (§ 11). `None` : aucun set, ou rien de joué.
    pub fn bar_map(&self) -> Option<BarMap> {
        self.bar_map_of(self.set.as_ref()?.at?.0)
    }

    /// La carte des mesures d'un morceau du set, joué ou non.
    fn bar_map_of(&self, song: usize) -> Option<BarMap> {
        let spec = self.set.as_ref()?.spec.songs.get(song)?;
        Some(BarMap::of(spec, self.prompt_of(song).and_then(|l| l.prompt())))
    }

    /// **`Espace`** (réglable, `[set] prompt_play`) : lance ou arrête la battue. **L'appui est le
    /// 1** de la mesure en cours — c'est ce qui permet de se raccrocher au groupe sans compter.
    pub(in crate::browse::state) fn toggle_beat(&mut self) -> Action {
        let now = self.now;
        let Some(set) = self.set.as_ref() else {
            self.message = Some("aucun set : L pour en choisir un".into());
            return Action::None;
        };
        if set.at.is_none() {
            self.message = Some("aucun son joué : Entrée joue le morceau visé".into());
            return Action::None;
        }
        if set.beat.running {
            if let Some(set) = self.set.as_mut() {
                set.beat.running = false;
            }
            self.message = Some("battue arrêtée".into());
            return Action::None;
        }
        let (bpm, meter) = (self.heard_tempo_bpm(), self.band_meter());
        let bar = self.set.as_ref().map(|s| s.beat.bar).unwrap_or(0);
        let ticks = self.clock_elapsed_ticks().unwrap_or(0);
        if let Some(set) = self.set.as_mut() {
            set.beat.start(now, ticks);
        }
        self.message =
            Some(format!("battue : cet appui est le 1 de la mesure {} (♩{bpm:.0} {})", bar + 1, meter.time));
        Action::None
    }

    /// **`Start` du transport** (MIDI) : la battue repart du **1 de la première mesure** du
    /// morceau joué, et elle avance. Sans set ni son joué, il n'y a rien à battre : on ne fait
    /// rien plutôt que d'allumer une bande vide.
    pub(in crate::browse::state) fn beat_from_start(&mut self, now_ms: u64) {
        if self.set.as_ref().is_none_or(|s| s.at.is_none()) {
            return;
        }
        let was = self.set.as_ref().map(|s| s.prompt_section).unwrap_or(0);
        let ticks = self.clock_elapsed_ticks().unwrap_or(0);
        if let Some(set) = self.set.as_mut() {
            set.prompt_section = 0;
            set.beat.restart(now_ms, ticks);
            set.beat.running = true;
        }
        if let Some(song) = self.set.as_ref().and_then(|s| s.at).map(|at| at.0) {
            self.place_prompt(song);
        }
        self.section_tempo_on_jump(was, 0);
    }

    /// **`Stop` du transport** : la battue s'arrête là où elle est. Le tempo, lui, est gardé par
    /// l'arbitre (ADR 0008) — un arrêt du transport n'est pas une raison de changer de son.
    pub(in crate::browse::state) fn beat_stop(&mut self) {
        if let Some(set) = self.set.as_mut() {
            set.beat.running = false;
        }
    }

    /// **`Continue` du transport** : la battue **reprend où elle est** — là où `Stop` l'a laissée,
    /// ou là où un SPP vient de la poser (Ableton en mode « Song » envoie SPP puis `Continue` dès
    /// que la lecture ne part pas de 1.1.1). Les deux repères sont recalés à cet instant, parce que
    /// l'arbitre **réancre** l'horloge sur `Continue` : compté depuis l'ancre d'avant, le repère en
    /// tops attendrait que le compteur le rattrape. Sans SPP, on repart au début du temps où l'on
    /// s'était arrêté : une fraction de temps rejouée, jamais plus.
    pub(in crate::browse::state) fn beat_continue(&mut self, now_ms: u64) {
        if self.set.as_ref().is_none_or(|s| s.at.is_none()) {
            return;
        }
        let ticks = self.clock_elapsed_ticks().unwrap_or(0);
        if let Some(set) = self.set.as_mut() {
            let (bar, tick) = (set.beat.bar, set.beat.tick);
            set.beat.seek(bar, tick, now_ms, ticks);
            set.beat.running = true;
        }
    }

    /// **Song Position Pointer** : une position en doubles-croches depuis le début du morceau,
    /// convertie par la carte des mesures — c'est elle qui sait qu'une section change de mesure.
    /// Rend faux si la position tombe au-delà de ce que le morceau déclare : on le dit alors,
    /// plutôt que de poser la battue n'importe où.
    pub(in crate::browse::state) fn beat_seek_sixteenths(&mut self, sixteenths: u16, now_ms: u64) -> bool {
        if self.set.as_ref().is_none_or(|s| s.at.is_none()) {
            return false;
        }
        let Some(map) = self.bar_map() else { return false };
        let Some(pos) = map.at_sixteenths(sixteenths as u32) else { return false };
        let was = self.set.as_ref().map(|s| s.prompt_section).unwrap_or(0);
        let (line, in_line) = map.place(pos.section, pos.bar);
        let ticks = self.clock_elapsed_ticks().unwrap_or(0);
        if let Some(set) = self.set.as_mut() {
            set.prompt_section = pos.section;
            set.prompt_line = line;
            set.prompt_bar = in_line;
            set.beat.seek(pos.bar, pos.tick, now_ms, ticks);
        }
        self.section_tempo_on_jump(was, pos.section);
        true
    }

    /// **La ligne et la mesure** du prompteur d'après la section et la mesure de la battue : un
    /// saut (`Fin`, `Début`, le son, `Start`) se pose sur la première mesure de la section, qui
    /// n'est pas forcément sa première ligne — une cue la précède souvent.
    /// `song` est le morceau dont on suit le texte : celui qu'on va jouer, quand le son change.
    pub(in crate::browse::state) fn place_prompt(&mut self, song: usize) {
        let Some(set) = self.set.as_ref() else { return };
        let (section, bar) = (set.prompt_section, set.beat.bar);
        let (line, in_line) = self.bar_map_of(song).map(|m| m.place(section, bar)).unwrap_or((0, None));
        if let Some(set) = self.set.as_mut() {
            set.prompt_line = line;
            set.prompt_bar = in_line;
        }
    }

    /// Le tempo d'une section **nouvelle** entre dans l'arbitre, comme en avançant (§ 11) : même
    /// règle pour un saut que pour le défilement, sinon un `Start` ou un SPP laisserait le tempo
    /// de la section qu'on vient de quitter.
    fn section_tempo_on_jump(&mut self, was: usize, now: usize) {
        if was == now {
            return;
        }
        let Some(map) = self.bar_map() else { return };
        let before = map.span(was).and_then(|s| s.tempo_bpm);
        if let Some(bpm) = map.span(now).and_then(|s| s.tempo_bpm).filter(|b| Some(*b) != before) {
            self.set_section_tempo(bpm);
        }
    }

    /// La métrique de la mesure en cours, celle que la bande dessine.
    fn band_meter(&self) -> Meter {
        let section = self.set.as_ref().map(|s| s.prompt_section).unwrap_or(0);
        self.bar_map().map(|m| m.meter(section)).unwrap_or_default()
    }

    /// **La position avance avec l'horloge**, appelée par [`State::tick`]. Elle ne touche ni au
    /// son ni à la carte son : elle déplace la position, fait suivre le texte, et donne à l'arbitre
    /// le tempo d'une section nouvelle — comme le ferait un preset.
    pub(in crate::browse::state) fn advance_beat(&mut self, now_ms: u64) {
        let Some(set) = self.set.as_ref() else { return };
        if !set.beat.running || set.at.is_none() {
            return;
        }
        let Some(map) = self.bar_map() else { return };
        if map.is_empty() {
            return;
        }
        let bpm = self.heard_tempo_bpm();
        // **Avec une horloge, la battue compte les tops** et non les millisecondes : sa position
        // devient absolue, donc elle ne dérive plus. Intégrer un tempo mesuré transformait la
        // moindre erreur de période en décalage définitif — à 0,3 BPM près, une demi-seconde en
        // quatre minutes —, et c'est le défaut que le clic a eu, pour la même raison.
        let on_clock = self.clock_elapsed_ticks();
        let was = set.prompt_section.min(map.len() - 1);
        let (mut section, mut bar, mut tick, mut at) = (was, set.beat.bar, set.beat.tick, set.beat.at_ms);
        let mut at_ticks = set.beat.at_ticks;
        let (mut running, mut ended) = (true, false);
        let mut caught = 0;
        loop {
            let meter = map.meter(section);
            let step = (meter.tick_ms(bpm).max(1.0)) as u64;
            // Combien de tops MIDI vaut un tick de battue : une noire en x/4 (24 tops), une
            // croche en x/8 (12). Déduit du rapport des durées, pour ne pas faire entrer le
            // protocole MIDI dans la carte des mesures — et exact, ce rapport ne dépendant pas du
            // tempo.
            let per_beat = 60_000.0 / bpm;
            let step_ticks = ((meter.tick_ms(bpm) / per_beat * TICKS_PER_BEAT as f32).round() as u64).max(1);
            let due = match on_clock {
                Some(elapsed) => elapsed >= at_ticks + step_ticks,
                None => now_ms >= at.saturating_add(step),
            };
            if !due {
                break;
            }
            at += step;
            at_ticks += step_ticks;
            caught += 1;
            if caught > CATCH_UP_TICKS {
                at = now_ms;
                if let Some(elapsed) = on_clock {
                    at_ticks = elapsed;
                }
                break;
            }
            tick += 1;
            if tick < meter.ticks() {
                continue;
            }
            tick = 0;
            bar += 1;
            // une section de durée inconnue ne finit pas d'elle-même : on y compte les mesures
            let Some(len) = map.span(section).and_then(|s| s.bars) else { continue };
            if bar < len {
                continue;
            }
            if section + 1 < map.len() {
                section += 1;
                bar = 0;
            } else {
                // **fin du morceau** : la battue s'arrête sur le dernier temps, et pas au début de
                // la mesure suivante — on doit voir où le morceau s'est terminé
                bar = len.saturating_sub(1);
                tick = meter.ticks().saturating_sub(1);
                running = false;
                ended = true;
                break;
            }
        }
        let (line, in_line) = map.place(section, bar);
        if let Some(set) = self.set.as_mut() {
            set.beat.bar = bar;
            set.beat.tick = tick;
            set.beat.at_ms = at;
            set.beat.at_ticks = at_ticks;
            set.beat.running = running;
            if set.prompt_section != section {
                set.prompt_section = section;
            }
            set.prompt_line = line;
            set.prompt_bar = in_line;
        }
        // **le tempo de la section entre dans l'arbitre comme celui d'un preset** (§ 11) : il n'y
        // entre qu'au passage d'une section à l'autre, et seulement s'il change — sinon chaque
        // mesure annulerait le tap, que le § 11 veut justement plus fort que le fichier.
        if section != was {
            let before = map.span(was).and_then(|s| s.tempo_bpm);
            if let Some(bpm) = map.span(section).and_then(|s| s.tempo_bpm).filter(|b| Some(*b) != before) {
                self.set_section_tempo(bpm);
            }
        }
        if ended {
            self.message = Some("fin du morceau : la battue s'arrête".into());
        }
    }

    /// Ce que la bande montre (§ 11). `None` : aucun son joué — il n'y a pas de morceau à battre.
    pub fn band(&self) -> Option<Band> {
        let set = self.set.as_ref()?;
        let at = set.at?;
        let map = self.bar_map()?;
        let section = set.prompt_section.min(map.len().saturating_sub(1));
        let here = map.span(section)?;
        let playing = set.spec.songs.get(at.0).and_then(|s| s.sounds.get(at.1)).map(|s| s.preset.as_str());
        let sectioned = !here.label.is_empty() || map.len() > 1;
        // **le groupe** : les sections voisines qui portent le même nom (§ 4 de `prompteur.md`)
        let same = |a: usize, b: usize| {
            let (x, y) = (&map.spans()[a].label, &map.spans()[b].label);
            super::prompt::same_name(x, y)
        };
        let first = (0..section).rev().take_while(|i| same(*i, i + 1)).last().unwrap_or(section);
        let end =
            (section + 1..map.len()).take_while(|i| same(*i - 1, *i)).last().map_or(section + 1, |i| i + 1);
        let bars_of =
            |r: std::ops::Range<usize>| r.map(|i| map.span(i).and_then(|s| s.bars)).sum::<Option<u32>>();
        let group_bar = bars_of(first..section).unwrap_or(0) + set.beat.bar;
        let group_bars = bars_of(first..end);
        let ahead = map.span(end).map(|n| Ahead {
            label: n.label.clone(),
            bars_left: group_bars.map(|b| b.saturating_sub(group_bar)),
        });
        let next = map.span(section + 1).map(|n| Next {
            inside: section + 1 < end,
            label: n.label.clone(),
            // sans durée connue, la section suivante ne vient jamais d'elle-même : on annonce
            // « dans 0 mes. » plutôt que de mentir sur un nombre
            bars_left: here.bars.map(|b| b.saturating_sub(set.beat.bar)).unwrap_or(0),
            sound: n.sound.clone().filter(|s| Some(s.as_str()) != playing),
            meter: Some(n.meter.clone()).filter(|m| *m != here.meter),
            tempo_bpm: n.tempo_bpm.filter(|t| Some(*t) != here.tempo_bpm),
        });
        Some(Band {
            running: set.beat.running,
            tempo_bpm: self.heard_tempo_bpm(),
            meter: here.meter.clone(),
            bar: set.beat.bar,
            bars: here.bars,
            tick: set.beat.tick,
            label: here.label.clone(),
            group_bar,
            group_bars,
            next,
            ahead,
            sectioned,
        })
    }

    /// La mesure du morceau joué, pour la ligne de la set list et l'en-tête : celle de la **mesure
    /// en cours** quand la battue tourne dans un morceau qui change de mesure.
    pub fn band_time_sig(&self) -> TimeSig {
        self.band().map(|b| b.meter.time).unwrap_or(TimeSig::COMMON)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::state::chain;
    use crate::browse::state::set::prompt::Loaded;
    use crate::browse::state::set::ActiveSet;
    use crate::browse::state::tests::with_chain;
    use crate::tui::Key;
    use library::prompt::Prompt;
    use library::set::{SetSpec, Song, Sound};
    use std::path::PathBuf;

    /// Le morceau des tests : 2 mesures de 4/4 liées à `clean`, 1 de 4/4 liée à `fuzz`, puis
    /// 1 mesure de 7/8 (2+2+3) liée à `clean`. À ♩120, la noire vaut 500 ms et la croche 250.
    const CHO: &str = "{title: Le Phare}\n{tempo: 120}\n{time: 4/4}\n\
        {start_of_verse: Couplet 1}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 2}\na\nb\n{end_of_verse}\n\
        {start_of_chorus: Refrain}\n{x_namfoop_sound: fuzz}\n{x_namfoop_bars: 1}\nc\n{end_of_chorus}\n\
        {time: 7/8}\n{x_namfoop_beats: 2+2+3}\n\
        {start_of_bridge: Pont}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 1}\nd\n{end_of_bridge}\n";

    /// Un set de deux morceaux : « Le Phare » (deux sons, son prompteur) et « Néon » (sans
    /// prompteur), dans l'espace Set.
    fn staged(cho: &str) -> State {
        let mut s = with_chain(vec![chain::Block::Leaf(chain::Leaf::Nam { model: Some(0), on: true })]);
        let spec = |name: &str| {
            let t = with_chain(vec![chain::Block::Leaf(chain::Leaf::Nam { model: Some(0), on: true })]);
            t.to_preset(name)
        };
        s.adopt_presets(vec![
            ("p1".into(), "clean".into(), "/x/clean.preset.toml".into(), spec("clean")),
            ("p2".into(), "fuzz".into(), "/x/fuzz.preset.toml".into(), spec("fuzz")),
        ]);
        let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
        let set = SetSpec {
            name: "Concert".into(),
            base: PathBuf::from("/x/sets"),
            songs: vec![
                Song {
                    title: Some("Le Phare".into()),
                    tempo_bpm: Some(120.0),
                    prompt: Some(PathBuf::from("prompts/le-phare.cho")),
                    sounds: vec![sound("clean"), sound("fuzz")],
                    ..Default::default()
                },
                Song {
                    title: Some("Néon".into()),
                    tempo_bpm: Some(90.0),
                    time: Some("6/8".into()),
                    sounds: vec![sound("clean")],
                    ..Default::default()
                },
            ],
        };
        let names: Vec<String> = s.presets.iter().map(|p| p.name.clone()).collect();
        let find = |q: &str| names.iter().position(|n| n == q);
        s.set = Some(ActiveSet::new("s".into(), "concert".into(), set, &find));
        s.adopt_prompts(vec![(
            PathBuf::from("/x/sets/prompts/le-phare.cho"),
            Loaded::Ok(Prompt::parse(cho)),
        )]);
        s.mark_start();
        s.on_key(Key::Char('3'));
        s.play_set_sound((0, 0));
        s
    }

    /// L'horloge comme la donne la boucle de `browse` : une image toutes les 30 ms. Avancer par
    /// grands sauts mesurerait le **rattrapage** au lieu du défilement.
    fn run_to(s: &mut State, to_ms: u64) {
        let mut t = s.now;
        while t < to_ms {
            t = (t + 30).min(to_ms);
            s.tick(t);
        }
    }

    fn pos(s: &State) -> (usize, u32, u32) {
        let set = s.set.as_ref().expect("un set");
        (set.prompt_section, set.beat.bar, set.beat.tick)
    }

    fn running(s: &State) -> bool {
        s.set.as_ref().is_some_and(|x| x.beat.running)
    }

    /// **`Espace` lance et arrête**, et l'appui est le 1 de la mesure en cours (§ 11).
    #[test]
    fn la_battue_suit_les_tops_et_non_les_millisecondes() {
        use midi_io::Event;
        let mut s = staged(CHO);
        // une horloge à 120 BPM, mais dont les tops sont espacés de **600 ms de battue** par
        // noire au lieu de 500 : les millisecondes et les tops ne disent donc pas la même chose,
        // et c'est aux tops que la battue doit obéir
        let faux_pas = 600_000 / TICKS_PER_BEAT as u64; // µs par top
        let mut at_us = 1_000_000u64;
        let mut seq = 0u64;
        let souffler = |s: &mut State, tops: u32, at_us: &mut u64, seq: &mut u64| {
            for _ in 0..tops {
                *at_us += faux_pas;
                *seq += 1;
                s.on_midi(Event::ClockTick, *at_us, *seq);
                s.tick(*at_us / 1000);
            }
        };
        // deux noires de tops verrouillent l'horloge
        souffler(&mut s, 2 * TICKS_PER_BEAT, &mut at_us, &mut seq);
        assert_eq!(s.tempo_source(), engine::tempo::TempoSource::MidiClock, "horloge verrouillée");

        s.on_key(Key::Char(' '));
        assert!(running(&s));
        assert_eq!(pos(&s), (0, 0, 0), "l'appui EST le 1");

        // 23 tops : on est encore sur le même temps, **même si 575 ms se sont écoulées** — la
        // noire du tempo publié (500 ms) est pourtant déjà passée
        souffler(&mut s, TICKS_PER_BEAT - 1, &mut at_us, &mut seq);
        assert_eq!(pos(&s), (0, 0, 0), "la battue attend le 24e top, pas la noire des ms");
        // le 24e le fait avancer
        souffler(&mut s, 1, &mut at_us, &mut seq);
        assert_eq!(pos(&s), (0, 0, 1), "un temps par 24 tops");

        // et au bout de huit noires de tops, on est exactement là où les tops le disent : la
        // mesure 3 (le refrain), premier temps — 2 mesures de 4/4 puis le début de la 3e
        souffler(&mut s, 7 * TICKS_PER_BEAT, &mut at_us, &mut seq);
        assert_eq!(pos(&s), (1, 0, 0), "8 noires de tops = section 2, mesure 1, temps 1");
    }

    /// Une horloge à ♩120 vue par la boucle : chaque top passe par `on_midi`, puis un tour de
    /// boucle. Le top dure **un rien plus** que 500 ms / 24, pour qu'avant le verrou — la battue
    /// compte alors les millisecondes — le 24ᵉ top tombe bien après la noire.
    struct Horloge {
        at_us: u64,
        seq: u64,
    }

    impl Horloge {
        const TOP_US: u64 = 20_834;

        fn tops(&mut self, s: &mut State, n: u32) {
            for _ in 0..n {
                self.at_us += Self::TOP_US;
                self.seq += 1;
                s.on_midi(midi_io::Event::ClockTick, self.at_us, self.seq);
                s.tick(self.at_us / 1000);
            }
        }

        fn send(&self, s: &mut State, e: midi_io::Event) {
            s.on_midi(e, self.at_us, self.seq);
        }
    }

    /// **`Stop` puis `Continue` : la battue reprend où elle s'était arrêtée**, et non au 1 du
    /// morceau. L'arbitre réancre l'horloge sur `Continue` : la battue doit quand même avancer
    /// dès les premiers tops, puis compter juste une fois le verrou retrouvé.
    #[test]
    fn continue_resumes_the_beat_where_stop_left_it() {
        use midi_io::Event;
        let mut s = staged(CHO);
        let mut h = Horloge { at_us: 1_000_000, seq: 0 };
        h.tops(&mut s, 2 * TICKS_PER_BEAT);
        h.send(&mut s, Event::ClockStart);
        // le temps 1 est le **premier top** après le transport (norme MIDI) : cinq noires plus
        // loin, c'est le 121ᵉ
        h.tops(&mut s, 5 * TICKS_PER_BEAT + 1);
        assert_eq!(pos(&s), (0, 1, 1), "cinq noires : mesure 2, temps 2");

        h.send(&mut s, Event::ClockStop);
        assert!(!running(&s));
        // Ableton peut continuer d'envoyer des tops à l'arrêt : la battue n'en bouge pas
        h.tops(&mut s, 3 * TICKS_PER_BEAT);
        assert_eq!(pos(&s), (0, 1, 1), "arrêtée, elle ne bouge plus");

        h.send(&mut s, Event::ClockContinue);
        assert!(running(&s), "Continue relance la battue");
        assert_eq!(pos(&s), (0, 1, 1), "là où Stop l'a laissée, pas au 1 du morceau");
        h.tops(&mut s, TICKS_PER_BEAT + 1);
        assert_eq!(pos(&s), (0, 1, 2), "elle avance dès la première noire, avant le verrou");
        // deux noires de plus : le verrou est retrouvé, la battue compte alors les tops
        h.tops(&mut s, 2 * TICKS_PER_BEAT);
        assert_eq!(pos(&s), (1, 0, 0), "fin du couplet : le Refrain, sur son 1");
        h.tops(&mut s, 3 * TICKS_PER_BEAT);
        assert_eq!(pos(&s), (1, 0, 3), "et elle suit les tops sans glisser");
    }

    /// **Ableton lancé au milieu du morceau** (mode « Song ») : SPP puis `Continue`. La battue part
    /// de la position reçue — c'est le cas le plus courant en répétition.
    #[test]
    fn a_song_position_then_continue_starts_from_that_position() {
        use midi_io::Event;
        let mut s = staged(CHO);
        let mut h = Horloge { at_us: 1_000_000, seq: 0 };
        h.tops(&mut s, 2 * TICKS_PER_BEAT);
        h.send(&mut s, Event::ClockStop);
        // 20 doubles-croches : la mesure 2 du couplet, sur son 2ᵉ temps
        h.send(&mut s, Event::SongPosition(20));
        assert_eq!(pos(&s), (0, 1, 1));
        assert!(!running(&s), "le SPP pose la battue sans la lancer");

        h.send(&mut s, Event::ClockContinue);
        assert!(running(&s));
        h.tops(&mut s, 2 * TICKS_PER_BEAT + 1);
        assert_eq!(pos(&s), (0, 1, 3), "deux noires plus loin que la position reçue");
    }

    /// `Continue` ne lance rien **sans son joué**, ni **horloge coupée** (`[midi] clock = false`) :
    /// la battue reste au pied.
    #[test]
    fn continue_does_nothing_without_a_sound_or_with_the_clock_off() {
        use midi_io::Event;
        let mut s = staged(CHO);
        s.midi.clock = false;
        s.on_midi(Event::ClockContinue, 1_000_000, 0);
        assert!(!running(&s), "horloge coupée : Continue n'a rien à dire à la battue");

        let mut s = staged(CHO);
        if let Some(set) = s.set.as_mut() {
            set.at = None;
        }
        s.on_midi(Event::ClockContinue, 1_000_000, 0);
        assert!(!running(&s), "aucun son joué : rien à battre");
    }

    /// Sans horloge, **rien ne change** : la battue compte les millisecondes comme avant.
    #[test]
    fn space_starts_the_beat_and_the_press_is_the_one() {
        let mut s = staged(CHO);
        assert!(!running(&s), "éteinte au départ : rien ne s'allume");
        run_to(&mut s, 1234);
        s.on_key(Key::Char(' '));
        assert!(running(&s));
        assert_eq!(pos(&s), (0, 0, 0), "l'appui EST le 1");
        let m = s.message.clone().unwrap_or_default();
        assert!(m.contains("mesure 1") && m.contains("♩120 4/4"), "{m}");
        // le premier temps dure une noire pleine depuis l'appui, et pas depuis une grille absolue
        run_to(&mut s, 1234 + 499);
        assert_eq!(pos(&s), (0, 0, 0));
        run_to(&mut s, 1234 + 500);
        assert_eq!(pos(&s), (0, 0, 1));
        s.on_key(Key::Char(' '));
        assert!(!running(&s));
        assert_eq!(s.message.as_deref(), Some("battue arrêtée"));
        // arrêtée, elle ne bouge plus
        run_to(&mut s, 9000);
        assert_eq!(pos(&s), (0, 0, 1));
    }

    /// **Le défilement** : la position avance temps par temps, puis mesure par mesure, et passe
    /// d'une section à l'autre au bout des mesures de celle-ci.
    #[test]
    fn the_position_advances_by_beats_then_bars_then_sections() {
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        for (ms, want) in [(500, (0, 0, 1)), (1000, (0, 0, 2)), (1500, (0, 0, 3))] {
            run_to(&mut s, ms);
            assert_eq!(pos(&s), want, "à {ms} ms");
        }
        run_to(&mut s, 2000);
        assert_eq!(pos(&s), (0, 1, 0), "la mesure 2 de la section");
        // 2 mesures de 4/4 = 4 s : la section suivante commence
        run_to(&mut s, 4000);
        assert_eq!(pos(&s), (1, 0, 0), "fin de section : le Refrain, sur son 1");
        assert_eq!(s.set.as_ref().unwrap().prompt_line, 0, "et la première de ses lignes");
        // le Refrain fait 1 mesure, puis le Pont est en **7/8** : ses ticks valent 250 ms
        run_to(&mut s, 6000);
        assert_eq!(pos(&s), (2, 0, 0), "le Pont");
        assert_eq!(s.band().expect("la bande").meter.groups, vec![2, 2, 3], "en 7/8 (2+2+3)");
        run_to(&mut s, 6250);
        assert_eq!(pos(&s), (2, 0, 1), "une croche, pas une noire");
    }

    /// **Les lignes se partagent les mesures de leur section** (§ 10) : c'est ce qui fait défiler
    /// le texte au tempo, sans que rien ne le pousse à la main.
    #[test]
    fn the_text_scrolls_with_the_bars() {
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        assert_eq!(s.set.as_ref().unwrap().prompt_line, 0);
        // la section a 2 lignes sur 2 mesures : la 2e mesure chante la 2e ligne
        run_to(&mut s, 2000);
        assert_eq!((pos(&s), s.set.as_ref().unwrap().prompt_line), ((0, 1, 0), 1));
    }

    /// **Fin du morceau** : la battue s'arrête, sur le **dernier** temps de la dernière mesure —
    /// on doit voir où le morceau s'est terminé, pas le début d'une mesure qui n'existe pas.
    #[test]
    fn the_end_of_the_song_stops_the_beat_on_the_last_beat() {
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        // 2 mesures de 4/4 + 1 de 4/4 + 1 de 7/8 = 4 s + 2 s + 1,75 s
        run_to(&mut s, 7750);
        assert!(!running(&s), "la battue s'est arrêtée");
        assert_eq!(pos(&s), (2, 0, 6), "sur la dernière croche du 7/8");
        assert_eq!(s.message.as_deref(), Some("fin du morceau : la battue s'arrête"));
        // et elle ne repart pas d'elle-même
        run_to(&mut s, 20_000);
        assert_eq!(pos(&s), (2, 0, 6));
    }

    /// **En marche, la position suit le temps ; à l'arrêt, elle suit le son** (§ 10, § 14) : c'est
    /// la règle qui empêche un son joué un temps trop tôt de décaler tout le texte.
    #[test]
    fn running_the_position_follows_time_stopped_it_follows_the_sound() {
        // à l'arrêt : jouer « fuzz » amène à sa section
        let mut s = staged(CHO);
        s.play_set_sound((0, 1));
        assert_eq!(pos(&s), (1, 0, 0), "à l'arrêt, la position suit le son");
        // en marche : le même changement de son ne déplace rien
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        run_to(&mut s, 1000);
        assert_eq!(pos(&s), (0, 0, 2));
        s.play_set_sound((0, 1));
        assert_eq!(pos(&s), (0, 0, 2), "en marche, changer de son ne déplace PAS la position");
        assert!(running(&s), "et n'arrête pas la battue");
    }

    /// **`Fin` / `Début` recalent sur le 1 de la section** (§ 11), en marche comme à l'arrêt :
    /// c'est le geste du pied quand le groupe n'est pas là où le texte le dit.
    #[test]
    fn the_foot_keys_recalibrate_on_the_one_of_the_section() {
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        run_to(&mut s, 2600);
        assert_eq!(pos(&s), (0, 1, 1));
        s.on_key(Key::End);
        assert_eq!(pos(&s), (1, 0, 0), "la section suivante, sur son 1");
        assert!(running(&s), "sans arrêter la battue");
        // et le temps repart de cet instant, pas de la grille d'avant
        run_to(&mut s, 2600 + 499);
        assert_eq!(pos(&s), (1, 0, 0));
        run_to(&mut s, 2600 + 530);
        assert_eq!(pos(&s), (1, 0, 1));
        s.on_key(Key::Home);
        assert_eq!(pos(&s), (0, 0, 0), "et vers l'arrière de même");
    }

    /// **Le tap recale le temps, pas la mesure** (§ 11) : on se raccroche au groupe sans perdre le
    /// compte des mesures de la section.
    #[test]
    fn the_tap_recalibrates_the_beat_but_not_the_bar() {
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        run_to(&mut s, 2600);
        assert_eq!(pos(&s), (0, 1, 1));
        s.on_key(Key::Char('*'));
        assert_eq!(pos(&s), (0, 1, 0), "le temps 1 — et toujours la mesure 2");
    }

    /// **Changer de morceau remet au début et arrête** (§ 11) : c'est un autre morceau, avec sa
    /// mesure et son tempo.
    #[test]
    fn changing_song_rewinds_to_the_start_and_stops() {
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        run_to(&mut s, 2600);
        s.play_set_sound((1, 0)); // « Néon », sans prompteur
        assert!(!running(&s), "la battue s'arrête");
        assert_eq!(pos(&s), (0, 0, 0), "et repart du début du morceau");
        // **un morceau sans prompteur a sa bande quand même** (§ 11) : mesure et tempo du morceau
        let band = s.band().expect("la bande");
        assert_eq!(band.meter.time, "6/8".parse().unwrap());
        assert_eq!(band.meter.groups, vec![3, 3]);
        assert!(!band.sectioned, "pas de sections : on compte les mesures");
        assert_eq!((band.bars, band.next), (None, None));
        // et elle compte bien les mesures : 6 croches à ♩90 = 2 s
        s.on_key(Key::Char(' '));
        run_to(&mut s, 2600 + 2000);
        assert_eq!(pos(&s), (0, 1, 0), "la mesure 2, sans fin de section");
    }

    /// **Le compte à rebours de changement de son** (§ 11) : il n'a lieu que si la section
    /// suivante est liée à un **autre** son que celui joué, et passe au décompte des temps sur la
    /// dernière mesure.
    #[test]
    fn the_countdown_warns_before_a_sound_change() {
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        let band = s.band().expect("la bande");
        assert_eq!(band.label, "Couplet 1");
        assert_eq!(band.bars, Some(2));
        let next = band.next.expect("le Refrain vient");
        assert_eq!((next.label.as_str(), next.bars_left), ("Refrain", 2));
        assert_eq!(next.sound.as_deref(), Some("fuzz"), "lié à un AUTRE son : on prévient");
        assert!(!s.band().unwrap().counting_down(), "pas encore : il reste 2 mesures");
        // la dernière mesure : le décompte des temps
        run_to(&mut s, 2000);
        let band = s.band().expect("la bande");
        assert_eq!(band.next.as_ref().map(|n| n.bars_left), Some(1));
        assert!(band.counting_down(), "dernière mesure : 4 · 3 · 2 · 1");
        // joué, le son visé fait disparaître le compte à rebours — il n'y a plus rien à changer
        s.play_set_sound((0, 1));
        let band = s.band().expect("la bande");
        assert_eq!(band.next.as_ref().and_then(|n| n.sound.clone()), None);
        assert!(!band.counting_down());
    }

    /// **Un changement de mesure et de tempo est annoncé** (§ 11), et le tempo de la section entre
    /// dans l'arbitre **comme celui d'un preset** : un tap le recouvre ensuite.
    #[test]
    fn a_meter_or_tempo_change_is_announced_and_reaches_the_arbiter() {
        let cho = "{tempo: 120}\n{time: 4/4}\n\
            {start_of_verse: Couplet}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 1}\na\n{end_of_verse}\n\
            {time: 7/8}\n{x_namfoop_beats: 2+2+3}\n{tempo: 92}\n\
            {start_of_bridge: Pont}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 1}\nb\n{end_of_bridge}\n";
        let mut s = staged(cho);
        assert_eq!(s.heard_tempo_bpm(), 120.0, "le tempo du morceau");
        let next = s.band().expect("la bande").next.expect("le Pont");
        assert_eq!(next.meter.map(|m| m.time.to_string()).as_deref(), Some("7/8"), "passe en 7/8");
        assert_eq!(next.tempo_bpm, Some(92.0), "♩92");
        assert_eq!(next.sound.as_deref(), None, "le même son : pas de compte à rebours");
        s.on_key(Key::Char(' '));
        run_to(&mut s, 2000);
        assert_eq!(pos(&s), (1, 0, 0), "le Pont");
        assert_eq!(s.heard_tempo_bpm(), 92.0, "son tempo est entré dans l'arbitre");
        assert_eq!(s.tempo_source(), engine::tempo::TempoSource::Preset, "au rang d'un preset");
        // et il ne touche pas au preset : rien n'est « non enregistré »
        assert!(s.unsaved().is_empty(), "{:?}", s.unsaved());
    }

    /// **Une section en grille** (§ 3 de `docs/design/prompteur.md`) : sa durée se compte dans sa
    /// grille, chaque ligne dure ses mesures, et la mesure dans la ligne avance avec la battue.
    /// `Fin` se pose sur la première **mesure** de la section suivante, pas sur sa cue.
    #[test]
    fn a_grid_section_moves_line_by_line_and_bar_by_bar() {
        // ♩120 en 4/4 : 2 s par mesure. Une ligne de 3 mesures, puis une d'une seule, puis 2/4.
        let cho = "{tempo: 120}\n{time: 4/4}\n\
            {start_of_grid label=\"Pont\"}\n| Am . . . | F . . . | C . . . |\n| G . . . |\n{end_of_grid}\n\
            {comment: regarde le batteur}\n\
            {start_of_grid label=\"Fin\"}\n| Em . . . | Em . . . |\n{end_of_grid}\n";
        let mut s = staged(cho);
        let place = |s: &State| {
            let set = s.set.as_ref().unwrap();
            (set.prompt_section, set.prompt_line, set.prompt_bar)
        };
        assert_eq!(place(&s), (0, 0, Some(0)), "à l'arrêt : la première mesure");
        s.on_key(Key::Char(' '));
        let mut seen = Vec::new();
        for bar in 0..6u64 {
            run_to(&mut s, bar * 2000 + 10);
            seen.push(place(&s));
        }
        assert_eq!(
            seen,
            vec![
                (0, 0, Some(0)),
                (0, 0, Some(1)),
                (0, 0, Some(2)),
                (0, 1, Some(0)),
                // la section suivante : sa cue (ligne 0) ne dure rien
                (1, 1, Some(0)),
                (1, 1, Some(1)),
            ],
            "ligne et mesure à chaque mesure, sans x_namfoop_bars"
        );
        let band = s.band().expect("une bande");
        assert_eq!((band.label.as_str(), band.bars), ("Fin", Some(2)));
        // `Fin` / `Début` : sur la première mesure, après la cue
        s.on_key(Key::Char(' '));
        s.on_key(Key::Home);
        assert_eq!(place(&s), (0, 0, Some(0)));
        s.on_key(Key::End);
        assert_eq!(place(&s), (1, 1, Some(0)), "la cue n'est pas une mesure");
    }

    /// **La bande compte dans le groupe** de sections voisines de même nom, celui que le prompteur
    /// montre comme une seule : la mesure sur la durée du groupe, la section annoncée est celle
    /// d'après, et un changement de mesure **à l'intérieur** se dit avec son propre compte.
    #[test]
    fn the_band_counts_in_the_group_of_same_named_sections() {
        let cho = "{tempo: 120}\n{time: 4/4}\n\
            {start_of_grid label=\"Refrain\"}\n| E . . . | A . . . |\n{end_of_grid}\n\
            {time: 2/4}\n{start_of_grid label=\"Refrain\"}\n| D E | E . | E . |\n{end_of_grid}\n\
            {time: 4/4}\n{start_of_grid label=\"Refrain\"}\n| D . . . |\n{end_of_grid}\n\
            {start_of_grid label=\"Couplet\"}\n| Em . . . |\n{end_of_grid}\n";
        let mut s = staged(cho);
        s.on_key(Key::End); // la grille en 2/4
        let band = s.band().expect("une bande");
        assert_eq!((band.label.as_str(), band.bar, band.bars), ("Refrain", 0, Some(3)), "la section");
        assert_eq!((band.group_bar, band.group_bars), (2, Some(6)), "et sa place dans le refrain entier");
        let next = band.next.as_ref().expect("la grille suivante");
        assert!(next.inside, "elle est du même refrain");
        assert_eq!((next.bars_left, next.meter.as_ref().map(|m| m.time)), (3, Some(TimeSig::COMMON)));
        let ahead = band.ahead.as_ref().expect("la section d'après le refrain");
        assert_eq!((ahead.label.as_str(), ahead.bars_left), ("Couplet", Some(4)));
        // la dernière section : pas d'après, pas de suivante
        s.on_key(Key::End);
        s.on_key(Key::End);
        let band = s.band().unwrap();
        assert_eq!((band.group_bar, band.group_bars, band.ahead), (0, Some(1), None));
    }

    /// **Sans set, ou sans son joué, `Espace` le dit** au lieu de ne rien faire (ADR 0009).
    #[test]
    fn without_a_sound_the_beat_says_why_it_cannot_start() {
        let mut s = with_chain(vec![]);
        s.on_key(Key::Char('3'));
        s.on_key(Key::Char(' '));
        assert_eq!(s.message.as_deref(), Some("aucun set : L pour en choisir un"));
        let mut s = staged(CHO);
        if let Some(set) = s.set.as_mut() {
            set.at = None;
        }
        s.on_key(Key::Char(' '));
        assert_eq!(s.message.as_deref(), Some("aucun son joué : Entrée joue le morceau visé"));
        assert!(s.band().is_none(), "rien de joué : pas de bande");
    }

    /// **Une pause longue ne fait pas défiler le morceau en accéléré** : au-delà du rattrapage
    /// prévu (un `$EDITOR` qui rend la main, un modèle qui se charge), la battue se **recale** sur
    /// l'horloge et garde la position atteinte. L'alternative — rejouer chaque temps manqué —
    /// ferait traverser le morceau d'un coup.
    #[test]
    fn a_long_pause_resynchronises_instead_of_fast_forwarding() {
        let mut s = staged(CHO);
        s.on_key(Key::Char(' '));
        s.tick(600_000); // dix minutes plus tard
        assert!(running(&s), "la battue tourne toujours");
        let (section, _, _) = pos(&s);
        assert!(section < 2, "elle n'a pas traversé le morceau : section {section}");
        // et elle repart normalement de là
        let was = pos(&s);
        s.tick(600_000 + 500);
        assert_ne!(pos(&s), was, "elle avance de nouveau");
    }
}
