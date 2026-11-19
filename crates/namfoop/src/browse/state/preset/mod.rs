//! Les **presets** dans `browse` : `S` enregistre la chaîne, `P` ouvre la **banque** (la
//! chercher, la réouvrir), `n` repart d'une **chaîne vierge**, `t` déplie le TOML, et
//! l'en-tête dit si quelque chose n'est pas enregistré.
//!
//! La conversion chaîne ⇄ preset est **ici**, pure et testée : la chaîne éditée porte des **index**
//! de la bibliothèque, le fichier porte des **chemins relatifs** et l'identifiant court en filet
//! (`dsp::preset`). L'écriture et la lecture des fichiers, elles, restent dans `browse/mod.rs`.

use std::path::PathBuf;

use dsp::preset::{PresetBlock, PresetBranch, PresetFile, PresetMode, PresetSpec};

use super::chain::{VOLUME_MAX_DB, VOLUME_MIN_DB};

use super::chain::{self, Block, Leaf};
use super::*;

/// Le preset dont la chaîne vient : son nom et son fichier.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetRef {
    pub name: String,
    pub path: PathBuf,
}

/// Ce qui doit remplacer la chaîne en cours : un preset de la banque, ou une **chaîne vierge**
/// (`n`). Les deux passent par la même fenêtre « la chaîne n'est pas enregistrée ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Preset(usize),
    New,
}

/// Un preset de la banque, sa chaîne déjà résolue (la bascule ne lit donc aucun fichier).
#[derive(Clone, Debug)]
pub struct PresetItem {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    /// La chaîne du preset, sons résolus. `None` : un son manque (la raison est dans `broken`).
    pub chain: Option<chain::Chain>,
    pub broken: Option<String>,
}

/// La fenêtre de la banque (`P`).
#[derive(Clone, Debug, Default)]
pub struct Bank {
    pub cursor: usize,
    pub filter: String,
    pub filtering: bool,
}

impl State {
    /// La banque, telle que la bibliothèque la donne (appelé par `browse/mod.rs` au lancement et
    /// après chaque enregistrement) : chaque preset est converti tout de suite, pour que charger
    /// ne coûte plus que la reconstruction de la chaîne.
    pub fn adopt_presets(&mut self, found: Vec<(String, String, PathBuf, PresetSpec)>) {
        self.presets = found
            .into_iter()
            .map(|(id, name, path, spec)| match self.chain_of(&spec) {
                Ok(chain) => PresetItem { id, name, path, chain: Some(chain), broken: None },
                Err(why) => PresetItem { id, name, path, chain: None, broken: Some(why) },
            })
            .collect();
        self.presets.sort_by_key(|p| p.name.to_lowercase());
    }

    /// Les presets visibles dans la banque (filtre par mots, comme les listes).
    pub fn bank_rows(&self) -> Vec<usize> {
        let words: Vec<String> = self
            .bank
            .as_ref()
            .map(|b| b.filter.to_lowercase().split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        (0..self.presets.len())
            .filter(|&i| {
                let hay = format!("{} {}", self.presets[i].id, self.presets[i].name).to_lowercase();
                words.iter().all(|w| hay.contains(w))
            })
            .collect()
    }

    pub fn bank_cursor(&self) -> usize {
        let n = self.bank_rows().len();
        self.bank.as_ref().map_or(0, |b| b.cursor).min(n.saturating_sub(1))
    }

    /// La chaîne en une ligne, pour la banque (`nam·800D › ir·410 › reverb`).
    pub fn preset_line(&self, i: usize) -> String {
        match (&self.presets[i].chain, &self.presets[i].broken) {
            (Some(c), _) => self.chain_line(c),
            (None, Some(why)) => why.clone(),
            _ => String::new(),
        }
    }

    /// La place du preset courant dans la banque (pour `PgDn` / `PgUp` et l'en-tête).
    pub fn preset_index(&self) -> Option<usize> {
        let p = self.preset.as_ref()?;
        self.presets.iter().position(|x| x.path == p.path)
    }

    /// **Preset suivant / précédent** (`PgDn` / `PgUp`) : le geste du pédalier. L'ordre est
    /// celui de la banque, et on **boucle** du dernier au premier. Si la chaîne n'est pas
    /// enregistrée, on demande une fois (`switch_to`).
    pub(super) fn step_preset(&mut self, down: bool) -> Action {
        // ATTENTION : un set actif ne change RIEN ici. Les espaces sont indépendants —
        // choisir un espace, c'est travailler sur ce qu'il représente —, donc la Chaîne parcourt
        // toujours la banque. Le set se joue dans SON espace (`step_set`) et sur scène.
        let n = self.presets.len();
        if n == 0 {
            self.message = Some("aucun preset enregistré : S enregistre la chaîne".into());
            return Action::None;
        }
        let next = match self.preset_index() {
            Some(i) if down => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            // jamais chargé : on entre dans la banque par le bout qui va dans le sens
            None if down => 0,
            None => n - 1,
        };
        if n == 1 && self.preset_index() == Some(next) {
            self.message = Some(format!("un seul preset : « {} »", self.presets[next].name));
            return Action::None;
        }
        self.switch_to(next)
    }

    /// Ce que l'en-tête affiche à droite : le preset courant et s'il est à jour (§ 110).
    pub fn preset_label(&self) -> (String, bool) {
        let saved = self.unsaved().is_empty();
        // le volume du preset, quand il n'est pas au neutre : c'est un réglage qu'on ne voit nulle
        // part ailleurs dans la liste des blocs
        let volume = match self.fx.chain.volume_db {
            0.0 => String::new(),
            v => format!(" · volume {v:+.0} dB"),
        };
        match &self.preset {
            // la place dans la banque : le repère du pédalier
            Some(p) => {
                let place = match (self.preset_index(), self.presets.len()) {
                    (Some(i), n) if n > 1 => format!(" {}/{n}", i + 1),
                    _ => String::new(),
                };
                (format!("preset{place} : {}{volume}", p.name), saved)
            }
            None if saved && volume.is_empty() => (String::new(), true),
            None if saved => (format!("chaîne{volume}"), true),
            None => (format!("chaîne non enregistrée{volume}"), false),
        }
    }

    /// Le nom proposé par `S` : celui du preset courant, sinon ce que la chaîne contient.
    pub fn default_preset_name(&self) -> String {
        if let Some(p) = &self.preset {
            return p.name.clone();
        }
        let first = self.fx.chain.leaves().find_map(|l| match l {
            Leaf::Nam { model: Some(m), .. } => Some(self.models[*m].label.clone()),
            _ => None,
        });
        let n = self.fx.chain.count();
        let blocs = if n > 1 { "blocs" } else { "bloc" };
        match first {
            Some(m) => format!("{m} + {n} {blocs}"),
            None => format!("chaîne de {n} {blocs}"),
        }
    }

    /// `,` / `.` : le **volume du preset**, appliqué après l'alignement de niveau sur le DI — donc
    /// le seul gain du preset que cet alignement n'annule pas. Il s'enregistre avec la
    /// chaîne, et sert à équilibrer les presets entre eux ; le master de la config, lui, reste le
    /// volume de la scène.
    pub(super) fn adjust_volume(&mut self, step: f32) -> Action {
        let now = self.fx.chain.volume_db;
        let next = (now + step).clamp(VOLUME_MIN_DB, VOLUME_MAX_DB);
        if next == now {
            self.message = Some(format!("volume du preset : {now:+.0} dB, c'est la borne"));
            return Action::None;
        }
        self.fx.chain.volume_db = next;
        self.message = Some(if next == 0.0 {
            "volume du preset : 0 dB (le niveau aligné sur le DI)".to_string()
        } else {
            format!("volume du preset {next:+.0} dB — après l'alignement sur le DI, S pour l'enregistrer")
        });
        // aucune chaîne à redemander : le volume est **vivant**, la boucle d'affichage pose la
        // valeur dans la case et la chaîne qui joue la lit au bloc suivant
        Action::None
    }

    /// La chaîne éditée, en preset : chemins relatifs et identifiants de la bibliothèque.
    pub fn to_preset(&self, name: &str) -> PresetSpec {
        PresetSpec {
            name: name.to_string(),
            volume_db: self.fx.chain.volume_db,
            tempo_bpm: self.fx.chain.tempo_bpm,
            blocks: self.fx.chain.blocks.iter().filter_map(|b| self.preset_block(b)).collect(),
            base: PathBuf::new(),
        }
    }

    /// Un bloc en bloc de preset ; `None` pour un bloc encore vide (`nam·(à choisir)`).
    fn preset_block(&self, b: &Block) -> Option<PresetBlock> {
        match b {
            Block::Leaf(l) => self.preset_leaf(l),
            Block::Split(s) => Some(PresetBlock::Split {
                mode: match s.mode {
                    chain::Mode::Copy => PresetMode::Copy,
                    chain::Mode::Bands => PresetMode::Bands,
                },
                freqs: s.freqs.clone(),
                branches: s
                    .branches
                    .iter()
                    .map(|br| PresetBranch {
                        name: br.name.clone(),
                        gain_db: br.gain_db,
                        invert: br.invert,
                        mute: br.mute,
                        // le solo ne s'enregistre jamais (ADR 0006)
                        blocks: br.blocks.iter().filter_map(|l| self.preset_leaf(l)).collect(),
                    })
                    .collect(),
            }),
        }
    }

    fn preset_leaf(&self, l: &Leaf) -> Option<PresetBlock> {
        let file = |it: &Item| PresetFile::new(it.rel.clone(), Some(it.id.clone()));
        match l {
            Leaf::Fx(slot) => Some(PresetBlock::Fx { on: slot.on, fx: slot.spec.clone() }),
            Leaf::Nam { model, on } => {
                model.map(|m| PresetBlock::Nam { on: *on, model: file(&self.models[m]) })
            }
            Leaf::Ir { ir, on } => match ir {
                IrChoice::Item(i) => Some(PresetBlock::Ir { on: *on, ir: file(&self.irs[*i]) }),
                // le mélange en cours : son fichier, une fois enregistré (`s` dans l'onglet Mélange)
                IrChoice::Blend => self.blend_file.as_ref().map(|(rel, id)| PresetBlock::Ir {
                    on: *on,
                    ir: PresetFile::new(rel.clone(), Some(id.clone())),
                }),
                IrChoice::None => None,
            },
        }
    }

    /// Un preset en chaîne éditable : les chemins relatifs redeviennent des index de la
    /// bibliothèque. `Err` nomme le premier son introuvable (le preset reste listé, grisé).
    fn chain_of(&self, spec: &PresetSpec) -> Result<chain::Chain, String> {
        let mut blocks = Vec::new();
        for b in &spec.blocks {
            blocks.push(match b {
                PresetBlock::Split { mode, freqs, branches } => {
                    let mut out = Vec::new();
                    for br in branches {
                        let mut leaves = Vec::new();
                        for l in &br.blocks {
                            leaves.push(self.leaf_of(l)?);
                        }
                        out.push(chain::Branch {
                            name: br.name.clone(),
                            gain_db: br.gain_db,
                            invert: br.invert,
                            mute: br.mute,
                            solo: false,
                            blocks: leaves,
                        });
                    }
                    Block::Split(chain::Split {
                        mode: match mode {
                            PresetMode::Copy => chain::Mode::Copy,
                            PresetMode::Bands => chain::Mode::Bands,
                        },
                        freqs: freqs.clone(),
                        branches: out,
                    })
                }
                l => Block::Leaf(self.leaf_of(l)?),
            });
        }
        Ok(chain::Chain {
            blocks,
            volume_db: spec.volume_db.clamp(VOLUME_MIN_DB, VOLUME_MAX_DB),
            tempo_bpm: dsp::tempo::clamp_bpm(spec.tempo_bpm),
        })
    }

    fn leaf_of(&self, b: &PresetBlock) -> Result<Leaf, String> {
        // le chemin relatif d'abord, l'identifiant court en filet (le fichier a pu bouger)
        let find = |list: &[Item], f: &PresetFile, what: &str| -> Result<usize, String> {
            let rel = f.file.to_string_lossy().replace('\\', "/");
            list.iter()
                .position(|it| it.rel == rel)
                .or_else(|| f.id.as_deref().and_then(|id| list.iter().position(|it| it.id == id)))
                .ok_or_else(|| format!("{what} introuvable : {rel}"))
        };
        match b {
            PresetBlock::Fx { on, fx } => Ok(Leaf::Fx(super::fx::FxSlot { spec: fx.clone(), on: *on })),
            PresetBlock::Nam { on, model } => {
                Ok(Leaf::Nam { model: Some(find(&self.models, model, "modèle")?), on: *on })
            }
            PresetBlock::Ir { on, ir } => {
                Ok(Leaf::Ir { ir: IrChoice::Item(find(&self.irs, ir, "IR")?), on: *on })
            }
            PresetBlock::Split { .. } => Err("un parallèle dans une branche : refusé".into()),
        }
    }

    /// Le TOML du preset tel qu'il serait écrit (`t` le déplie).
    pub fn preset_toml(&self) -> Result<String, String> {
        let name = self.preset.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| self.default_preset_name());
        toml::to_string_pretty(&self.to_preset(&name)).map_err(|e| e.to_string())
    }

    /// Le mélange en cours vient d'être écrit : un preset peut maintenant le citer par son fichier.
    pub fn set_blend_file(&mut self, rel: String, id: String) {
        self.blend_file = Some((rel, id));
    }

    /// Le preset vient d'être écrit : il devient la référence du « non enregistré ».
    pub fn mark_preset_saved(&mut self, name: String, path: PathBuf) {
        self.preset = Some(PresetRef { name, path });
        self.naming_preset = false;
        self.mark_start();
    }

    /// Installe la chaîne d'un preset chargé : elle devient la chaîne éditée **et** la référence.
    pub fn load_preset(&mut self, i: usize) -> Action {
        let Some(item) = self.presets.get(i) else { return Action::None };
        let Some(chain) = item.chain.clone() else {
            self.message = Some(format!(
                "preset « {} » : {}",
                item.name,
                item.broken.clone().unwrap_or_else(|| "illisible".into())
            ));
            return Action::None;
        };
        let (name, path) = (item.name.clone(), item.path.clone());
        // le preset donne son tempo à l'arbitre : il annule un tap, mais pas une horloge
        // verrouillée, qui est plus forte (ADR 0008)
        self.tempo.preset(chain.tempo_bpm);
        self.fx.chain = chain;
        self.fx.cursor = if self.fx.chain.is_empty() { chain::Cursor::Empty } else { chain::Cursor::Top(0) };
        self.fx.focus = super::fx::FxFocus::Chain;
        self.picker = None;
        self.bank = None;
        self.preset = Some(PresetRef { name: name.clone(), path });
        self.mark_start();
        // charger un preset mène à la Chaîne — sauf **depuis la vue scène**, où jouer un son du
        // set ne doit pas quitter l'écran de concert (§ 6)
        if self.space != super::space::Space::Set {
            self.space = super::space::Space::Chain;
        }
        let place = match (
            self.presets.iter().position(|x| x.path == self.preset.as_ref().expect("posé").path),
            self.presets.len(),
        ) {
            (Some(i), n) if n > 1 => format!(" ({}/{n})", i + 1),
            _ => String::new(),
        };
        self.message = Some(format!("preset « {name} » chargé{place}"));
        self.play()
    }

    /// `S` : enregistrer. Avec un nom déjà connu, on écrit ; sinon on le demande.
    pub(super) fn save_preset(&mut self) -> Action {
        if self.fx.chain.is_empty() {
            self.message = Some("chaîne vide : rien à enregistrer".into());
            return Action::None;
        }
        // un mélange en cours doit d'abord être enregistré pour qu'un preset puisse le citer
        if self.fx.chain.leaves().any(|l| matches!(l, Leaf::Ir { ir: IrChoice::Blend, .. }))
            && self.blend_file.is_none()
        {
            self.message = Some("enregistre d'abord le mélange d'IR (s dans l'onglet Mélange)".into());
            return Action::None;
        }
        self.naming_preset = true;
        self.naming = Some(self.default_preset_name());
        Action::None
    }

    /// Touches de la fenêtre de la banque (`P`). `None` : elle est fermée.
    pub(super) fn bank_key(&mut self, key: Key) -> Option<Action> {
        self.bank.as_ref()?;
        let rows = self.bank_rows();
        let cur = self.bank_cursor();
        if self.bank.as_ref().is_some_and(|b| b.filtering) {
            let b = self.bank.as_mut().expect("ouverte");
            match key {
                Key::Enter => b.filtering = false,
                Key::Esc => {
                    b.filtering = false;
                    b.filter.clear();
                    b.cursor = 0;
                }
                Key::Backspace => {
                    b.filter.pop();
                    b.cursor = 0;
                }
                Key::Char(c) => {
                    b.filter.push(c);
                    b.cursor = 0;
                }
                Key::Up | Key::Down => {
                    b.filtering = false;
                    return self.bank_key(key);
                }
                _ => {}
            }
            return Some(Action::None);
        }
        match key {
            Key::Down | Key::PageDown => {
                let b = self.bank.as_mut().expect("ouverte");
                b.cursor = (cur + 1).min(rows.len().saturating_sub(1));
            }
            Key::Up | Key::PageUp => self.bank.as_mut().expect("ouverte").cursor = cur.saturating_sub(1),
            Key::Char('/') => self.bank.as_mut().expect("ouverte").filtering = true,
            Key::Enter => {
                let Some(&i) = rows.get(cur) else {
                    self.message = Some("aucun preset ne correspond".into());
                    return Some(Action::None);
                };
                return Some(self.switch_to(i));
            }
            Key::Esc | Key::Char('P') => {
                self.bank = None;
                self.message = None;
            }
            // `n` : repartir d'une chaîne vierge ; la banque se referme
            Key::Char('n') => return Some(self.new_chain_or_confirm()),
            _ => {}
        }
        Some(Action::None)
    }

    /// Charger un preset, en demandant **une fois** si la chaîne n'est pas enregistrée.
    pub(super) fn switch_to(&mut self, i: usize) -> Action {
        self.replace_chain(Pending::Preset(i))
    }

    /// **`n` : repartir d'une chaîne vierge**, en demandant une fois si la chaîne n'est pas
    /// enregistrée. C'est le seul moyen de bâtir une chaîne neuve sans vider celle d'un preset (et
    /// sans risquer de réenregistrer par-dessus lui).
    pub(super) fn new_chain_or_confirm(&mut self) -> Action {
        if self.fx.chain.is_empty() && self.preset.is_none() {
            self.message = Some("la chaîne est déjà vierge : a ajoute un bloc".into());
            return Action::None;
        }
        self.replace_chain(Pending::New)
    }

    /// Mettre `what` à la place de la chaîne en cours : tout de suite s'il n'y a rien à perdre,
    /// après la fenêtre « la chaîne n'est pas enregistrée » sinon.
    fn replace_chain(&mut self, what: Pending) -> Action {
        // la CHAÎNE seulement : un set édité n'empêche pas de changer de preset
        if self.chain_unsaved().is_empty() {
            return self.apply_pending(what);
        }
        self.pending_switch = Some(what);
        self.confirm_switch = true;
        self.bank = None;
        self.message = None;
        Action::None
    }

    /// Installe ce qui attendait : la chaîne du preset, ou une chaîne vierge.
    pub fn apply_pending(&mut self, what: Pending) -> Action {
        match what {
            Pending::Preset(i) => self.load_preset(i),
            Pending::New => self.new_chain(),
        }
    }

    /// La chaîne devient **vide** et plus aucun preset n'est courant : `S` proposera donc un nom
    /// neuf, et l'en-tête n'affiche plus de preset. La basse passe en son direct le temps qu'on
    /// ajoute le premier bloc.
    fn new_chain(&mut self) -> Action {
        self.fx.chain = chain::Chain::default();
        self.tempo.preset(self.fx.chain.tempo_bpm);
        self.fx.cursor = chain::Cursor::Empty;
        self.fx.focus = super::fx::FxFocus::Chain;
        self.fx.catalog = None;
        self.picker = None;
        self.bank = None;
        self.preset = None;
        self.mark_start();
        self.space = super::space::Space::Chain;
        self.message = Some("nouvelle chaîne vierge : a ajoute un bloc (son direct en attendant)".into());
        self.play()
    }

    /// Touche reçue pendant la fenêtre « la chaîne n'est pas enregistrée » d'une bascule.
    pub(super) fn confirm_switch_key(&mut self, key: Key) -> Action {
        match key {
            // enregistrer d'abord : la bascule attend l'écriture (`mod.rs` la reprend)
            Key::Char('S') => {
                self.confirm_switch = false;
                self.save_preset()
            }
            Key::Enter => {
                self.confirm_switch = false;
                match self.pending_switch.take() {
                    Some(what) => self.apply_pending(what),
                    None => Action::None,
                }
            }
            _ => {
                self.confirm_switch = false;
                self.pending_switch = None;
                self.message = Some("on reste sur cette chaîne".into());
                Action::None
            }
        }
    }
}

#[cfg(test)]
mod tests;
