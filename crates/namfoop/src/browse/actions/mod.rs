//! **L'exécution des actions** : tout ce que `browse::state` demande et ne peut pas faire
//! lui-même, parce qu'il est pur — écrire un fichier, rendre la main au terminal, demander une
//! chaîne au thread de construction.
//!
//! La **frontière** est ici : `state` décide (et c'est testé sans terminal ni carte), ces modules
//! exécutent, et la boucle de `browse/mod.rs` ne garde que l'aiguillage. Une famille par fichier :
//!
//! - [`sound`]  ce qu'on entend : la chaîne demandée, le grain d'un modèle ;
//! - [`files`]  ce qui s'écrit dans la bibliothèque : correction d'un modèle, preset, mélange ;
//! - [`set`]    le set de concert : son fichier, ses prompteurs, le set du moment.

pub(super) mod files;
pub(super) mod set;
pub(super) mod sound;

use std::path::Path;
use std::sync::mpsc::Sender;

use anyhow::Result;

use super::build::Job;
use super::plan::Request;
use super::state::{Action, BlendPart, State};
use super::ui;
use crate::config::LoadedConfig;
use crate::tui;

/// Ce que la boucle fait de son tour après l'action.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Flow {
    /// Finir le tour : poser les cases vivantes, relever les vumètres, dessiner.
    Go,
    /// Passer au tour suivant — l'action n'avait rien à faire (un set absent, un prompteur sans
    /// fichier) et rien n'a changé à l'écran.
    Next,
    /// Quitter `browse`.
    Quit,
}

/// Ce qu'une action a le droit de toucher. C'est **la liste des effets de bord** de `browse` : en
/// voir un de plus ici est le signe qu'une décision est en train de descendre dans l'exécution.
pub(super) struct Ctx<'a> {
    pub st: &'a mut State,
    pub library: &'a Path,
    pub conf: &'a LoadedConfig,
    pub profiles: &'a library::profile::Profiles,
    /// Le jeu de l'instrument joué, qui mesure le grain des modèles.
    pub signal: &'a library::model_profile::Signal,
    /// Le mélange d'IR **écouté**, écrit dans un fichier temporaire avant d'être cité par un bloc.
    pub audition: &'a Path,
    /// Les chaînes que la banque doit préparer : refaites dès que la banque change.
    pub bank_reqs: &'a mut Vec<Option<Request>>,
    pub tx_req: &'a Sender<Job>,
    pub tx_grain: &'a Sender<(usize, f32, std::path::PathBuf)>,
    /// La dernière chaîne demandée : une demande identique ne rebâtit rien.
    pub last_req: &'a mut Option<Request>,
    pub status: &'a mut ui::Status,
    /// L'action à rejouer au tour suivant, sans attendre une touche.
    pub first: &'a mut Action,
    /// Le mélange sauvé pendant la session (id, parties, alignement) : la ligne de commande finale
    /// ne peut le citer que s'il est encore celui qu'on écoute.
    pub saved: &'a mut Option<(String, Vec<BlendPart>, bool)>,
    /// Le terminal, pour la seule action qui le rend à un autre programme (`$EDITOR`).
    pub session: &'a mut tui::Session,
}

impl Ctx<'_> {
    /// La banque du moment est à refaire : le set ou les presets ont changé.
    pub(super) fn rebuild_bank(&mut self) {
        *self.bank_reqs = super::build::bank_requests(self.st, self.audition);
        let _ = self.tx_req.send(Job::Bank(self.bank_reqs.clone()));
        self.status.bank = Some((0, self.bank_reqs.len()));
    }
}

/// **Qui exécute quoi.** Le `match` est exhaustif : une action nouvelle ne compile pas sans dire
/// qui la prend, et son corps vit dans la famille à laquelle elle appartient.
pub(super) fn run(ctx: &mut Ctx, action: Action) -> Result<Flow> {
    match action {
        Action::None => Ok(Flow::Go),
        Action::Quit => Ok(Flow::Quit),
        Action::SaveSet(stem) => set::save(ctx, &stem),
        Action::EditPrompt(song) => set::edit_prompt(ctx, song),
        Action::ListSets => set::list(ctx),
        Action::OpenSet(id) => set::open(ctx, &id),
        Action::SetChanged => set::changed(ctx),
        Action::Play { .. } => sound::play(ctx),
        Action::MeasureGrain { model, input_db } => sound::measure_grain(ctx, model, input_db),
        Action::SaveMeta(edits) => files::save_meta(ctx, &edits),
        Action::SavePreset(name) => files::save_preset(ctx, &name),
        Action::SaveBlend(name) => files::save_blend(ctx, &name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Une bibliothèque **jetable** — jamais celle de l'utilisateur (`CLAUDE.md` racine). Deux
    /// presets dont **un illisible**, deux sets dont **un illisible** : c'est la bibliothèque
    /// qu'on a vraiment, pas celle qu'on voudrait.
    fn disposable(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("namfoop-actions-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for (rel, body) in [
            ("nam/Ampeg SVT/SVT Ultra Lo SM57.nam", "{}"),
            (
                "presets/Chill verse.preset.toml",
                "name = \"Chill verse\"\n[[block]]\nkind = \"fx\"\n[block.fx]\ntype = \"reverb\"\n",
            ),
            ("presets/Cassé.preset.toml", "name = \"Cassé\"\n[[block]]\nkind = \"inconnu\"\n"),
            (
                "sets/Concert.set.toml",
                "name = \"Concert\"\n[[song]]\ntitle = \"A\"\n[[song.sound]]\npreset = \"Chill verse\"\n",
            ),
            ("sets/Cassé.set.toml", "name = \"Cassé\"\n[[song]]\npc = 999\n"),
        ] {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().expect("dossier")).expect("création");
            std::fs::write(p, body).expect("écriture");
        }
        d
    }

    /// **Un preset illisible ne cache pas les autres.** La banque se charge au démarrage et après
    /// chaque enregistrement : un fichier cassé y est signalé, pas fatal — on ne refuse pas
    /// d'ouvrir `browse` avant un concert parce qu'un preset a été édité à la main de travers.
    #[test]
    fn a_broken_preset_does_not_hide_the_others() {
        let d = disposable("presets");
        let got = files::scan_presets(&d);
        let names: Vec<&str> = got.iter().map(|(_, stem, ..)| stem.as_str()).collect();
        assert_eq!(names, ["Chill verse"], "le preset cassé est ignoré, pas l'autre");
        assert!(!got[0].0.is_empty(), "le preset a un identifiant court");
    }

    /// **Un set illisible reste dans la liste `L`, marqué.** L'écran doit pouvoir le dire : le
    /// cacher laisserait croire que le fichier n'existe pas.
    #[test]
    fn a_broken_set_stays_in_the_list_marked() {
        let d = disposable("sets");
        let got = set::list_sets(&d);
        let names: Vec<String> = got
            .iter()
            .map(|e| match e {
                crate::browse::state::set::screen::Entry::Set { name, .. } => name.clone(),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(got.len(), 2, "{names:?}");
        assert!(names.iter().any(|n| n == "Concert"), "{names:?}");
        assert!(names.iter().any(|n| n.starts_with('⚠')), "le set cassé est marqué : {names:?}");
    }

    /// **Un prompteur absent n'est pas une erreur** (§ 10) : il est retenu comme tel, le cadre le
    /// dit, et le concert continue.
    #[test]
    fn a_missing_prompt_is_kept_as_unreadable() {
        let got = set::read_prompt(std::path::Path::new("/n-existe-pas/rien.cho"));
        assert_eq!(got.len(), 1);
        assert!(
            matches!(got[0].1, crate::browse::state::set::prompt::Loaded::Failed(_)),
            "un fichier absent se garde en « illisible », pas en panique"
        );
    }

    /// **Sans config (`--no-config`), rien n'est écrit.** C'est la garantie des tests et des
    /// comparaisons reproductibles : `browse` ne doit rien laisser derrière lui.
    #[test]
    fn no_config_remembers_nothing() {
        let conf = crate::config::LoadedConfig::load(None, true).expect("config ignorée");
        let before = conf.path.exists();
        set::remember(&conf, Some("s1"));
        assert_eq!(conf.path.exists(), before, "le fichier de config n'a pas bougé");
    }
}
