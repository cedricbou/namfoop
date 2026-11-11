//! Les listes des onglets Modèles et IR : filtre par mots, lignes affichées, et les IR conseillées
//! pour le modèle en cours d'abord.

use library::recommend::NeedsIr;

use super::{IrRow, Item, ModelRow, State};

fn matches(item: &Item, words: &[String]) -> bool {
    let hay = format!("{} {} {}", item.id, item.label, item.pack).to_lowercase();
    words.iter().all(|w| hay.contains(w))
}

impl State {
    fn words(&self) -> Vec<String> {
        self.filter.to_lowercase().split_whitespace().map(String::from).collect()
    }

    /// Modèles visibles (index dans `models`).
    pub fn model_rows(&self) -> Vec<usize> {
        let w = self.words();
        (0..self.models.len()).filter(|&i| matches(&self.models[i], &w)).collect()
    }

    /// Lignes de l'onglet Modèles : « son direct (DI) » (sans filtre), puis les modèles visibles.
    pub fn model_list(&self) -> Vec<ModelRow> {
        let mut rows = Vec::new();
        if self.filter.trim().is_empty() {
            rows.push(ModelRow::Direct);
        }
        rows.extend(self.model_rows().into_iter().map(ModelRow::Model));
        rows
    }

    /// Modèle sous le curseur de l'onglet Modèles (aucun sur la ligne « son direct »).
    pub fn model_at_cursor(&self) -> Option<usize> {
        match self.model_list().get(self.cursor()) {
            Some(ModelRow::Model(m)) => Some(*m),
            _ => None,
        }
    }

    /// Lignes de l'onglet IR : « aucune IR », les conseillées pour le modèle courant, puis le reste.
    pub fn ir_rows(&self) -> Vec<IrRow> {
        let w = self.words();
        let rec: Vec<usize> = self
            .model
            .filter(|&m| self.models[m].needs != NeedsIr::No)
            .map(|m| self.models[m].recommended.iter().copied().take(5).collect())
            .unwrap_or_default();
        let mut rows = Vec::new();
        if w.is_empty() {
            rows.push(IrRow::NoIr);
        }
        rows.extend(
            rec.iter()
                .filter(|&&i| matches(&self.irs[i], &w))
                .map(|&index| IrRow::Ir { index, recommended: true }),
        );
        rows.extend(
            (0..self.irs.len())
                .filter(|i| !rec.contains(i) && matches(&self.irs[*i], &w))
                .map(|index| IrRow::Ir { index, recommended: false }),
        );
        rows
    }
}
