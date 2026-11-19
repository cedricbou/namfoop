//! **La carte des mesures** d'un morceau (§ 11 de `docs/design/set.md`) : ce qui place un instant
//! du morceau dans une section, une mesure et un temps.
//!
//! Aucun pédalier ne tient la mesure — les Helix, Quad Cortex et autres ne connaissent qu'un
//! tempo, et l'horloge MIDI ne transmet pas la mesure (son Song Position Pointer compte des
//! **doubles croches** depuis le début). C'est donc namfoop qui tient cette carte, tirée du
//! `.set.toml` et du prompteur.
//!
//! **Le BPM se compte toujours à la noire**, comme Live et l'horloge MIDI (24 tops par noire) :
//! en 6/8 à ♩ = 82, on bat deux noires pointées. Le **tick** de la bande, lui, est l'unité de la
//! mesure : la croche en x/8, la noire en x/4.
//!
//! Module **pur** : ni écran, ni horloge, ni tempo entendu. Le tempo est un argument des durées
//! ([`Meter::tick_ms`]), parce que le tempo réellement entendu est arbitré ailleurs (un tap, une
//! horloge MIDI le recouvrent) ; la carte, elle, ne porte que ce que les fichiers déclarent.

use crate::prompt::Prompt;
use crate::set::{Song, TimeSig};

/// La **métrique** d'une mesure : sa mesure, et le regroupement de ses temps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meter {
    pub time: TimeSig,
    /// Les **groupes** de ticks, un par temps battu : `[1,1,1,1]` en 4/4, `[3,3]` en 6/8,
    /// `[2,2,3]` en 7/8. Leur somme est le nombre de ticks de la mesure.
    pub groups: Vec<u8>,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new(TimeSig::COMMON, None)
    }
}

impl Meter {
    /// La métrique d'une mesure, avec le regroupement de `{x_namfoop_beats}` s'il est **cohérent**
    /// avec elle. Un `2+2+3` écrit sous un `4/4` est ignoré plutôt que de produire une mesure de
    /// sept ticks dans une mesure de quatre : la mesure fait foi, le regroupement l'orne.
    pub fn new(time: TimeSig, groups: Option<&[u8]>) -> Self {
        let sum = |g: &[u8]| g.iter().map(|x| *x as u32).sum::<u32>();
        let groups = match groups {
            Some(g) if !g.is_empty() && sum(g) == time.beats => g.to_vec(),
            _ => default_groups(time),
        };
        Self { time, groups }
    }

    /// Le nombre de **ticks** de la mesure : des croches en x/8, des noires en x/4.
    pub fn ticks(&self) -> u32 {
        self.time.beats
    }

    /// Le nombre de temps **battus** : 4 en 4/4, 2 en 6/8, 3 en 7/8 (2+2+3).
    pub fn beats(&self) -> usize {
        self.groups.len()
    }

    /// Les ticks qui **commencent** un temps battu, dans l'ordre : `[0,1,2,3]` en 4/4, `[0,3]` en
    /// 6/8, `[0,2,4]` en 7/8 (2+2+3).
    pub fn starts(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.groups.len());
        let mut acc = 0;
        for g in &self.groups {
            out.push(acc);
            acc += *g as u32;
        }
        out
    }

    /// Le temps battu auquel ce tick appartient (0 pour le temps 1).
    pub fn beat_of(&self, tick: u32) -> usize {
        self.starts().iter().filter(|s| **s <= tick).count().saturating_sub(1)
    }

    /// Ce tick commence-t-il un temps battu ? Sinon c'est une **subdivision**, que la bande montre
    /// en point gris.
    pub fn is_beat(&self, tick: u32) -> bool {
        self.starts().contains(&tick)
    }

    /// La durée d'un tick (ms) au tempo donné, **compté à la noire**.
    pub fn tick_ms(&self, bpm: f32) -> f32 {
        (60_000.0 / bpm) * 4.0 / self.time.unit as f32
    }

    /// La durée d'une mesure (ms) au tempo donné.
    pub fn bar_ms(&self, bpm: f32) -> f32 {
        self.ticks() as f32 * self.tick_ms(bpm)
    }

    /// Le nombre de **doubles croches** d'une mesure : 16 en 4/4, 12 en 6/8, 14 en 7/8. C'est
    /// l'unité du Song Position Pointer de MIDI.
    pub fn sixteenths(&self) -> f64 {
        self.time.beats as f64 * 16.0 / self.time.unit as f64
    }
}

/// Le regroupement **par défaut** : les mesures composées en x/8 se battent par trois (6/8, 9/8,
/// 12/8), tout le reste temps par temps. Une mesure impaire sans `{x_namfoop_beats}` se bat donc
/// toutes les croches, ce que le § 11 demande explicitement : deviner `2+2+3` plutôt que `3+2+2`
/// serait se tromper une fois sur deux.
fn default_groups(time: TimeSig) -> Vec<u8> {
    if time.unit == 8 && time.beats >= 6 && time.beats.is_multiple_of(3) {
        vec![3; (time.beats / 3) as usize]
    } else {
        vec![1; time.beats as usize]
    }
}

/// Une **section** de la carte : ce que la bande annonce, et sa durée.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    /// L'étiquette du prompteur (`Couplet 1`). Vide : un morceau sans sections.
    pub label: String,
    /// `{x_namfoop_sound}` : le son du set auquel la section est liée.
    pub sound: Option<String>,
    pub meter: Meter,
    /// Le tempo en vigueur dans la section. `None` : ni le morceau ni le prompteur n'en donnent,
    /// c'est celui du preset qui s'entend.
    pub tempo_bpm: Option<f32>,
    /// Sa durée en mesures. `None` : inconnue — la battue compte les mesures sans fin de section.
    pub bars: Option<u32>,
    /// Le nombre de lignes de texte : le défilement les répartit sur les mesures de la section.
    pub lines: usize,
}

/// Une **position** dans le morceau : la section, la mesure dans cette section, le tick dans cette
/// mesure. Toutes comptées à partir de zéro ; la bande affiche `mesure + 1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    pub section: usize,
    pub bar: u32,
    pub tick: u32,
}

/// La carte des mesures d'un morceau : une [`Span`] par section, dans l'ordre.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarMap {
    spans: Vec<Span>,
}

impl BarMap {
    /// La carte d'un morceau du set, avec son prompteur s'il en a un.
    ///
    /// Le morceau fait foi sur le prompteur pour la mesure et le tempo (`time` et `tempo_bpm` du
    /// `.set.toml`), et le **premier** `{time}` / `{tempo}` du fichier sert quand le set ne donne
    /// rien (§ 10). Les `{time}` / `{tempo}` suivants valent à partir de leur section : c'est
    /// `library::prompt` qui les a déjà résolus, chaque section portant ce qui est **en vigueur**
    /// à son entrée.
    pub fn of(song: &Song, prompt: Option<&Prompt>) -> Self {
        let parse = |t: Option<&str>| t.and_then(|t| t.parse::<TimeSig>().ok());
        let base_time = parse(song.time.as_deref()).or_else(|| parse(prompt.and_then(|p| p.time.as_deref())));
        let base_tempo = song.tempo_bpm.or_else(|| prompt.and_then(|p| p.tempo_bpm));
        let base_meter = Meter::new(base_time.unwrap_or(TimeSig::COMMON), None);
        let Some(prompt) = prompt.filter(|p| !p.sections.is_empty()) else {
            // **un morceau sans prompteur a sa bande quand même** (§ 11) : une section sans nom,
            // de durée inconnue, dont on compte les mesures
            return Self {
                spans: vec![Span {
                    meter: base_meter,
                    tempo_bpm: base_tempo,
                    bars: None,
                    ..Default::default()
                }],
            };
        };
        // **le morceau remplace la valeur initiale du fichier, pas ses changements** (§ 10) :
        // le premier `{tempo}` / `{time}` du `.cho` ne sert que si le `.set.toml` se taît, mais
        // une directive **plus loin** dans le morceau est une vraie modulation, et elle gagne.
        // D'où ces deux drapeaux : dès qu'une section s'écarte de la valeur initiale, le fichier
        // mène jusqu'à la fin — y compris s'il revient ensuite à la valeur de départ.
        let (mut tempo_moved, mut time_moved) = (false, false);
        let mut spans: Vec<Span> = Vec::with_capacity(prompt.sections.len());
        for sec in &prompt.sections {
            tempo_moved |= sec.tempo_bpm != prompt.tempo_bpm;
            time_moved |= parse(sec.time.as_deref()) != parse(prompt.time.as_deref());
            let time = if time_moved { parse(sec.time.as_deref()) } else { base_time };
            let tempo = if tempo_moved { sec.tempo_bpm } else { base_tempo };
            spans.push(Span {
                label: sec.label.clone(),
                sound: sec.sound.clone(),
                meter: Meter::new(time.unwrap_or(TimeSig::COMMON), sec.beats.as_deref()),
                tempo_bpm: tempo,
                bars: sec.bars,
                lines: sec.lines.len(),
            });
        }
        spread_duration(&mut spans, prompt.duration_s);
        Self { spans }
    }

    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    pub fn span(&self, section: usize) -> Option<&Span> {
        self.spans.get(section)
    }

    /// La métrique de cette section, ou celle de la dernière si l'index dépasse : la bande doit
    /// toujours pouvoir se dessiner.
    pub fn meter(&self, section: usize) -> Meter {
        self.spans.get(section).or_else(|| self.spans.last()).map(|s| s.meter.clone()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// La durée du morceau en mesures. `None` dès qu'une section est de durée inconnue.
    pub fn total_bars(&self) -> Option<u32> {
        self.spans.iter().try_fold(0u32, |acc, s| Some(acc + s.bars?))
    }

    /// La **ligne de texte** qu'on chante à cette mesure : les lignes d'une section se partagent
    /// ses mesures (§ 10).
    pub fn line_of(&self, section: usize, bar: u32) -> usize {
        let Some(s) = self.spans.get(section) else { return 0 };
        match (s.lines, s.bars) {
            (0, _) => 0,
            (n, Some(bars)) if bars > 0 => ((bar as u64 * n as u64) / bars as u64) as usize,
            (n, _) => (bar as usize).min(n - 1),
        }
    }

    /// La position d'un nombre de **doubles croches** depuis le début du morceau : c'est la
    /// conversion du Song Position Pointer de MIDI. Au-delà de la dernière mesure connue, `None`.
    pub fn at_sixteenths(&self, sixteenths: u32) -> Option<Pos> {
        let mut left = sixteenths as f64;
        for (i, span) in self.spans.iter().enumerate() {
            let per_bar = span.meter.sixteenths();
            if per_bar <= 0.0 {
                continue;
            }
            let bars = match span.bars {
                Some(b) => b,
                // section de durée inconnue : elle absorbe tout ce qui reste
                None => return Some(self.pos_in(i, left, per_bar)),
            };
            let whole = bars as f64 * per_bar;
            if left < whole {
                return Some(self.pos_in(i, left, per_bar));
            }
            left -= whole;
        }
        None
    }

    fn pos_in(&self, section: usize, left: f64, per_bar: f64) -> Pos {
        let bar = (left / per_bar).floor();
        let in_bar = left - bar * per_bar;
        let per_tick = per_bar / self.spans[section].meter.ticks().max(1) as f64;
        Pos { section, bar: bar as u32, tick: (in_bar / per_tick).floor() as u32 }
    }

    /// L'inverse : le nombre de doubles croches d'une position. `None` si une section avant elle
    /// est de durée inconnue — on ne peut alors pas compter depuis le début.
    pub fn sixteenths_at(&self, pos: Pos) -> Option<f64> {
        let mut acc = 0.0;
        for (i, span) in self.spans.iter().enumerate() {
            let per_bar = span.meter.sixteenths();
            if i == pos.section {
                let per_tick = per_bar / span.meter.ticks().max(1) as f64;
                return Some(acc + pos.bar as f64 * per_bar + pos.tick as f64 * per_tick);
            }
            acc += span.bars? as f64 * per_bar;
        }
        None
    }
}

/// `{duration}` **réparti** quand des sections ne donnent pas leur durée (§ 10) : ce qui reste du
/// morceau après les sections qui la donnent se partage au prorata des **lignes de texte**.
///
/// Sans tempo, la durée ne se convertit pas en mesures : ces sections restent de durée inconnue,
/// et la battue y compte les mesures sans fin de section — ce que le § 10 appelle « pas de
/// défilement automatique ».
fn spread_duration(spans: &mut [Span], duration_s: Option<f32>) {
    let Some(total_s) = duration_s.filter(|d| *d > 0.0) else { return };
    let unknown: Vec<usize> = (0..spans.len()).filter(|i| spans[*i].bars.is_none()).collect();
    if unknown.is_empty() {
        return;
    }
    let known_s: f32 =
        spans.iter().filter_map(|s| Some(s.bars? as f32 * s.meter.bar_ms(s.tempo_bpm?) / 1000.0)).sum();
    let left_s = total_s - known_s;
    if left_s <= 0.0 {
        return;
    }
    let weight = |s: &Span| s.lines.max(1) as f32;
    let total_w: f32 = unknown.iter().map(|i| weight(&spans[*i])).sum();
    for i in unknown {
        let Some(bpm) = spans[i].tempo_bpm else { continue };
        let share_s = left_s * weight(&spans[i]) / total_w;
        let bars = (share_s * 1000.0 / spans[i].meter.bar_ms(bpm)).round() as i64;
        spans[i].bars = Some(bars.clamp(1, u32::MAX as i64) as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::set::Song;

    fn song(time: Option<&str>, tempo: Option<f32>) -> Song {
        Song {
            title: Some("Le Phare".into()),
            time: time.map(str::to_string),
            tempo_bpm: tempo,
            ..Default::default()
        }
    }

    fn map(cho: &str, time: Option<&str>, tempo: Option<f32>) -> BarMap {
        BarMap::of(&song(time, tempo), Some(&Prompt::parse(cho)))
    }

    /// **4/4** : quatre ticks, quatre temps battus, aucune subdivision. Le tick est la noire.
    #[test]
    fn common_time_beats_four_quarters() {
        let m = Meter::new(TimeSig::COMMON, None);
        assert_eq!((m.ticks(), m.beats()), (4, 4));
        assert_eq!(m.starts(), vec![0, 1, 2, 3]);
        assert!((0..4).all(|t| m.is_beat(t)), "aucune subdivision en 4/4");
        assert_eq!(m.beat_of(2), 2);
        // ♩ = 120 : la noire vaut 500 ms, la mesure 2 s
        assert!((m.tick_ms(120.0) - 500.0).abs() < 0.01, "{}", m.tick_ms(120.0));
        assert!((m.bar_ms(120.0) - 2000.0).abs() < 0.01);
        assert_eq!(m.sixteenths(), 16.0);
    }

    /// **6/8** : six croches groupées par trois — deux pulsations, les autres croches en
    /// subdivisions. Le BPM reste à la noire : à ♩ = 82, la croche vaut la moitié de la noire.
    #[test]
    fn six_eight_beats_two_dotted_quarters() {
        let m = Meter::new("6/8".parse().expect("mesure"), None);
        assert_eq!((m.ticks(), m.beats()), (6, 2));
        assert_eq!(m.starts(), vec![0, 3], "deux pulsations");
        assert_eq!(m.groups, vec![3, 3]);
        assert!(m.is_beat(0) && m.is_beat(3));
        assert!(!m.is_beat(1) && !m.is_beat(2), "les autres croches sont des subdivisions");
        assert_eq!(m.beat_of(4), 1);
        // à ♩ = 82 la noire vaut 731,7 ms, donc la croche 365,9 ms
        assert!((m.tick_ms(82.0) - 365.85).abs() < 0.1, "{}", m.tick_ms(82.0));
        assert_eq!(m.sixteenths(), 12.0);
        // 9/8 et 12/8 suivent la même règle ; 3/8 non — trois croches se battent une à une
        assert_eq!(Meter::new("9/8".parse().unwrap(), None).groups, vec![3, 3, 3]);
        assert_eq!(Meter::new("12/8".parse().unwrap(), None).groups, vec![3, 3, 3, 3]);
        assert_eq!(Meter::new("3/8".parse().unwrap(), None).groups, vec![1, 1, 1]);
    }

    /// **7/8 en 2+2+3** : trois temps battus inégaux. Sans `{x_namfoop_beats}`, toutes les croches
    /// se battent — on ne devine pas `2+2+3` plutôt que `3+2+2`.
    #[test]
    fn seven_eight_follows_its_declared_grouping() {
        let m = Meter::new("7/8".parse().expect("mesure"), Some(&[2, 2, 3]));
        assert_eq!((m.ticks(), m.beats()), (7, 3));
        assert_eq!(m.starts(), vec![0, 2, 4]);
        assert_eq!((m.beat_of(0), m.beat_of(3), m.beat_of(6)), (0, 1, 2));
        assert_eq!(m.sixteenths(), 14.0);
        let plain = Meter::new("7/8".parse().unwrap(), None);
        assert_eq!(plain.groups, vec![1; 7], "sans regroupement, toutes les croches");
        // un regroupement **incohérent** avec la mesure est ignoré : la mesure fait foi
        let wrong = Meter::new(TimeSig::COMMON, Some(&[2, 2, 3]));
        assert_eq!(wrong.groups, vec![1, 1, 1, 1], "2+2+3 sous un 4/4 ne fait pas 7 ticks");
    }

    /// La carte d'un morceau : chaque section porte sa mesure, son tempo et sa durée, et le
    /// morceau fait foi sur le prompteur.
    #[test]
    fn the_map_carries_each_section() {
        let m = map(
            "{tempo: 100}\n{time: 4/4}\n\
             {start_of_verse: Couplet 1}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 8}\na\nb\n{end_of_verse}\n\
             {start_of_chorus: Refrain}\n{x_namfoop_sound: fuzz}\n{x_namfoop_bars: 4}\nc\n{end_of_chorus}\n",
            Some("4/4"),
            Some(124.0),
        );
        assert_eq!(m.len(), 2);
        let v = m.span(0).expect("le couplet");
        assert_eq!((v.label.as_str(), v.bars, v.lines), ("Couplet 1", Some(8), 2));
        assert_eq!(v.sound.as_deref(), Some("clean"));
        assert_eq!(v.tempo_bpm, Some(124.0), "le tempo du MORCEAU prime sur le {{tempo}} du fichier");
        assert_eq!(v.meter.time, TimeSig::COMMON);
        assert_eq!(m.span(1).map(|s| s.bars), Some(Some(4)));
        assert_eq!(m.total_bars(), Some(12));
        // le morceau ne donne rien : le premier {tempo} / {time} du fichier sert (§ 10)
        let m = map("{tempo: 100}\n{time: 6/8}\n{sov}\n{x_namfoop_bars: 2}\na\n{eov}\n", None, None);
        assert_eq!(m.span(0).unwrap().tempo_bpm, Some(100.0));
        assert_eq!(m.meter(0).time, "6/8".parse().unwrap());
    }

    /// **Un changement de mesure et de tempo en cours de morceau** : chaque section porte ce qui
    /// est en vigueur à son entrée, et la carte le voit section par section.
    #[test]
    fn a_meter_and_tempo_change_inside_the_song() {
        let m = map(
            "{sov}\n{x_namfoop_bars: 4}\na\n{eov}\n\
             {time: 7/8}\n{x_namfoop_beats: 2+2+3}\n{tempo: 92}\n\
             {start_of_bridge: Pont}\n{x_namfoop_bars: 2}\nb\n{end_of_bridge}\n\
             {time: 4/4}\n{start_of_chorus: Refrain}\n{x_namfoop_bars: 4}\nc\n{end_of_chorus}\n",
            Some("4/4"),
            Some(124.0),
        );
        assert_eq!(m.len(), 3);
        assert_eq!(m.meter(0), Meter::new(TimeSig::COMMON, None));
        assert_eq!(m.meter(1).groups, vec![2, 2, 3], "le pont passe en 7/8 (2+2+3)");
        assert_eq!(m.span(1).unwrap().tempo_bpm, Some(92.0), "et à ♩92");
        assert_eq!(m.meter(2), Meter::new(TimeSig::COMMON, None), "le refrain revient en 4/4");
        assert_eq!(m.span(2).unwrap().tempo_bpm, Some(92.0), "le tempo, lui, reste celui du pont");
        assert_eq!(m.total_bars(), Some(10));
    }

    /// **La conversion depuis des doubles croches** : c'est le Song Position Pointer de MIDI, et
    /// elle traverse un changement de mesure.
    #[test]
    fn a_song_position_pointer_becomes_a_bar_and_a_beat() {
        // 2 mesures de 4/4 (32 doubles croches), puis 2 de 7/8 (14 chacune), puis 1 de 6/8 (12)
        let m = map(
            "{sov}\n{x_namfoop_bars: 2}\na\n{eov}\n\
             {time: 7/8}\n{x_namfoop_beats: 2+2+3}\n{sob}\n{x_namfoop_bars: 2}\nb\n{eob}\n\
             {time: 6/8}\n{soc}\n{x_namfoop_bars: 1}\nc\n{eoc}\n",
            Some("4/4"),
            Some(120.0),
        );
        let at = |n| m.at_sixteenths(n).expect("dans le morceau");
        assert_eq!(at(0), Pos { section: 0, bar: 0, tick: 0 }, "le tout début");
        assert_eq!(at(4), Pos { section: 0, bar: 0, tick: 1 }, "une noire plus loin : le temps 2");
        assert_eq!(at(16), Pos { section: 0, bar: 1, tick: 0 }, "la mesure 2");
        assert_eq!(at(32), Pos { section: 1, bar: 0, tick: 0 }, "le pont, en 7/8");
        assert_eq!(at(34), Pos { section: 1, bar: 0, tick: 1 }, "une croche du 7/8 = 2 doubles");
        assert_eq!(at(46), Pos { section: 1, bar: 1, tick: 0 }, "la 2e mesure du pont (32 + 14)");
        assert_eq!(at(60), Pos { section: 2, bar: 0, tick: 0 }, "le refrain, en 6/8 (32 + 28)");
        assert_eq!(at(63), Pos { section: 2, bar: 0, tick: 1 }, "sa 2e croche, une subdivision");
        assert_eq!(at(66), Pos { section: 2, bar: 0, tick: 3 }, "sa 2e PULSATION (3 croches)");
        assert_eq!(m.at_sixteenths(72), None, "au-delà de la fin : la carte ne promet rien");
        // l'aller-retour, sur chaque frontière
        for n in [0, 16, 32, 46, 60, 66] {
            assert_eq!(m.sixteenths_at(at(n)), Some(n as f64), "aller-retour de {n}");
        }
    }

    /// Un morceau **sans prompteur** a quand même sa carte : une section sans nom, de durée
    /// inconnue, dont on compte les mesures — c'est ce que le § 11 exige pour que la bande
    /// s'affiche toujours.
    #[test]
    fn a_song_without_a_prompt_still_has_a_map() {
        let m = BarMap::of(&song(Some("6/8"), Some(82.0)), None);
        assert_eq!(m.len(), 1);
        let s = m.span(0).expect("la section implicite");
        assert_eq!((s.label.as_str(), s.bars, s.lines), ("", None, 0));
        assert_eq!(s.meter.groups, vec![3, 3]);
        assert_eq!(m.total_bars(), None);
        // de durée inconnue, elle **absorbe** n'importe quelle position (12 doubles par mesure)
        assert_eq!(m.at_sixteenths(0), Some(Pos { section: 0, bar: 0, tick: 0 }));
        assert_eq!(m.at_sixteenths(27), Some(Pos { section: 0, bar: 2, tick: 1 }));
        // un prompteur vide revient au même
        assert_eq!(BarMap::of(&song(None, None), Some(&Prompt::parse("{title: X}\n"))).len(), 1);
    }

    /// **`{duration}` réparti** quand des sections ne donnent pas leur durée (§ 10), au prorata de
    /// leurs lignes de texte, et ce que donnent les sections qui la déclarent est retiré d'abord.
    #[test]
    fn duration_is_spread_over_the_sections_without_bars() {
        // ♩120 en 4/4 : 2 s par mesure. 60 s = 30 mesures, dont 10 déclarées par le couplet.
        let m = map(
            "{duration: 60}\n\
             {start_of_verse: Couplet}\n{x_namfoop_bars: 10}\na\n{end_of_verse}\n\
             {start_of_chorus: Refrain}\nb\nc\nd\n{end_of_chorus}\n\
             {start_of_bridge: Pont}\ne\n{end_of_bridge}\n",
            Some("4/4"),
            Some(120.0),
        );
        assert_eq!(m.span(0).unwrap().bars, Some(10), "déclarée : intacte");
        // 20 mesures restantes partagées 3 lignes / 1 ligne
        assert_eq!(m.span(1).unwrap().bars, Some(15));
        assert_eq!(m.span(2).unwrap().bars, Some(5));
        assert_eq!(m.total_bars(), Some(30));
        // **sans tempo**, la durée ne se convertit pas : pas de défilement automatique (§ 10)
        let m = map("{duration: 60}\n{sov}\na\n{eov}\n", None, None);
        assert_eq!(m.span(0).unwrap().bars, None);
        // sans durée non plus
        let m = map("{sov}\na\n{eov}\n", None, Some(120.0));
        assert_eq!(m.span(0).unwrap().bars, None);
    }

    /// **Les lignes se partagent les mesures de leur section** (§ 10) : c'est ce qui fait défiler
    /// le texte au tempo.
    #[test]
    fn the_lines_share_the_bars_of_their_section() {
        let m = map("{sov}\n{x_namfoop_bars: 8}\na\nb\nc\nd\n{eov}\n", Some("4/4"), Some(120.0));
        let lines: Vec<usize> = (0..8).map(|b| m.line_of(0, b)).collect();
        assert_eq!(lines, vec![0, 0, 1, 1, 2, 2, 3, 3], "4 lignes sur 8 mesures");
        // une section de durée inconnue avance d'une ligne par mesure, puis s'arrête à la dernière
        let m = map("{sov}\na\nb\n{eov}\n", Some("4/4"), None);
        assert_eq!((m.line_of(0, 0), m.line_of(0, 1), m.line_of(0, 9)), (0, 1, 1));
        // une section vide ne demande rien
        assert_eq!(m.line_of(9, 3), 0, "une section qui n'existe pas ne panique pas");
    }
}
