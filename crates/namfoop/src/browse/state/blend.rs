//! Mélanges d'IR (onglet IR) : réglage des parts, nom par défaut, ligne de commande à rejouer.

use super::*;

impl State {
    pub(super) fn adjust(&mut self, f: impl FnOnce(&mut BlendPart)) -> Action {
        let c = self.cursor();
        if let Some(p) = self.blend.get_mut(c) {
            f(p);
            p.gain_db = p.gain_db.clamp(-30.0, 12.0);
            return self.blend_changed();
        }
        Action::None
    }

    /// Le mélange a changé : on l'écoute (ou on revient à la dernière IR s'il est vide).
    pub(super) fn blend_changed(&mut self) -> Action {
        self.fresh();
        if self.blend.is_empty() {
            if self.ir == IrChoice::Blend {
                self.ir = IrChoice::None;
            }
        } else {
            self.ir = IrChoice::Blend;
        }
        self.message = Some(format!("mélange : {} IR", self.blend.len()));
        self.play()
    }

    /// Le mélange en cours diffère du dernier enregistré (`s` dans l'onglet Mélange).
    pub fn blend_unsaved(&self) -> bool {
        !self.blend.is_empty() && (self.blend != self.saved_blend.0 || self.align != self.saved_blend.1)
    }

    pub fn default_blend_name(&self) -> String {
        let first = self.blend.first().map(|p| &self.irs[p.ir]);
        let pack = first
            .map(|i| i.pack.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        format!("{pack} mix {}", self.blend.len())
    }

    /// Ligne de commande équivalente (affichée en quittant), quand la chaîne s'exprime en
    /// options : sinon il faut un preset (les branches ne se décrivent pas en texte).
    ///
    /// Elle décrit **la chaîne**, pas le banc d'essai : depuis les deux espaces, le banc
    /// peut jouer autre chose que ce qui est dans la chaîne, et c'est la chaîne qu'on rejoue.
    pub fn command_line(&self, saved_blend_id: Option<&str>) -> Option<String> {
        if !self.fx.chain.fits_command_line() {
            return None;
        }
        let model = self.fx.chain.leaves().find_map(|l| match l {
            chain::Leaf::Nam { model, on: true } => *model,
            _ => None,
        });
        let ir = self
            .fx
            .chain
            .leaves()
            .find_map(|l| match l {
                chain::Leaf::Ir { ir, on: true } => Some(*ir),
                _ => None,
            })
            .unwrap_or(IrChoice::None);
        let fx = self.fx_args();
        if model.is_none() && ir == IrChoice::None && fx.is_empty() {
            return None;
        }
        let mut cmd = "namfoop run".to_string();
        if let Some(m) = model {
            cmd += &format!(" -m {}", self.models[m].id);
        }
        match ir {
            IrChoice::Item(i) => cmd += &format!(" -i {}", self.irs[i].id),
            IrChoice::Blend => match saved_blend_id {
                Some(id) => cmd += &format!(" -i {id}"),
                None => cmd += "   (mélange non sauvé : s dans l'onglet Mélange)",
            },
            IrChoice::None => {}
        }
        if !fx.is_empty() {
            cmd += &format!(" {fx}");
        }
        Some(cmd)
    }
}

impl State {
    /// Les effets de la chaîne en `--pre` / `--post` : ce qui précède le modèle (ou l'IR) d'un côté,
    /// ce qui le suit de l'autre. N'a de sens que si la chaîne `fits_command_line`.
    pub fn fx_args(&self) -> String {
        let amp = self
            .fx
            .chain
            .blocks
            .iter()
            .position(|b| matches!(b, chain::Block::Leaf(l) if l.is_nam() || l.is_ir()));
        let mut out = Vec::new();
        for (i, b) in self.fx.chain.blocks.iter().enumerate() {
            if let chain::Block::Leaf(chain::Leaf::Fx(slot)) = b {
                if !slot.on {
                    continue;
                }
                let place = if amp.is_some_and(|a| i > a) { "--post" } else { "--pre" };
                out.push(format!("{place} {}", slot.spec));
            }
        }
        out.join(" ")
    }
}
