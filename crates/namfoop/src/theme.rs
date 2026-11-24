//! Les couleurs des écrans, nommées par ce qu'elles **signifient**.
//!
//! C'est la liste unique : aucune couleur en dur ailleurs dans le binaire, pour qu'un changement de
//! teinte porte partout où la couleur a ce sens, et nulle part ailleurs. Deux usages qui partagent
//! une teinte aujourd'hui gardent deux noms dès qu'ils peuvent diverger demain — le jaune du mode
//! Édition et le jaune d'un vumètre qui chauffe, par exemple.
//!
//! Des **fonctions** et non des constantes : la teinte se choisit à l'exécution, selon ce que le
//! terminal sait afficher, sans retoucher un seul appel. Un terminal qui annonce les couleurs
//! exactes (`COLORTERM` à `truecolor` ou `24bit`) reçoit les teintes de la charte ; les autres —
//! la console du Pi, tmux qui ne transmet pas la variable — reçoivent les couleurs de base, celles
//! de la colonne « Terminal » de la charte. On ne questionne pas le terminal : la réponse est
//! lente, et certains ne répondent pas.
//!
//! Seuls les **accents** changent : le texte courant, le fond d'une fenêtre et la couleur héritée
//! restent ceux du terminal (la charte ne peint pas le fond). `NO_COLOR` est respecté par
//! crossterm, sous ces fonctions.
//!
//! Les noms et les familles viennent de [`docs/identite/charte.md`](../../../../docs/identite/charte.md).

use ratatui::style::{Color, Modifier, Style};

/// Ce que le terminal sait afficher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    /// Les couleurs de base, dans les teintes du thème de l'utilisateur.
    Ansi,
    /// Les teintes exactes de la charte.
    Exact,
}

/// La palette qu'annonce la valeur de `COLORTERM`.
pub fn palette_for(colorterm: Option<&str>) -> Palette {
    match colorterm.map(str::trim) {
        Some(v) if v.eq_ignore_ascii_case("truecolor") || v.eq_ignore_ascii_case("24bit") => Palette::Exact,
        _ => Palette::Ansi,
    }
}

/// La palette du processus, lue une fois dans l'environnement.
#[cfg(not(test))]
pub fn palette() -> Palette {
    static PALETTE: std::sync::OnceLock<Palette> = std::sync::OnceLock::new();
    *PALETTE.get_or_init(|| palette_for(std::env::var("COLORTERM").ok().as_deref()))
}

#[cfg(test)]
thread_local! {
    static FORCED: std::cell::Cell<Palette> = const { std::cell::Cell::new(Palette::Ansi) };
}

/// En test, la palette ne dépend pas du terminal qui lance `cargo test` : couleurs de base, sauf
/// dans [`with_palette`].
#[cfg(test)]
pub fn palette() -> Palette {
    FORCED.with(|f| f.get())
}

/// Exécute `f` avec la palette donnée, sur ce fil seulement (les tests tournent en parallèle).
#[cfg(test)]
pub fn with_palette<R>(p: Palette, f: impl FnOnce() -> R) -> R {
    let was = FORCED.with(|c| c.replace(p));
    let r = f();
    FORCED.with(|c| c.set(was));
    r
}

fn pick(ansi: Color, exact: Color) -> Color {
    match palette() {
        Palette::Ansi => ansi,
        Palette::Exact => exact,
    }
}

// --- La charte (docs/identite/charte.md) ----------------------------------------------------

const MAGENTA: Color = Color::Rgb(0xFF, 0x00, 0x7F);
const CYAN: Color = Color::Rgb(0x00, 0xF0, 0xFF);
const GREEN: Color = Color::Rgb(0x00, 0xE6, 0x76);
const YELLOW: Color = Color::Rgb(0xFF, 0xD6, 0x00);
const RED: Color = Color::Rgb(0xFF, 0x3B, 0x30);
const STEEL: Color = Color::Rgb(0x8A, 0x8A, 0x9C);
const WHITE: Color = Color::Rgb(0xFF, 0xFF, 0xFF);
/// Le violet UV : la basse coupée, et rien d'autre.
const UV: Color = Color::Rgb(0xBF, 0x40, 0xFF);
/// Le noir d'encre, pour le texte posé sur un aplat.
const INK: Color = Color::Rgb(0x0F, 0x0F, 0x14);
/// Les teintes assombries de la rampe : le mélange avec le noir d'encre le plus sombre qui reste
/// lisible à 3:1 au moins (`docs/identite/gradations.html`).
const MAGENTA_DARK: Color = Color::Rgb(0xBC, 0x04, 0x61);
const CYAN_DARK: Color = Color::Rgb(0x07, 0x8B, 0x95);
const GREEN_DARK: Color = Color::Rgb(0x07, 0x85, 0x4A);
/// Le gris acier à mi-chemin du noir d'encre : la part vide d'un vumètre, qui ne doit pas briller
/// autant que le texte secondaire.
const TRACK: Color = Color::Rgb(0x4C, 0x4C, 0x58);

// --- Les espaces et les modes ---------------------------------------------------------------

/// Espace Bibliothèque, et ce qu'on entend au banc d'essai.
pub fn library() -> Color {
    pick(Color::Magenta, MAGENTA)
}

/// Espace Chaîne, et l'écoute A d'une comparaison.
pub fn chain() -> Color {
    pick(Color::Cyan, CYAN)
}

/// Espace Set et vue scène.
pub fn set() -> Color {
    pick(Color::Green, GREEN)
}

/// Mode édition : le champ qu'on règle, la ligne qu'on saisit.
pub fn edit() -> Color {
    pick(Color::Yellow, YELLOW)
}

/// La référence B d'une comparaison A/B.
pub fn reference_b() -> Color {
    pick(Color::Yellow, YELLOW)
}

/// Saturation, écrêtage, IR coupée : ce qui abîme le son.
pub fn clip() -> Color {
    pick(Color::Red, RED)
}

/// **La basse coupée** (l'accordeur) : le bandeau de l'en-tête, le cadre de l'accordeur, la
/// ligne d'écoute. Le rouge reste au danger : une coupure voulue n'en est pas un.
pub fn cut() -> Color {
    pick(Color::LightBlue, UV)
}

// --- L'accordeur ----------------------------------------------------------------------------
// Les trois paliers de la charte, toujours verts quel que soit l'espace : « juste » garde la même
// couleur sous les yeux. Écart loin : blanc ; proche : vert atténué ; juste : vert signal.

/// L'accordeur : écart au-delà de ±10 cents, et les flèches « monte » / « descends ».
pub fn tuner_far() -> Style {
    Style::new().fg(pick(Color::White, WHITE)).add_modifier(Modifier::BOLD)
}

/// L'accordeur : écart de ±3 à ±10 cents.
pub fn tuner_near() -> Style {
    grade(Accent::Set, Step::Low).add_modifier(Modifier::BOLD)
}

/// L'accordeur : juste, à ±3 cents.
pub fn tuner_in() -> Style {
    Style::new().fg(set()).add_modifier(Modifier::BOLD)
}

// --- Le texte -------------------------------------------------------------------------------

/// Texte courant d'une liste ou d'un panneau.
pub fn text() -> Color {
    Color::Gray
}

/// Texte secondaire, élément inactif, aide en bas de panneau.
pub fn dim() -> Color {
    pick(Color::DarkGray, STEEL)
}

/// **L'accent d'un écran neutre** — hors des espaces, comme `namfoop midi` : titres, touches,
/// onglet actif et ligne visée (en inverse). Le blanc pur de la charte, pour que la couleur n'y
/// porte que des états.
pub fn neutral() -> Color {
    pick(Color::White, WHITE)
}

/// Texte posé sur un aplat de couleur (les inversions).
pub fn on_accent() -> Color {
    pick(Color::Black, INK)
}

/// La couleur que le terminal donnerait de lui-même.
pub fn inherit() -> Color {
    Color::Reset
}

// --- Les états ------------------------------------------------------------------------------

/// Ce qui est en place, validé, sain.
pub fn ok() -> Color {
    pick(Color::Green, GREEN)
}

/// Avertissement, attente, ce qui demande un geste.
pub fn warn() -> Color {
    pick(Color::Yellow, YELLOW)
}

/// La touche d'une action sans retour : quitter, abandonner.
pub fn danger() -> Color {
    pick(Color::Red, RED)
}

/// Erreur, refus, ce qui est cassé.
pub fn error() -> Color {
    pick(Color::Red, RED)
}

// --- Les commandes et les fenêtres ------------------------------------------------------------

/// Une touche à presser, le curseur « ▶ », le cadre et le titre d'une fenêtre, l'invite de filtre.
pub fn key() -> Color {
    pick(Color::Cyan, CYAN)
}

/// Le texte qu'on est en train de taper dans un filtre.
pub fn typed() -> Color {
    pick(Color::Green, GREEN)
}

/// Le fond opaque d'une fenêtre modale, qui doit masquer l'écran du dessous.
pub fn popup_bg() -> Color {
    Color::Black
}

// --- Les vumètres ---------------------------------------------------------------------------

/// Niveau qui approche du plafond.
pub fn meter_warn() -> Color {
    pick(Color::Yellow, YELLOW)
}

/// Niveau au plafond.
pub fn meter_clip() -> Color {
    pick(Color::Red, RED)
}

/// La part de la barre que le compresseur réduit, prise sur la droite : neutre, pour que le jaune
/// et le rouge restent aux alarmes.
pub fn meter_action() -> Color {
    pick(Color::White, WHITE)
}

/// Le gate fermé : la barre entière, éteinte.
pub fn meter_gate() -> Color {
    pick(Color::DarkGray, STEEL)
}

/// La part non remplie de la barre, et la barre d'un bloc coupé ou d'une branche muette.
pub fn meter_track() -> Color {
    pick(Color::DarkGray, TRACK)
}

/// Le repère de calibration posé sur la barre.
pub fn meter_mark() -> Color {
    pick(Color::White, WHITE)
}

// --- Les gradations : la rampe de l'espace ----------------------------------------------------

/// La couleur dans laquelle une gradation se décline : celle de l'espace où elle s'affiche, ou le
/// blanc hors des espaces (`setup`, `run`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accent {
    Library,
    Chain,
    Set,
    Neutral,
}

/// Un palier d'une gradation : un son clair ou un écart faible, le milieu, le plus fort.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Low,
    Mid,
    High,
}

/// Le palier d'une gradation (grain, écart de spectre, corps d'un vumètre) : teinte assombrie,
/// teinte pleine, blanc gras (« chauffé à blanc »). Les alarmes de niveau n'en font pas partie :
/// [`meter_warn`] et [`meter_clip`] sortent de l'espace. Les paliers et leurs mesures :
/// `docs/identite/gradations.html`. En couleurs de base, la teinte assombrie est l'attribut
/// atténué du terminal.
pub fn grade(accent: Accent, step: Step) -> Style {
    let (base, full, dark) = match accent {
        Accent::Library => (Color::Magenta, MAGENTA, MAGENTA_DARK),
        Accent::Chain => (Color::Cyan, CYAN, CYAN_DARK),
        Accent::Set => (Color::Green, GREEN, GREEN_DARK),
        Accent::Neutral => (Color::White, WHITE, STEEL),
    };
    let s = Style::new();
    match (palette(), step) {
        (_, Step::High) => s.fg(pick(Color::White, WHITE)).add_modifier(Modifier::BOLD),
        (_, Step::Mid) => s.fg(pick(base, full)),
        (Palette::Exact, Step::Low) => s.fg(dark),
        (Palette::Ansi, Step::Low) if accent == Accent::Neutral => s.fg(Color::DarkGray),
        (Palette::Ansi, Step::Low) => s.fg(base).add_modifier(Modifier::DIM),
    }
}

// --- La chaîne en blocs -----------------------------------------------------------------------

/// Les nœuds d'un parallèle : « ◆ » où il s'ouvre, « ◇ » où les branches se somment.
pub fn split() -> Color {
    pick(Color::Yellow, YELLOW)
}

/// En-tête et gains d'une branche d'un parallèle : un style, parce que le blanc de la charte
/// est en gras.
pub fn branch() -> Style {
    match palette() {
        Palette::Ansi => Style::new().fg(Color::Blue),
        Palette::Exact => Style::new().fg(WHITE).add_modifier(Modifier::BOLD),
    }
}

/// Le bloc qui fait le son (modèle, IR) : blanc gras, comme les en-têtes de branche.
pub fn amp() -> Style {
    Style::new().fg(pick(Color::White, WHITE)).add_modifier(Modifier::BOLD)
}

/// La jauge du réglage sous le curseur.
pub fn gauge_on() -> Color {
    pick(Color::Cyan, CYAN)
}

/// La jauge d'un réglage au repos.
pub fn gauge_off() -> Color {
    pick(Color::Blue, STEEL)
}

/// Le mini-spectre d'une ligne et les courbes de réponse.
pub fn curve() -> Color {
    pick(Color::Cyan, CYAN)
}

/// Une valeur posée à la main : le PC d'un morceau, son tempo, le tempo tapé au pied.
pub fn value() -> Color {
    pick(Color::Cyan, CYAN)
}

/// Une cue du prompteur : ce qui se lit sans se chanter.
pub fn cue() -> Color {
    pick(Color::Yellow, YELLOW)
}

/// Les accords posés au-dessus des paroles.
pub fn chord() -> Color {
    pick(Color::Cyan, CYAN)
}

/// Le son qu'on essaie sans l'avoir adopté.
pub fn trial() -> Color {
    pick(Color::Yellow, YELLOW)
}

/// Un modèle dont le profil a été corrigé à la main.
pub fn corrected() -> Color {
    pick(Color::Magenta, MAGENTA)
}

/// Une IR conseillée pour le modèle en cours.
pub fn recommended() -> Color {
    pick(Color::Yellow, YELLOW)
}

/// Le mélange d'IR : sa part dans une ligne, sa courbe.
pub fn blend() -> Color {
    pick(Color::Magenta, MAGENTA)
}

// --- L'assistant de configuration ---------------------------------------------------------------

/// L'entrée où la basse est branchée.
pub fn input_bass() -> Color {
    pick(Color::Cyan, CYAN)
}

/// L'entrée où un micro est branché.
pub fn input_mic() -> Color {
    pick(Color::Magenta, MAGENTA)
}

/// Une entrée où du signal arrive.
pub fn signal() -> Color {
    pick(Color::Green, GREEN)
}

/// La sortie qu'on écoute.
pub fn monitored() -> Color {
    pick(Color::Cyan, CYAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_terminal_that_announces_exact_colors_gets_them() {
        for v in ["truecolor", "24bit", "TrueColor", " truecolor\n"] {
            assert_eq!(palette_for(Some(v)), Palette::Exact, "{v:?}");
        }
        for v in [None, Some(""), Some("yes"), Some("256color"), Some("1")] {
            assert_eq!(palette_for(v), Palette::Ansi, "{v:?}");
        }
    }

    #[test]
    fn base_colors_are_the_charter_s_terminal_column() {
        with_palette(Palette::Ansi, || {
            assert_eq!(library(), Color::Magenta);
            assert_eq!(chain(), Color::Cyan);
            assert_eq!(set(), Color::Green);
            assert_eq!(edit(), Color::Yellow);
            assert_eq!(clip(), Color::Red);
            assert_eq!(dim(), Color::DarkGray);
            assert_eq!(on_accent(), Color::Black);
            assert_eq!(branch(), Style::new().fg(Color::Blue));
            assert_eq!(gauge_off(), Color::Blue);
        });
    }

    #[test]
    fn exact_colors_are_those_of_the_charter() {
        with_palette(Palette::Exact, || {
            assert_eq!(library(), Color::Rgb(0xFF, 0x00, 0x7F), "magenta électrique");
            assert_eq!(chain(), Color::Rgb(0x00, 0xF0, 0xFF), "cyan néon");
            assert_eq!(set(), Color::Rgb(0x00, 0xE6, 0x76), "vert signal");
            assert_eq!(edit(), Color::Rgb(0xFF, 0xD6, 0x00), "jaune ambre");
            assert_eq!(clip(), Color::Rgb(0xFF, 0x3B, 0x30), "rouge clip");
            assert_eq!(dim(), Color::Rgb(0x8A, 0x8A, 0x9C), "gris acier");
            assert_eq!(branch(), Style::new().fg(Color::Rgb(0xFF, 0xFF, 0xFF)).add_modifier(Modifier::BOLD));
            assert_eq!(gauge_off(), dim(), "la jauge au repos est en gris acier");
        });
    }

    #[test]
    fn the_background_stays_the_terminal_s() {
        for p in [Palette::Ansi, Palette::Exact] {
            with_palette(p, || {
                assert_eq!(text(), Color::Gray);
                assert_eq!(inherit(), Color::Reset);
                assert_eq!(popup_bg(), Color::Black);
            });
        }
    }

    #[test]
    fn the_meter_body_never_looks_like_an_alarm() {
        with_palette(Palette::Exact, || {
            for a in [Accent::Library, Accent::Chain, Accent::Set, Accent::Neutral] {
                let body = grade(a, Step::Mid).fg.unwrap();
                assert!(body != meter_warn() && body != meter_clip(), "{a:?}");
            }
            assert_ne!(meter_action(), meter_clip(), "le compresseur ne se confond pas avec la saturation");
            assert_ne!(meter_gate(), meter_clip(), "le gate fermé n'est pas une alarme");
        });
    }

    fn luminance(c: Color) -> f64 {
        let Color::Rgb(r, g, b) = c else { panic!("{c:?} n'est pas une teinte exacte") };
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    }

    fn ratio(a: Color, b: Color) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn each_step_of_a_ramp_reads_apart_from_the_next() {
        with_palette(Palette::Exact, || {
            for a in [Accent::Library, Accent::Chain, Accent::Set, Accent::Neutral] {
                let [lo, mid, hi] = [Step::Low, Step::Mid, Step::High].map(|s| grade(a, s).fg.unwrap());
                assert!(ratio(lo, INK) >= 3.0, "{a:?} : le palier faible se lit sur le noir");
                assert!(ratio(lo, mid) >= 1.5, "{a:?} faible → moyen : {:.2}", ratio(lo, mid));
                if a != Accent::Neutral {
                    assert!(ratio(mid, hi) >= 1.3, "{a:?} moyen → fort : {:.2}", ratio(mid, hi));
                }
                assert!(grade(a, Step::High).add_modifier.contains(Modifier::BOLD));
            }
        });
    }

    #[test]
    fn base_colors_keep_three_steps() {
        with_palette(Palette::Ansi, || {
            assert_eq!(
                grade(Accent::Library, Step::Low),
                Style::new().fg(Color::Magenta).add_modifier(Modifier::DIM)
            );
            assert_eq!(grade(Accent::Library, Step::Mid), Style::new().fg(Color::Magenta));
            assert_eq!(
                grade(Accent::Chain, Step::High),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD)
            );
            assert_eq!(grade(Accent::Neutral, Step::Low), Style::new().fg(Color::DarkGray));
        });
    }
}
