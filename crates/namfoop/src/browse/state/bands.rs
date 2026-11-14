//! Le panneau **Bandes** du banc d'essai (#183) : ce que la chaîne écoutée fait aux sept zones
//! nommées de la bibliothèque (`library::profile::ZONES`).
//!
//! Rien n'est mesuré ici. Tout vient des profils déjà calculés avec la basse de l'utilisateur et
//! gardés en cache : `Item::voicing` est la courbe du **modèle nu** (sa sortie divisée par la
//! basse) et `Item::profile` celle d'une IR. `library::model_profile::with_ir` les compose, et
//! `library::profile::zones` en tire les sept niveaux. Les dB sont **déjà relatifs à la moyenne
//! 80 Hz – 5 kHz** (`dsp::profile::IrProfile`), donc le zéro de l'égaliseur est donné : il n'y a
//! aucune normalisation à faire.
//!
//! L'affichage est donc **figé** : il change au déplacement du curseur, sans qu'une seule chaîne
//! soit reconstruite et sans rien coûter au thread audio.

use dsp::profile::IrProfile;

use super::{IrChoice, State};

/// Combien de zones : les sept de `library::profile::ZONES`.
pub const N: usize = 7;

/// À partir de combien de colonnes l'écran est « large » : le panneau Spectre s'ouvre à droite de
/// la liste (128 : la police 10×20 de l'écran du Pi, #138) et le panneau Bandes s'y glisse sans
/// coûter une seule ligne de liste — il y est donc affiché d'office. En dessous, il faudrait
/// prendre des lignes à la liste : il attend `z`.
pub const WIDE_COLUMNS: u16 = 128;

/// Ce que les colonnes doivent montrer.
#[derive(Clone, Debug, PartialEq)]
pub struct BandView {
    /// Les sept zones en dB, dans l'ordre de `library::profile::ZONES`.
    pub db: [f32; N],
    /// Ce que les colonnes comparent, écrit dans le titre du panneau.
    pub title: String,
    /// `true` : un écart entre deux chaînes (A/B) ; `false` : la couleur de la chaîne écoutée.
    pub diff: bool,
}

/// Nom court pour le titre : les étiquettes de modèles sont longues.
fn short(label: &str) -> String {
    let n = label.chars().count();
    if n <= 18 {
        return label.to_string();
    }
    format!("{}…", label.chars().take(17).collect::<String>().trim_end())
}

/// Les deux noms d'un A/B, débarrassés de leur **début commun** : deux réglages du même ampli
/// (« … - SEND - REGGAE » et « … - SEND - FAT CLASSIC ROCK ») doivent se distinguer, et pas finir
/// tous les deux en « MESA BASS 800D -… ». Le début n'est retiré qu'au dernier espace commun, et
/// seulement s'il reste de quoi lire les deux.
fn pair(a: &str, b: &str) -> (String, String) {
    let common: String = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).map(|(x, _)| x).collect();
    let cut = common.rfind(' ').map_or(0, |i| i + 1);
    let (ta, tb) = (&a[cut..], &b[cut..]);
    if cut == 0 || ta.chars().count() < 3 || tb.chars().count() < 3 {
        return (short(a), short(b));
    }
    (format!("…{}", short(ta)), format!("…{}", short(tb)))
}

impl State {
    /// Le panneau Bandes est-il affiché ? `z` tranche ; sinon c'est l'écran qui décide.
    pub fn show_bands(&self) -> bool {
        self.bands.unwrap_or(self.width >= WIDE_COLUMNS)
    }

    /// `z` : montrer ou masquer le panneau (depuis la Bibliothèque seulement).
    pub(super) fn toggle_bands(&mut self) {
        let on = !self.show_bands();
        self.bands = Some(on);
        self.message = Some(
            if on {
                "panneau Bandes affiché (z pour le masquer)"
            } else {
                "panneau Bandes masqué (z pour le revoir)"
            }
            .into(),
        );
    }

    /// La courbe de la chaîne du banc (`basse › modèle › IR`) pour un côté de l'A/B : le modèle
    /// **nu** plus l'IR donnée. `None` s'il n'y a pas de modèle, ou pas de profil mesuré pour lui.
    fn bench_curve(&self, model: Option<usize>, ir: IrChoice) -> Option<IrProfile> {
        let voicing = self.models.get(model?)?.voicing.clone()?;
        let ir = match ir {
            IrChoice::None => None,
            IrChoice::Item(i) => self.irs.get(i)?.profile.as_ref().map(|d| d.profile.clone()),
            IrChoice::Blend => self.blend_profile.clone(),
        };
        Some(match ir {
            Some(ir) => library::model_profile::with_ir(&voicing, &ir),
            None => (*voicing).clone(),
        })
    }

    /// Le nom du côté `listen_ref` de l'A/B : celui du modèle quand c'est lui qui est épinglé,
    /// celui de l'IR sinon (c'est l'élément qui diffère entre les deux côtés).
    fn side_name(&self, listen_ref: bool) -> String {
        if self.ref_model.is_some() {
            return self
                .model_of(listen_ref)
                .and_then(|m| self.models.get(m))
                .map_or_else(|| "son direct".into(), |it| it.label.clone());
        }
        match self.ir_of(listen_ref) {
            IrChoice::Item(i) => self.irs.get(i).map_or_else(|| "?".into(), |it| it.label.clone()),
            IrChoice::Blend => "mélange".into(),
            IrChoice::None => "sans IR".into(),
        }
    }

    /// Les sept zones à afficher. `None` : rien de mesuré à montrer (son direct, ou modèle dont le
    /// profil manque) — le panneau le dit lui-même, il ne disparaît pas.
    pub fn bands(&self) -> Option<BandView> {
        let heard = self.bench_curve(self.effective_model(), self.effective_ir())?;
        let seven = |c: &IrProfile| {
            let z = library::profile::zones(c);
            let mut out = [0.0; N];
            out.copy_from_slice(&z[..N]);
            out
        };
        let other = self
            .has_reference()
            .then(|| self.bench_curve(self.model_of(!self.listen_ref), self.ir_of(!self.listen_ref)))
            .flatten();
        let Some(other) = other else {
            return Some(BandView {
                db: seven(&heard),
                title: "ce que la chaîne fait à ta basse".into(),
                diff: false,
            });
        };
        let (a, b) = (seven(&heard), seven(&other));
        let mut db = [0.0; N];
        for k in 0..N {
            db[k] = a[k] - b[k];
        }
        // le `◆` suit la référence, qu'on l'écoute (B) ou non (A)
        let (now, then) = pair(&self.side_name(self.listen_ref), &self.side_name(!self.listen_ref));
        let title =
            if self.listen_ref { format!("◆ {now} vs {then}") } else { format!("{now} vs ◆ {then}") };
        Some(BandView { db, title, diff: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::state::tests::{sample, with_model_profiles};
    use crate::browse::state::{space::Space, Key};

    /// Sans référence épinglée, les colonnes montrent la couleur de la chaîne écoutée, IR comprise.
    #[test]
    fn the_columns_show_what_the_heard_chain_does_to_the_bass() {
        let mut s = sample();
        with_model_profiles(&mut s);
        s.model = Some(1); // un modèle qui a besoin d'une IR
        s.ir = IrChoice::Item(2);
        let v = s.bands().expect("une vue");
        assert!(!v.diff, "pas de référence : pas un écart");
        assert_eq!(v.title, "ce que la chaîne fait à ta basse");
        // l'IR entre bien dans le calcul : la couper change les colonnes
        s.bypass = true;
        let sans = s.bands().expect("une vue");
        assert_ne!(sans.db, v.db, "l'IR écoutée doit compter");
    }

    /// Un modèle épinglé (`c`) fait passer les colonnes en écart, et `r` retourne la comparaison.
    #[test]
    fn a_pinned_reference_turns_the_columns_into_a_difference() {
        let mut s = sample();
        with_model_profiles(&mut s);
        s.model = Some(1);
        s.ir = IrChoice::Item(2);
        s.ref_model = Some(0);
        let v = s.bands().expect("une vue");
        assert!(v.diff, "référence épinglée : un écart");
        assert!(v.title.contains("◆") && v.title.contains("vs"), "{}", v.title);
        assert!(!v.title.contains("◆ 800D") || v.title.starts_with("800D"), "{}", v.title);
        // les deux côtés se distinguent : le début commun des noms est retiré
        let (a, b) = pair("MESA 800D - SEND - REGGAE", "MESA 800D - SEND - FAT CLASSIC ROCK");
        assert_eq!(a, "…REGGAE");
        assert_eq!(b, "…FAT CLASSIC ROCK");
        assert_eq!(pair("B7K", "Angel"), ("B7K".into(), "Angel".into()), "rien de commun : tels quels");
        // `r` : on écoute la référence, l'écart change de signe
        s.listen_ref = true;
        let b = s.bands().expect("une vue");
        assert!(b.title.starts_with("◆"), "{}", b.title);
        for k in 0..N {
            assert!((b.db[k] + v.db[k]).abs() < 1e-3, "zone {k} : {} vs {}", b.db[k], v.db[k]);
        }
    }

    /// Sans modèle (son direct), il n'y a rien à montrer.
    #[test]
    fn the_direct_sound_has_nothing_to_show() {
        let mut s = sample();
        with_model_profiles(&mut s);
        s.model = None;
        assert_eq!(s.bands(), None);
    }

    /// `z` montre et masque le panneau, et **seulement** depuis la Bibliothèque : une touche non
    /// gérée par l'espace Chaîne retombe dans la file de la Bibliothèque (`fx_key` rend `None`).
    #[test]
    fn z_toggles_the_panel_from_the_library_only() {
        let mut s = sample();
        // écran étroit : masqué par défaut, il coûterait des lignes de liste
        s.width = WIDE_COLUMNS - 1;
        assert!(!s.show_bands());
        s.on_key(Key::Char('z'));
        assert!(s.show_bands(), "z depuis la Bibliothèque");
        s.on_key(Key::Char('z'));
        assert!(!s.show_bands(), "z referme");
        s.space = Space::Chain;
        s.on_key(Key::Char('z'));
        assert!(!s.show_bands(), "z ne fait rien depuis la Chaîne");
        // écran large : affiché d'office, puisqu'il ne prend la place de personne
        let mut s = sample();
        s.width = WIDE_COLUMNS;
        assert!(s.show_bands());
    }
}
