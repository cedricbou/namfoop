//! **Ce qu'on entend** : la chaîne demandée au thread de construction, et le grain d'un modèle
//! mesuré par l'éditeur.
//!
//! Rien ici ne décide de ce qu'on écoute — c'est `State::plan()` qui le sait déjà (banc d'essai à
//! nu, blocs édités, essai du sélecteur, écoute isolée). Ce module **traduit** ce plan en demande,
//! et évite les demandes inutiles : la même chaîne que la dernière ne rebâtit rien.

use anyhow::Result;

use super::{Ctx, Flow};
use crate::browse::build::Job;
use crate::browse::plan::{plan_blocks, Request};
use crate::browse::state::{self, IrChoice};

/// La chaîne qu'on écoute, envoyée au thread de construction — si elle a changé.
pub(super) fn play(ctx: &mut Ctx) -> Result<Flow> {
    // le mélange d'écoute est écrit avant d'être cité par un bloc `ir`
    let plan = ctx.st.plan();
    if plan.leaves().any(|l| matches!(l, state::chain::Leaf::Ir { ir: IrChoice::Blend, .. })) {
        super::files::blend_spec("écoute", ctx.st, true).save(ctx.audition)?;
        ctx.st.blend_profile = dsp::profile::IrProfile::of_file(ctx.audition).ok().map(std::sync::Arc::new);
    }
    let mut req = Request {
        blocks: plan_blocks(&plan.blocks, ctx.st, ctx.audition),
        input_db: ctx.st.input_offset(),
        preset: None,
    };
    // cette chaîne est-elle exactement celle d'un preset ? alors elle est déjà bâtie dans la
    // banque : la comparaison porte sur les blocs résolus
    req.preset = ctx.bank_reqs.iter().position(|r| r.as_ref() == Some(&req));
    // même chaîne que la dernière : pas de reconstruction ni de fondu inutile (le mélange
    // d'écoute garde son nom de fichier mais change de contenu)
    let same = ctx.last_req.as_ref() == Some(&req) && !req.irs().iter().any(|p| p.as_path() == ctx.audition);
    if !same {
        *ctx.last_req = Some(req.clone());
        let _ = ctx.tx_req.send(Job::Chain(req));
        ctx.status.loading = true;
    }
    Ok(Flow::Go)
}

/// Le grain d'un modèle : mesuré dans un thread à part, parce que l'éditeur écoute pendant ce
/// temps. Le résultat revient par la file que la boucle relève.
pub(super) fn measure_grain(ctx: &mut Ctx, model: usize, input_db: f32) -> Result<Flow> {
    let _ = ctx.tx_grain.send((model, input_db, ctx.st.models[model].path.clone()));
    Ok(Flow::Go)
}
