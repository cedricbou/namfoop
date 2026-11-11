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

    pub fn default_blend_name(&self) -> String {
        let first = self.blend.first().map(|p| &self.irs[p.ir]);
        let pack = first
            .map(|i| i.pack.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        format!("{pack} mix {}", self.blend.len())
    }

    /// Ligne de commande équivalente (affichée en quittant).
    pub fn command_line(&self, saved_blend_id: Option<&str>) -> Option<String> {
        let fx = self.fx.args();
        if self.model.is_none() && self.ir == IrChoice::None && fx.is_empty() {
            return None;
        }
        let mut cmd = "namfoop run".to_string();
        if let Some(m) = self.model {
            cmd += &format!(" -m {}", self.models[m].id);
        }
        match self.ir {
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
