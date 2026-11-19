//! **Le contexte** : qui a le clavier, en une valeur.
//!
//! **Une** fonction dit où l'on est, et la table de `bindings.rs` dit ce qu'on y peut faire. Trois
//! cascades voulaient cette réponse — `on_key` (qui décide), `ui::footer::keys` (qui annonce) et
//! `ui::windows::help_lines` (qui explique) : qu'elles la tirent toutes d'ici est ce qui empêche
//! une touche d'être annoncée sans être traitée, ou traitée sans être annoncée.
//!
//! L'ordre des `if` suit exactement celui des étages de `on_key` (ADR 0009) : c'est la même
//! vérité, écrite une fois.

use super::{fx::FxFocus, space::Space, State, Tab};

/// Où va la prochaine touche. Les variantes sont dans l'**ordre de priorité**, de la plus forte
/// (un mur) à la plus faible (l'espace où l'on travaille).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Context {
    /// `run --set` : la scène, et sa fenêtre de sortie.
    Stage,
    StageQuit,
    /// L'aide `h` : n'importe quelle touche la referme.
    Help,
    /// Les fenêtres de confirmation.
    ConfirmQuit,
    /// La même, quand la chaîne est vide alors que le banc joue un son (§ 9).
    ConfirmQuitEmpty,
    ConfirmSwitch,
    /// L'éditeur d'un modèle (`e`), et sa saisie de texte.
    EditorTyping,
    Editor,
    /// Les saisies de texte de la Bibliothèque.
    Naming,
    Filtering,
    /// Les fenêtres : la banque `P`, « mettre dans la chaîne », le mode pose, le sélecteur sur
    /// place (et son filtre), le catalogue de blocs, le choix du set.
    Bank,
    Send,
    Place,
    Picker,
    PickerFiltering,
    Catalog,
    SetPicker,
    /// Les réglages d'un élément de la chaîne (`Entrée` sur un effet).
    Params,
    /// **L'édition d'un set** (`e`, § 8) : ses saisies et sa liste de presets prennent tout.
    SetEditTyping,
    SetEditPicking,
    SetEditRemove,
    SetEdit,
    /// Les trois espaces. La Bibliothèque est **par onglet** : c'est la seule façon de dire
    /// « `a` / `s` / `x` sont à moi, en Mélange seulement ».
    Set,
    /// La **sortie d'un parallèle** (le `◇`) : ses touches ne sont pas celles d'un bloc.
    ChainMerge,
    Chain,
    Models,
    Irs,
    Blend,
}

impl State {
    /// **Qui a le clavier.** À garder d'accord avec l'ordre des étages de `on_key` : c'est la même
    /// vérité. Un test le vérifie contexte par contexte.
    pub fn context(&self) -> Context {
        self.context_at(true)
    }

    /// Le contexte **sous** l'aide : c'est lui que l'overlay `h` doit expliquer. Sans ça, `h`
    /// montrerait ses propres touches (« n'importe quelle touche la referme »), ce qui est vrai
    /// mais inutile.
    pub fn context_under_help(&self) -> Context {
        self.context_at(false)
    }

    fn context_at(&self, with_help: bool) -> Context {
        if self.locked {
            return if self.confirm_quit { Context::StageQuit } else { Context::Stage };
        }
        if with_help && self.help {
            return Context::Help;
        }
        if self.confirm_quit {
            return if self.empty_chain_warning().is_some() {
                Context::ConfirmQuitEmpty
            } else {
                Context::ConfirmQuit
            };
        }
        if self.confirm_switch {
            return Context::ConfirmSwitch;
        }
        if let Some(e) = self.editor.as_ref() {
            return if e.typing.is_some() { Context::EditorTyping } else { Context::Editor };
        }
        if self.naming.is_some() {
            return Context::Naming;
        }
        if let Some(p) = self.picker.as_ref() {
            return if p.filtering { Context::PickerFiltering } else { Context::Picker };
        }
        if self.fx.catalog.is_some() {
            return Context::Catalog;
        }
        if self.bank.is_some() {
            return Context::Bank;
        }
        if self.send.is_some() {
            return Context::Send;
        }
        if self.place.is_some() {
            return Context::Place;
        }
        if self.set_picker.is_some() {
            return Context::SetPicker;
        }
        if let Some(e) = self.set_edit.as_ref() {
            return if e.typing.is_some() {
                Context::SetEditTyping
            } else if e.picking.is_some() {
                Context::SetEditPicking
            } else if e.confirm_remove {
                Context::SetEditRemove
            } else {
                Context::SetEdit
            };
        }
        if self.filtering {
            return Context::Filtering;
        }
        match self.space {
            Space::Set => Context::Set,
            Space::Chain => {
                if self.fx.focus == FxFocus::Params {
                    Context::Params
                } else if self.fx.chain.spot(self.fx.cursor) == super::chain::Spot::Merge {
                    Context::ChainMerge
                } else {
                    Context::Chain
                }
            }
            Space::Library => match self.tab {
                Tab::Models => Context::Models,
                Tab::Irs => Context::Irs,
                Tab::Blend => Context::Blend,
            },
        }
    }
}

impl Context {
    /// Tous les contextes, pour les tests qui doivent en balayer la liste entière (la jointure
    /// annoncé ↔ possédé, surtout) : en oublier un reviendrait à ne pas le vérifier. Comme
    /// `Binding::keys`, ce n'est lu que par les tests, et c'est là son rôle.
    #[allow(dead_code, reason = "lu par la jointure annoncé ↔ possédé, dans keyboard.rs")]
    pub const ALL: &'static [Context] = &[
        Context::Stage,
        Context::StageQuit,
        Context::Help,
        Context::ConfirmQuit,
        Context::ConfirmQuitEmpty,
        Context::ConfirmSwitch,
        Context::EditorTyping,
        Context::Editor,
        Context::Naming,
        Context::Filtering,
        Context::Bank,
        Context::Send,
        Context::Place,
        Context::Picker,
        Context::PickerFiltering,
        Context::Catalog,
        Context::SetPicker,
        Context::Params,
        Context::SetEditTyping,
        Context::SetEditPicking,
        Context::SetEditRemove,
        Context::SetEdit,
        Context::Set,
        Context::ChainMerge,
        Context::Chain,
        Context::Models,
        Context::Irs,
        Context::Blend,
    ];
}
