//! **La table des touches** : ce que chaque contexte possède, déclaré **une fois**.
//!
//! Quatre endroits ont besoin de cette déclaration — le dispatch (`keys.rs`), la barre du bas
//! (`ui/footer.rs`), l'aide `h` (`ui/windows.rs`) et la liste blanche du verrou. Une touche écrite
//! quatre fois diverge : annoncée sans être traitée (un `PgDn/PgUp son` affiché avec rien
//! derrière), ou traitée sans être annoncée nulle part.
//!
//! Cette table est la **déclaration**. Le `match` de `keys.rs` reste l'**exécuteur**, et deux tests
//! font la **jointure** :
//!
//! - toute touche déclarée ici **agit** dans son contexte (jointure avec l'oracle du clavier) ;
//! - toute touche que la barre du bas annonce **est déclarée** ici.
//!
//! On n'a volontairement **pas** fait porter un identifiant d'action au dispatch pour que le
//! compilateur garantisse la correspondance : la jointure donne la même garantie pour un dixième du
//! câblage, dans le fichier le plus délicat du binaire (ADR 0009).

use super::context::Context;
use crate::tui::Key;

/// Une table par famille d'écrans : un fichier qu'on lit d'un bloc, et une seule édition par
/// touche ajoutée.
mod chain;
mod library;
mod set;
/// Les touches de l'édition d'un set (`e`, § 8) : nombreuses, et groupées par ce qu'elles visent.
pub mod set_edit;
mod stage;
mod windows;

/// Une touche de la table : fixe, ou **réglable** par la config.
///
/// Les réglables le sont parce qu'un footswitch les envoie (`[set] next_song` / `prev_song` pour le
/// morceau, `prompt_next` / `prompt_prev` pour les sections du prompteur), et c'est pour ça
/// qu'elles ne peuvent pas être de simples `Key` `'static`. Quatre variantes **nommées**, et non un
/// système de configuration : le besoin est le pédalier, pas la personnalisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySpec {
    Fixed(Key),
    NextSong,
    PrevSong,
    PromptNext,
    PromptPrev,
}

/// Ce qu'un contexte déclare : des touches, leur étiquette, et ce qu'elles font.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    /// Les touches concernées. **Vide** = ce n'est pas une touche mais une **note** de l'aide
    /// (`division`, `niveau DI`, `◇`, `tape`) : l'aide l'affiche, la jointure l'ignore.
    ///
    /// Ce champ n'est lu **que par les tests**, et c'est voulu : c'est lui qui porte le contrat
    /// « ce qui est annoncé est possédé » (`keyboard.rs`). L'affichage, lui, n'a besoin que de
    /// `label`, `what` et `bar`.
    #[allow(dead_code, reason = "lu par la jointure annoncé ↔ possédé, dans keyboard.rs")]
    pub keys: &'static [KeySpec],
    /// L'étiquette telle qu'on l'écrit à l'écran (`PgDn / PgUp`, `b / r`).
    pub label: &'static str,
    /// L'explication longue, pour l'aide `h`.
    pub what: &'static str,
    /// Le segment de la **barre du bas**, déjà compressé pour tenir sur une ligne
    /// (`↑↓ choisir`). Vide : cette touche n'est pas dans la barre — l'aide `h` la donne.
    ///
    /// Deux rendus, **une** déclaration. La barre garde un texte taillé à la main — il est lu sur
    /// scène, et `help_bar` compte ses segments pour toujours garder les trois derniers —, mais ce
    /// texte ne vit qu'ici.
    pub bar: &'static str,
}

/// **Une ligne par touche.** Les quatre champs sont toujours dans le même ordre — les touches,
/// l'étiquette, l'explication de l'aide `h`, le segment de la barre du bas —, et une déclaration
/// qui tient sur une ligne se lit comme une table.
macro_rules! bind {
    ($keys:expr, $label:expr, $what:expr, $bar:expr) => {
        Binding { keys: $keys, label: $label, what: $what, bar: $bar }
    };
}
pub(super) use bind;

/// Raccourcis d'écriture de la table.
pub(super) const fn k(c: char) -> KeySpec {
    KeySpec::Fixed(Key::Char(c))
}
pub(super) const fn n(key: Key) -> KeySpec {
    KeySpec::Fixed(key)
}

pub(super) const UP_DOWN: &[KeySpec] = &[n(Key::Up), n(Key::Down)];
const ARROWS: &[KeySpec] = &[n(Key::Up), n(Key::Down), n(Key::Left), n(Key::Right)];
pub(super) const ENTER: &[KeySpec] = &[n(Key::Enter)];
pub(super) const ESC: &[KeySpec] = &[n(Key::Esc)];

/// **Ce que ce contexte possède.** L'ordre est celui de l'aide `h` : du geste principal aux
/// sorties, parce que c'est l'ordre dans lequel on en a besoin.
pub fn bindings(ctx: Context) -> &'static [Binding] {
    use Context as C;
    match ctx {
        C::Stage => stage::STAGE,
        C::StageQuit => stage::STAGE_QUIT,
        C::Help => windows::HELP,
        C::ConfirmQuit => windows::CONFIRM_QUIT,
        C::ConfirmQuitEmpty => windows::CONFIRM_QUIT_EMPTY,
        C::ConfirmSwitch => windows::CONFIRM_SWITCH,
        C::Naming => windows::NAMING,
        C::Filtering => windows::FILTERING,
        C::PickerFiltering => windows::PICKER_FILTERING,
        C::Catalog => windows::CATALOG,
        C::EditorTyping => windows::EDITOR_TYPING,
        C::Editor => windows::EDITOR,
        C::Bank => windows::BANK,
        C::Send => windows::SEND,
        C::Place => windows::PLACE,
        C::Picker => windows::PICKER,
        C::SetPicker => windows::SET_PICKER,
        C::SetEdit => set_edit::table(),
        C::SetEditTyping => set_edit::TYPING,
        C::SetEditPicking => set_edit::PICKING,
        C::SetEditRemove => set_edit::REMOVE,
        C::Set => set::SET,
        C::Params => chain::PARAMS,
        C::ChainMerge => chain::CHAIN_MERGE,
        C::Chain => chain::CHAIN,
        C::Models | C::Irs | C::Blend => library::table(ctx),
    }
}

impl KeySpec {
    /// La touche réelle : les réglables sont résolues contre la config lue au démarrage. Comme
    /// `Binding::keys`, lu par la jointure seulement.
    #[allow(dead_code, reason = "lu par la jointure annoncé ↔ possédé, dans keyboard.rs")]
    pub fn resolve(self, song_keys: (Key, Key), prompt_keys: (Key, Key)) -> Key {
        match self {
            KeySpec::Fixed(k) => k,
            KeySpec::NextSong => song_keys.0,
            KeySpec::PrevSong => song_keys.1,
            KeySpec::PromptNext => prompt_keys.0,
            KeySpec::PromptPrev => prompt_keys.1,
        }
    }
}
