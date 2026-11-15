//! **Le tempo et les divisions rythmiques** (ADR 0008).
//!
//! Un effet de temps peut exprimer son temps de deux façons : en **millisecondes** (ce qu'il a
//! toujours fait) ou en **division rythmique** — « une croche pointée », qui ne vaut un nombre de
//! millisecondes qu'une fois le tempo connu. La table [`DIVISIONS`] est la seule source de ces
//! valeurs ; son **indice 0 est `libre`**, qui rend la main au temps en millisecondes.
//!
//! Ici, rien que du calcul : la table, la conversion BPM -> millisecondes, et les bornes. Qui
//! *décide* du tempo (le preset, un tap, une horloge MIDI) est l'affaire de l'arbitre d'`engine`,
//! et qui l'écrit est l'affaire de l'ADR 0007 — le contrôle possède la valeur, le temps réel la lit.

/// Tempo le plus lent accepté (BPM). En dessous, une noire dépasse deux secondes.
pub const MIN_BPM: f32 = 30.0;
/// Tempo le plus rapide accepté (BPM).
pub const MAX_BPM: f32 = 300.0;
/// Tempo d'un preset qui n'en porte pas, et de `run` / `render` / `bench` sans `--tempo`.
pub const DEFAULT_BPM: f32 = 120.0;

/// Ramène un tempo dans les bornes ; une valeur non finie (contrôleur qui dérape, case jamais
/// écrite) redevient [`DEFAULT_BPM`] plutôt que de propager un NaN dans le temps réel.
#[inline]
pub fn clamp_bpm(bpm: f32) -> f32 {
    if bpm.is_finite() {
        bpm.clamp(MIN_BPM, MAX_BPM)
    } else {
        DEFAULT_BPM
    }
}

/// Une division rythmique : ce que vaut le temps d'un effet, en fraction de noire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Division {
    /// Étiquette affichée, et acceptée en mini-syntaxe (`delay:division=1/8.`).
    pub label: &'static str,
    /// Nom de la note en français, **article compris** (`une croche pointée`, `un triolet de
    /// croche`) : le genre change, donc l'appelant ne peut pas l'ajouter lui-même.
    /// Les fractions nues se lisent mal : `1/4`
    /// est **une noire**, donc *un temps*, et non « un quart de temps » — le marché nomme les
    /// notes pour cette raison (« Quarter notes », « Dotted Eighth notes » chez Strymon).
    pub name: &'static str,
    /// Ce que la note vaut **en temps**, en français : c'est la phrase qui lève l'ambiguïté.
    pub beats_label: &'static str,
    /// Durée en noires. **0 pour `libre`** : l'effet garde alors son temps en millisecondes.
    pub beats: f32,
}

impl Division {
    /// Durée en millisecondes à ce tempo, ou `None` pour `libre`.
    ///
    /// Le tempo est borné par [`clamp_bpm`] : appelable depuis le temps réel sans crainte.
    #[inline]
    pub fn ms_at(&self, bpm: f32) -> Option<f32> {
        (self.beats > 0.0).then(|| 60_000.0 / clamp_bpm(bpm) * self.beats)
    }
}

/// **La table des divisions.** Neuf valeurs, dans l'ordre des **noms de notes** — qui se trouve
/// être aussi strictement croissant en durée, si bien que `←` / `→` dans `browse` vont toujours du
/// plus court au plus long.
///
/// Pourquoi neuf et pas quinze : les musiciens naviguent par nom de note, et le marché en offre peu
/// (Strymon en vend cinq). `1/32`, `1/16.`, `1/4T`, `1/2T` et `1/2.` sont écartés — rares à la
/// basse, et chaque valeur de plus est un cran de plus à traverser sur scène (ADR 0008). En
/// ajouter une ne coûte qu'une ligne ici.
pub const DIVISIONS: &[Division] = &[
    Division { label: "libre", name: "", beats_label: "", beats: 0.0 },
    Division { label: "1/16", name: "une double-croche", beats_label: "un quart de temps", beats: 0.25 },
    Division {
        label: "1/8T",
        name: "un triolet de croche",
        beats_label: "un tiers de temps",
        beats: 1.0 / 3.0,
    },
    Division { label: "1/8", name: "une croche", beats_label: "un demi-temps", beats: 0.5 },
    Division {
        label: "1/8.", name: "une croche pointée", beats_label: "trois quarts de temps", beats: 0.75
    },
    Division { label: "1/4", name: "une noire", beats_label: "un temps", beats: 1.0 },
    Division { label: "1/4.", name: "une noire pointée", beats_label: "un temps et demi", beats: 1.5 },
    Division { label: "1/2", name: "une blanche", beats_label: "deux temps", beats: 2.0 },
    Division { label: "1/1", name: "une ronde", beats_label: "quatre temps", beats: 4.0 },
];

/// Indice de `libre` : le défaut de tout paramètre `division`.
pub const FREE: f32 = 0.0;

/// Plus grand indice de la table : le `max` d'un `ParamInfo` de division. Une **constante**,
/// parce que les tables `PARAMS` des effets en sont une ; le test
/// `the_table_grows_strictly_after_free` vérifie qu'elle suit [`DIVISIONS`].
pub const MAX_INDEX: f32 = 8.0;

/// La division à cet indice (valeur d'un paramètre). Hors table ou non finie : `libre`.
///
/// Appelable depuis le temps réel : aucune allocation, aucune panique possible.
#[inline]
pub fn at(index: f32) -> Division {
    if !index.is_finite() || index < 0.0 {
        return DIVISIONS[0];
    }
    DIVISIONS.get(index.round() as usize).copied().unwrap_or(DIVISIONS[0])
}

/// L'étiquette de la division à cet indice (`libre`, `1/8.`).
pub fn label_at(index: f32) -> &'static str {
    at(index).label
}

/// L'indice d'une étiquette (`1/8.` -> 4), sans tenir compte de la casse : ce qui permet d'écrire
/// `delay:division=1/8.` en mini-syntaxe au lieu de compter les crans.
pub fn index_of(label: &str) -> Option<f32> {
    let label = label.trim();
    DIVISIONS.iter().position(|d| d.label.eq_ignore_ascii_case(label)).map(|i| i as f32)
}

/// Le tempo d'une chaîne : figé à la construction, ou **vivant** (ADR 0007). Même patron que
/// [`crate::chain::Volume`].
#[derive(Clone, Debug)]
pub enum Tempo {
    /// Figé : `run`, `render`, `bench` et les rendus de référence — reproductible.
    Bpm(f32),
    /// **Vivant** : la case est lue **une fois par bloc** par l'étage qui s'en sert. C'est ce que
    /// pose `browse`, pour qu'un tap s'entende sans rebâtir la chaîne. Une **seule** case, partagée
    /// par toutes les chaînes audibles, queue de preset comprise.
    Live(crate::param::Control),
}

impl Default for Tempo {
    fn default() -> Self {
        Tempo::Bpm(DEFAULT_BPM)
    }
}

impl Tempo {
    /// Le tempo courant, borné ; une lecture atomique quand il est vivant.
    #[inline]
    pub fn bpm(&self) -> f32 {
        clamp_bpm(match self {
            Tempo::Bpm(v) => *v,
            Tempo::Live(c) => c.get(),
        })
    }
}

impl From<f32> for Tempo {
    fn from(bpm: f32) -> Self {
        Tempo::Bpm(bpm)
    }
}

impl From<crate::param::Control> for Tempo {
    fn from(c: crate::param::Control) -> Self {
        Tempo::Live(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La table est dans l'ordre des noms de notes, qui est aussi croissant en durée : c'est ce qui
    /// rend `←` / `→` prévisibles dans `browse` (ADR 0008).
    #[test]
    fn the_table_grows_strictly_after_free() {
        assert_eq!(DIVISIONS[0].label, "libre");
        assert_eq!(DIVISIONS[0].beats, 0.0);
        for w in DIVISIONS[1..].windows(2) {
            assert!(w[1].beats > w[0].beats, "{} puis {}", w[0].label, w[1].label);
        }
        // pas d'étiquette en double, et toutes utilisables en mini-syntaxe
        for d in DIVISIONS {
            assert_eq!(index_of(d.label).map(at), Some(*d), "{}", d.label);
            assert!(!d.label.contains(['=', ',', ' ']), "{} : illisible en mini-syntaxe", d.label);
        }
        // chaque note est nommée et dit ce qu'elle vaut en temps : c'est ce qui lève l'ambiguïté
        // de la fraction nue (`1/4` = une noire = UN temps, pas un quart de temps)
        for d in &DIVISIONS[1..] {
            assert!(!d.name.is_empty() && !d.beats_label.is_empty(), "{}", d.label);
        }
        assert_eq!(at(index_of("1/4").unwrap()).beats_label, "un temps");
        assert_eq!(at(index_of("1/16").unwrap()).beats_label, "un quart de temps");
    }

    /// Les durées du marché, à 120 BPM : la noire vaut 500 ms et la croche pointée 375.
    #[test]
    fn durations_match_the_usual_table() {
        let ms = |label: &str| at(index_of(label).expect(label)).ms_at(120.0);
        assert_eq!(ms("libre"), None, "libre ne vaut aucune durée : le temps en ms agit");
        assert_eq!(ms("1/4"), Some(500.0));
        assert_eq!(ms("1/8"), Some(250.0));
        assert_eq!(ms("1/8."), Some(375.0));
        assert_eq!(ms("1/16"), Some(125.0));
        assert_eq!(ms("1/2"), Some(1000.0));
        assert_eq!(ms("1/1"), Some(2000.0));
        assert!((ms("1/8T").unwrap() - 500.0 / 3.0).abs() < 1e-3, "triolet de croche");
        // le tempo double, la durée est de moitié
        assert_eq!(at(index_of("1/4").unwrap()).ms_at(60.0), Some(1000.0));
    }

    /// Un indice ou un tempo qui dérape ne fait jamais paniquer le temps réel.
    #[test]
    fn nothing_out_of_range_can_panic() {
        for bad in [-1.0, 9.0, 1e9, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(at(bad), DIVISIONS[0], "indice {bad}");
            assert_eq!(label_at(bad), "libre");
        }
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(clamp_bpm(bad), DEFAULT_BPM, "{bad}");
        }
        assert_eq!(clamp_bpm(1.0), MIN_BPM);
        assert_eq!(clamp_bpm(10_000.0), MAX_BPM);
        assert_eq!(index_of("1/3"), None, "une étiquette inconnue n'est pas une division");
        // une division non finie reste bornée
        assert!(at(5.0).ms_at(f32::NAN).unwrap().is_finite());
        assert_eq!(DIVISIONS.len(), MAX_INDEX as usize + 1, "MAX_INDEX ne suit plus la table");
    }

    /// Le tempo vivant se lit comme le volume vivant (ADR 0007), et borné.
    #[test]
    fn a_live_tempo_follows_its_control() {
        assert_eq!(Tempo::default().bpm(), DEFAULT_BPM);
        let c = crate::param::Control::new(96.0);
        let t = Tempo::from(c.clone());
        assert_eq!(t.bpm(), 96.0);
        c.set(104.0);
        assert_eq!(t.bpm(), 104.0, "l'étage voit l'écriture du contrôle");
        c.set(f32::NAN);
        assert_eq!(t.bpm(), DEFAULT_BPM, "une case qui dérape ne propage pas de NaN");
        c.set(1.0);
        assert_eq!(t.bpm(), MIN_BPM);
        assert_eq!(Tempo::from(1000.0).bpm(), MAX_BPM);
    }
}
