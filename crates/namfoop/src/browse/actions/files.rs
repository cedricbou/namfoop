//! **Ce qui s'écrit dans la bibliothèque** : la correction d'un modèle
//! (`library/overrides.toml`), un preset (`library/presets/`), un mélange d'IR (`library/cabs/`).
//!
//! Trois écritures, et chacune a la même forme : écrire, le **dire** à l'écran, puis remettre
//! l'index et la banque d'accord avec le disque. Une écriture qui échoue laisse l'état tel quel et
//! le message l'explique — on ne quitte jamais sur un enregistrement raté.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;

use super::{Ctx, Flow};
use crate::browse::state::{BlendPart, ModelMeta, State};
use library::overrides::{ModelOverride, Overrides};
use library::{content_hash, Kind, Library};

/// **`a` dans l'éditeur `e`** : les corrections de modèles, puis l'index et les profils relus.
pub(super) fn save_meta(ctx: &mut Ctx, edits: &[(usize, ModelMeta)]) -> Result<Flow> {
    if let Err(e) = write_meta(ctx.library, ctx.st, edits) {
        ctx.st.message = Some(format!("enregistrement impossible : {e:#}"));
        return Ok(Flow::Go);
    }
    let lib = Library::scan(ctx.library);
    let by_rel: HashMap<String, usize> =
        ctx.st.irs.iter().enumerate().map(|(i, x)| (x.rel.clone(), i)).collect();
    for (i, _) in edits {
        if let Some(e) = lib.by_path(&ctx.st.models[*i].path) {
            let profile = ctx.st.models[*i].profile.take();
            ctx.st.models[*i] = crate::browse::model_item(&lib, e, ctx.conf.default_ir.as_deref(), &by_rel);
            ctx.st.models[*i].profile = profile;
        }
    }
    // l'entrée ou l'IR ont pu changer : profils remesurés (le reste vient du cache)
    let mp = library::model_profile::ModelProfiles::load(&lib, ctx.conf, ctx.profiles, |_| {});
    for m in ctx.st.models.iter_mut() {
        m.profile = mp.by_rel.get(&m.rel).cloned();
    }
    // la chaîne relit la correction enregistrée
    *ctx.last_req = None;
    if ctx.st.model.is_some_and(|m| edits.iter().any(|(i, _)| *i == m)) {
        *ctx.first = crate::browse::state::Action::Play { model: ctx.st.model, ir: ctx.st.ir };
    }
    Ok(Flow::Go)
}

/// **`S`** : le preset, puis la banque qui le contient, et ce qui attendait l'écriture — quitter
/// (`q` › `S`) ou basculer vers une autre chaîne.
pub(super) fn save_preset(ctx: &mut Ctx, name: &str) -> Result<Flow> {
    let lib = Library::scan(ctx.library);
    let file = lib.preset_path(name);
    match ctx.st.to_preset(name).save(&file) {
        Ok(()) => {
            ctx.st.message = Some(format!("preset « {name} » enregistré ({})", file.display()));
            ctx.st.mark_preset_saved(name.to_string(), file);
            // la banque contient le nouveau preset : à précharger de nouveau
            let presets = scan_presets(ctx.library);
            ctx.st.adopt_presets(presets);
            ctx.rebuild_bank();
            // `S` depuis la fenêtre de `q` : on quitte maintenant que c'est écrit
            if std::mem::take(&mut ctx.st.quit_after_save) {
                return Ok(Flow::Quit);
            }
            // une bascule attendait l'enregistrement (preset ou chaîne vierge)
            if let Some(what) = ctx.st.pending_switch.take() {
                *ctx.first = ctx.st.apply_pending(what);
            }
        }
        Err(e) => {
            ctx.st.message = Some(format!("enregistrement impossible : {e:#}"));
            ctx.st.quit_after_save = false;
            ctx.st.pending_switch = None;
        }
    }
    Ok(Flow::Go)
}

/// **`s` dans l'onglet Mélange** : le `.cab.toml`, et l'identifiant que la ligne de commande
/// finale pourra citer — tant que c'est encore ce mélange qu'on écoute.
pub(super) fn save_blend(ctx: &mut Ctx, name: &str) -> Result<Flow> {
    let file = ctx.library.join("cabs").join(format!("{}.cab.toml", name.replace(['/', '\\'], "-")));
    blend_spec(name, ctx.st, false).save(&file)?;
    let lib = Library::scan(ctx.library);
    let id = lib.by_path(&file).map(|e| e.id.clone()).unwrap_or_default();
    let rel = lib.relative(&file);
    ctx.st.message = Some(format!("mélange sauvé : {id} {name} ({})", file.display()));
    ctx.st.set_blend_file(rel, id.clone());
    *ctx.saved = Some((id, ctx.st.blend.clone(), ctx.st.align));
    ctx.st.mark_blend_saved();
    Ok(Flow::Go)
}

/// Les métadonnées éditées (les **différences** avec le fichier) dans `library/overrides.toml`.
fn write_meta(library: &std::path::Path, st: &State, edits: &[(usize, ModelMeta)]) -> Result<()> {
    let mut ov = Overrides::load(library)?;
    for (i, draft) in edits {
        let item = &st.models[*i];
        let file = item.meta.as_ref().map(|b| b.1.clone()).unwrap_or_default();
        let diff = |a: &String, b: &String| (a != b).then(|| a.clone());
        let o = ModelOverride {
            file: Some(item.rel.clone()),
            gear_type: diff(&draft.gear_type, &file.gear_type),
            gear_make: diff(&draft.make, &file.make),
            name: diff(&draft.name, &file.name),
            input_db: (draft.input_db != 0.0).then_some(draft.input_db),
            ir: draft.ir.map(|x| st.irs[x].rel.clone()),
            note: Some("namfoop browse".into()),
            ..Default::default()
        };
        ov.set(&content_hash(&item.path)?, o);
    }
    Ok(ov.save(library)?)
}

/// Le mélange tel qu'on l'enregistre — chemins **relatifs** à la bibliothèque — ou tel qu'on
/// l'**écoute** (`absolute`), dans le fichier temporaire que la chaîne cite.
pub(super) fn blend_spec(name: &str, s: &State, absolute: bool) -> dsp::blend::BlendSpec {
    dsp::blend::BlendSpec {
        name: name.to_string(),
        align: s.align,
        max_ms: None,
        parts: s
            .blend
            .iter()
            .map(|p: &BlendPart| dsp::blend::BlendPart {
                file: if absolute { s.irs[p.ir].path.clone() } else { PathBuf::from(&s.irs[p.ir].rel) },
                gain_db: p.gain_db,
                invert: p.invert,
            })
            .collect(),
        base: PathBuf::new(),
    }
}

/// La banque : les presets de `library/presets/`, chargés pour que la bascule ne lise plus de
/// fichier. Un preset illisible est signalé et listé grisé par l'état.
pub(in crate::browse) fn scan_presets(
    library: &std::path::Path,
) -> Vec<(String, String, PathBuf, dsp::preset::PresetSpec)> {
    let lib = Library::scan(library);
    let mut out = Vec::new();
    for e in lib.of_kind(Kind::Preset) {
        match lib.preset(&e.path) {
            Ok(spec) => out.push((e.id.clone(), e.stem.clone(), e.path.clone(), spec)),
            Err(err) => log::warn!("preset {} ignoré : {err:#}", e.rel),
        }
    }
    out
}
