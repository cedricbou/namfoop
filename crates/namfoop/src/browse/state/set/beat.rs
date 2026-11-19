//! **La battue** (§ 11 de `docs/design/set.md`) : où l'on en est dans le morceau — la section, la
//! mesure, le temps —, et ce que la bande en tête du prompteur doit montrer.
//!
//! L'**horloge est fournie** par l'appelant, comme partout dans `browse::state` : la position
//! avance dans [`State::tick`], et les tests donnent les millisecondes qu'ils veulent. Aucun fil,
//! aucun `Instant`, et **rien dans le fil audio** — la bande est dessinée par le fil de
//! l'interface, et le tempo de section entre dans l'arbitre comme celui d'un preset.
//!
//! La carte des mesures, elle, est dans `library::bars` (pure) : ici il n'y a que la position et
//! ce qui la déplace.
//!
//! **La règle qui tient tout** (§ 10, § 14) : *en marche, la position suit le **temps*** — changer
//! de son ne la déplace pas, sinon un son joué un temps trop tôt décalerait tout le texte ; *à
//! l'arrêt, elle suit le **son***.

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
}

impl Beat {
    /// **Recaler sur le 1** d'une mesure, à cet instant : `Fin` / `Début` (le 1 de la section) et
    /// un changement de morceau s'en servent.
    pub fn restart(&mut self, now_ms: u64) {
        self.bar = 0;
        self.tick = 0;
        self.at_ms = now_ms;
    }

    /// **Le tap recale le temps, pas la mesure** (§ 11) : on repart sur un 1 de temps sans toucher
    /// au numéro de la mesure en cours.
    pub fn realign(&mut self, now_ms: u64) {
        self.tick = 0;
        self.at_ms = now_ms;
    }

    /// `Espace` : l'appui **est** le 1 de la mesure en cours.
    fn start(&mut self, now_ms: u64) {
        self.running = true;
        self.realign(now_ms);
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
    pub next: Option<Next>,
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
        let set = self.set.as_ref()?;
        let song = set.at.map(|(i, _)| i)?;
        let spec = set.spec.songs.get(song)?;
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
        if let Some(set) = self.set.as_mut() {
            set.beat.start(now);
        }
        self.message =
            Some(format!("battue : cet appui est le 1 de la mesure {} (♩{bpm:.0} {})", bar + 1, meter.time));
        Action::None
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
        let was = set.prompt_section.min(map.len() - 1);
        let (mut section, mut bar, mut tick, mut at) = (was, set.beat.bar, set.beat.tick, set.beat.at_ms);
        let (mut running, mut ended) = (true, false);
        let mut caught = 0;
        loop {
            let meter = map.meter(section);
            let step = (meter.tick_ms(bpm).max(1.0)) as u64;
            if now_ms < at.saturating_add(step) {
                break;
            }
            at += step;
            caught += 1;
            if caught > CATCH_UP_TICKS {
                at = now_ms;
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
        let line = map.line_of(section, bar);
        if let Some(set) = self.set.as_mut() {
            set.beat.bar = bar;
            set.beat.tick = tick;
            set.beat.at_ms = at;
            set.beat.running = running;
            if set.prompt_section != section {
                set.prompt_section = section;
            }
            set.prompt_line = line;
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
        let next = map.span(section + 1).map(|n| Next {
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
            next,
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
