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
/// morceau, `prompt_next` / `prompt_prev` pour les sections du prompteur, `prompt_play` pour la
/// battue), et c'est pour ça qu'elles ne peuvent pas être de simples `Key` `'static`. Des
/// variantes **nommées**, et non un système de configuration : le besoin est le pédalier, pas la
/// personnalisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySpec {
    Fixed(Key),
    NextSong,
    PrevSong,
    PromptNext,
    PromptPrev,
    /// La battue (`Espace` par défaut) : son appui est le 1.
    PromptPlay,
    /// L'accordeur (`u` par défaut, `[tuner] key`) : coupe la basse, puis rend le son.
    Tuner,
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

// ------------------------------------------------------------------- les étages (ADR 0009)
// Les touches que **tout espace** possède, déclarées **une fois**. Un espace les reçoit dans cet
// ordre, après ses touches à lui : c'est ce qui fait qu'elles sont toujours au même endroit, d'un
// écran à l'autre. Il ne les replace pas ; il peut les **surcharger** (voir `assemble`).

/// **L'étage 3 — les communes.** Éclipsables par déclaration, jamais par omission.
const COMMONS: &[Binding] = &[
    bind!(
        &[k('S'), k('P')],
        "S / P",
        "enregistrer la chaîne en preset · ouvrir la banque",
        "S enregistrer   P banque"
    ),
    bind!(&[k('n')], "n", "nouvelle chaîne vierge (le preset courant est détaché)", "n vierge"),
];

/// **L'étage 1 — la pédale** : les gestes du pied et de l'oreille, qu'aucun espace n'éclipse.
const PEDAL: &[Binding] = &[
    bind!(
        &[k('*')],
        "*",
        "tap tempo : deux frappes suffisent · modifie le preset, comme tout réglage",
        "* tap"
    ),
    bind!(
        &[k('k')],
        "k",
        "clic du métronome, dans les sorties in-ear seulement · éteint au démarrage",
        "k clic"
    ),
    bind!(
        &[KeySpec::Tuner],
        "u",
        "accordeur : coupe la basse (traitée et DI, micro et clic continuent), puis rend le son \
         · tout changement de son ou de morceau le rend aussi ([tuner] key)",
        "u accordeur"
    ),
];

/// **Les sorties**, toujours les trois dernières : l'aide, l'autre espace, quitter. `help_bar`
/// (`ui/mod.rs`) les garde quelle que soit la largeur du terminal — c'est le **défaut**, et le jour
/// où un écran devra en protéger d'autres, il le dira dans sa table plutôt que dans `help_bar`.
const HELP_KEY: Binding = bind!(&[k('h'), k('?')], "h", "cette aide", "h aide");
const QUIT_KEY: Binding =
    bind!(&[k('q')], "q", "quitter — Échap ne quitte jamais, il remonte d'un niveau", "q quitter");

/// La touche qui nomme **les deux autres** espaces : le seul élément des étages qui dépende de
/// l'écran où l'on est, donc le seul qu'on passe à l'assemblage.
const FROM_LIBRARY: Binding =
    bind!(&[k('2'), k('3')], "2 / 3", "la Chaîne (effets, parallèles) · le Set", "2 Chaîne 3 Set");
const FROM_CHAIN: Binding =
    bind!(&[k('1'), k('3')], "1 / 3", "la Bibliothèque (le banc d\'essai) · le Set", "1 Bibliothèque 3 Set");
const FROM_SET: Binding = bind!(&[k('1'), k('2')], "1 / 2", "la Bibliothèque · la Chaîne", "1 2 espaces");

/// **La table d'un espace** : ses touches à lui, puis les étages dans l'ordre canonique.
///
/// Un écran **surcharge** une touche d'étage en la redéclarant dans sa liste `OVERRIDES` : mêmes
/// touches, d'autres mots, et `bar: ""` pour la laisser à l'aide `h` sans encombrer la barre du
/// bas. C'est ainsi que la Bibliothèque dit « le tap appartient à la Chaîne » et que l'espace Set
/// éclipse `n`. La surcharge prend **la place de l'étage** : l'ordre, lui, ne se surcharge pas —
/// c'est tout l'intérêt (`every_override_matches_a_tier` refuse une surcharge qui ne correspond à
/// rien, donc une surcharge qui ne servirait silencieusement à rien).
fn assemble(own: &[Binding], over: &'static [Binding], spaces: Binding) -> Vec<Binding> {
    let pick = move |tier: &Binding| -> Binding {
        over.iter().find(|o| o.keys == tier.keys).copied().unwrap_or(*tier)
    };
    own.iter()
        .copied()
        .chain(COMMONS.iter().map(&pick))
        .chain(PEDAL.iter().map(&pick))
        .chain([pick(&HELP_KEY), spaces, pick(&QUIT_KEY)])
        .collect()
}

/// Les tables des espaces, assemblées **une fois** par exécution (et non à chaque image).
fn space(ctx: Context) -> &'static [Binding] {
    use Context as C;
    static TABLES: std::sync::OnceLock<std::collections::BTreeMap<Context, Vec<Binding>>> =
        std::sync::OnceLock::new();
    TABLES
        .get_or_init(|| {
            let mut m = std::collections::BTreeMap::new();
            for tab in [C::Models, C::Irs, C::Blend] {
                m.insert(tab, assemble(library::own(tab), library::OVERRIDES, FROM_LIBRARY));
            }
            m.insert(C::Chain, assemble(chain::CHAIN, &[], FROM_CHAIN));
            m.insert(C::ChainMerge, assemble(chain::CHAIN_MERGE, &[], FROM_CHAIN));
            m.insert(C::Set, assemble(set::SET, set::OVERRIDES, FROM_SET));
            m
        })
        .get(&ctx)
        .map_or(&[][..], Vec::as_slice)
}

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
        C::Set => space(ctx),
        C::Params => chain::PARAMS,
        C::ChainMerge => space(ctx),
        C::Chain => space(ctx),
        C::Models | C::Irs | C::Blend => space(ctx),
    }
}

impl KeySpec {
    /// La touche réelle : les réglables sont résolues contre la config lue au démarrage. Comme
    /// `Binding::keys`, lu par la jointure seulement.
    #[allow(dead_code, reason = "lu par la jointure annoncé ↔ possédé, dans keyboard.rs")]
    pub fn resolve(
        self,
        song_keys: (Key, Key),
        prompt_keys: (Key, Key),
        play_key: Key,
        tuner_key: Key,
    ) -> Key {
        match self {
            KeySpec::Fixed(k) => k,
            KeySpec::NextSong => song_keys.0,
            KeySpec::PrevSong => song_keys.1,
            KeySpec::PromptNext => prompt_keys.0,
            KeySpec::PromptPrev => prompt_keys.1,
            KeySpec::PromptPlay => play_key,
            KeySpec::Tuner => tuner_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les six contextes d'**espace** : ceux qui reçoivent les étages. Les autres écrans — les
    /// fenêtres, le panneau des réglages, l'édition d'un set, la scène verrouillée — gardent leur
    /// table à la main, parce que ce qu'ils laissent passer est une **liste blanche** et non un
    /// étage (ADR 0009).
    const SPACES: &[Context] =
        &[Context::Models, Context::Irs, Context::Blend, Context::Chain, Context::ChainMerge, Context::Set];

    /// **Tout espace reçoit les étages.** Un contexte d'espace qui ne passerait pas par
    /// `space()` — un `match` édité de travers — perdrait `S P n * k h q` et les touches d'espace,
    /// et rien n'échouerait : la jointure ne compare chaque contexte qu'à lui-même.
    #[test]
    fn every_space_declares_the_tiers() {
        for ctx in SPACES {
            let declared: Vec<&[KeySpec]> = bindings(*ctx).iter().map(|b| b.keys).collect();
            for tier in COMMONS.iter().chain(PEDAL).chain([&HELP_KEY, &QUIT_KEY]) {
                assert!(declared.contains(&tier.keys), "{ctx:?} ne déclare pas l'étage « {} »", tier.label);
            }
            // la touche qui nomme les deux autres espaces : trois chiffres, deux par écran
            let spaces = declared.iter().filter(|k| {
                k.iter().all(|s| matches!(s, KeySpec::Fixed(Key::Char('1' | '2' | '3')))) && k.len() == 2
            });
            assert_eq!(spaces.count(), 1, "{ctx:?} doit nommer les deux autres espaces, une fois");
        }
    }

    /// **Une surcharge qui ne correspond à aucun étage ne sert à rien** — et c'est pire
    /// qu'absente : on croit avoir changé les mots d'une touche, et l'étage continue de parler.
    #[test]
    fn every_override_matches_a_tier() {
        let tiers: Vec<&[KeySpec]> =
            COMMONS.iter().chain(PEDAL).chain([&HELP_KEY, &QUIT_KEY]).map(|b| b.keys).collect();
        for (who, over) in [("Bibliothèque", library::OVERRIDES), ("Set", set::OVERRIDES)] {
            for b in over {
                assert!(
                    tiers.contains(&b.keys),
                    "{who} surcharge « {} », qui n'est pas une touche d'étage",
                    b.label
                );
            }
        }
    }

    /// **Une surcharge remplace, elle n'ajoute pas** : la table d'un espace a donc exactement le
    /// nombre d'entrées de ses touches à lui, plus les étages. Sans ça, une surcharge mal nommée
    /// doublerait la touche dans l'aide (ce que `no_key_is_declared_twice_in_a_context` verrait,
    /// mais après coup, et sans dire pourquoi).
    #[test]
    fn an_override_replaces_and_never_adds() {
        let tiers = COMMONS.len() + PEDAL.len() + 2 + 1; // + l'aide, quitter, et les espaces
        assert_eq!(bindings(Context::Chain).len(), chain::CHAIN.len() + tiers);
        assert_eq!(bindings(Context::Set).len(), set::SET.len() + tiers);
    }
}
