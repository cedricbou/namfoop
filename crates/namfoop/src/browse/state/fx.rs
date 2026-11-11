//! Onglet Effets : catalogue (`dsp::fx::catalog`), chaîne pré / post autour du modèle et de l'IR,
//! réglage du paramètre choisi. Pas, plages, unités et aides viennent de `ParamInfo` : rien n'est
//! codé en dur ici. Chaque changement s'écoute après `AUDITION_DELAY_MS` sans touche.

use dsp::fx::{catalog, FxSpec, ParamInfo};

use super::*;

/// Colonne active de l'onglet (de gauche à droite).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxFocus {
    Catalog,
    Chain,
    Params,
}

/// Un effet de la chaîne, coupé ou non (`b`).
#[derive(Clone, Debug, PartialEq)]
pub struct FxSlot {
    pub spec: FxSpec,
    pub on: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Pre,
    Post,
}

#[derive(Clone, Debug)]
pub struct FxState {
    pub catalog: Vec<FxSpec>,
    pub pre: Vec<FxSlot>,
    pub post: Vec<FxSlot>,
    pub focus: FxFocus,
    pub catalog_cursor: usize,
    /// Effet choisi dans la chaîne : index dans `pre` puis `post` mis bout à bout.
    pub selected: usize,
    pub param: usize,
    /// A/B : on écoute sans aucun effet (`r` dans l'onglet).
    pub bypass_all: bool,
}

impl Default for FxState {
    fn default() -> Self {
        Self {
            catalog: catalog(),
            pre: Vec::new(),
            post: Vec::new(),
            focus: FxFocus::Catalog,
            catalog_cursor: 0,
            selected: 0,
            param: 0,
            bypass_all: false,
        }
    }
}

/// Valeur suivante d'un paramètre : `steps` pas (négatif : vers le bas ; 0,1 : pas fin), bornée,
/// arrondie au pas fin (évite 0,30000001 dans la ligne de commande).
pub fn step_value(p: &ParamInfo, v: f32, steps: f32) -> f32 {
    let fine = p.step * 0.1;
    let x = ((v + steps * p.step) / fine).round() * fine;
    ((x * 1e4).round() / 1e4).clamp(p.min, p.max)
}

impl FxState {
    pub fn len(&self) -> usize {
        self.pre.len() + self.post.len()
    }

    /// L'effet choisi : (pré ou post, index dans sa liste).
    pub fn place_of(&self, i: usize) -> Option<(Place, usize)> {
        if i < self.pre.len() {
            Some((Place::Pre, i))
        } else if i < self.len() {
            Some((Place::Post, i - self.pre.len()))
        } else {
            None
        }
    }

    pub fn slot(&self, i: usize) -> Option<&FxSlot> {
        match self.place_of(i)? {
            (Place::Pre, k) => self.pre.get(k),
            (Place::Post, k) => self.post.get(k),
        }
    }

    fn slot_mut(&mut self, i: usize) -> Option<&mut FxSlot> {
        match self.place_of(i)? {
            (Place::Pre, k) => self.pre.get_mut(k),
            (Place::Post, k) => self.post.get_mut(k),
        }
    }

    pub fn selected_slot(&self) -> Option<&FxSlot> {
        self.slot(self.selected)
    }

    /// Effets réellement écoutés (ni coupés, ni « sans effets »).
    pub fn active(&self) -> (Vec<FxSpec>, Vec<FxSpec>) {
        if self.bypass_all {
            return (Vec::new(), Vec::new());
        }
        let on = |v: &[FxSlot]| v.iter().filter(|s| s.on).map(|s| s.spec.clone()).collect();
        (on(&self.pre), on(&self.post))
    }

    /// Arguments équivalents pour `run` (`--pre gate --post delay:time=300`), effets coupés exclus.
    pub fn args(&self) -> String {
        let one = |flag: &str, v: &[FxSlot]| -> Vec<String> {
            v.iter().filter(|s| s.on).map(|s| format!("--{flag} {}", s.spec)).collect()
        };
        let mut a = one("pre", &self.pre);
        a.extend(one("post", &self.post));
        a.join(" ")
    }

    fn add(&mut self, place: Place) -> String {
        let Some(spec) = self.catalog.get(self.catalog_cursor).cloned() else {
            return String::new();
        };
        let kind = spec.kind();
        let slot = FxSlot { spec, on: true };
        match place {
            Place::Pre => {
                self.pre.push(slot);
                self.selected = self.pre.len() - 1;
            }
            Place::Post => {
                self.post.push(slot);
                self.selected = self.len() - 1;
            }
        }
        self.param = 0;
        self.focus = if self.selected_slot().is_some_and(|s| !s.spec.params().is_empty()) {
            FxFocus::Params
        } else {
            FxFocus::Chain
        };
        format!("{kind} ajouté en {}", if place == Place::Pre { "pré (avant le modèle)" } else { "post" })
    }

    fn remove(&mut self) -> Option<String> {
        let (place, k) = self.place_of(self.selected)?;
        let s = match place {
            Place::Pre => self.pre.remove(k),
            Place::Post => self.post.remove(k),
        };
        self.selected = self.selected.min(self.len().saturating_sub(1));
        self.param = 0;
        if self.len() == 0 {
            self.focus = FxFocus::Catalog;
        }
        Some(format!("{} retiré", s.spec.kind()))
    }

    /// Déplace l'effet choisi d'un cran (`delta` −1 ou +1) ; passe de pré à post (et retour) au bord,
    /// de part et d'autre du modèle.
    fn shift(&mut self, delta: isize) -> bool {
        let Some((place, k)) = self.place_of(self.selected) else { return false };
        match (place, delta) {
            (Place::Pre, -1) if k > 0 => self.pre.swap(k, k - 1),
            (Place::Pre, 1) if k + 1 < self.pre.len() => self.pre.swap(k, k + 1),
            (Place::Pre, 1) => {
                // dernier du pré : passe en tête du post (position inchangée dans la liste)
                let s = self.pre.remove(k);
                self.post.insert(0, s);
                return true;
            }
            (Place::Post, -1) if k > 0 => self.post.swap(k, k - 1),
            (Place::Post, -1) => {
                let s = self.post.remove(0);
                self.pre.push(s);
                return true;
            }
            (Place::Post, 1) if k + 1 < self.post.len() => self.post.swap(k, k + 1),
            _ => return false,
        }
        self.selected = (self.selected as isize + delta) as usize;
        true
    }

    /// Change le paramètre choisi de `steps` pas ; renvoie le texte « nom = valeur unité ».
    fn adjust(&mut self, steps: f32) -> Option<String> {
        let param = self.param;
        let slot = self.slot_mut(self.selected)?;
        let p = *slot.spec.params().get(param)?;
        let v = step_value(&p, slot.spec.get(p.name)?, steps);
        slot.spec.set(p.name, v).ok()?;
        Some(format!("{} : {} = {} {}", slot.spec.kind(), p.name, v, p.unit.symbol()))
    }
}

impl State {
    /// Touches de l'onglet Effets ; `None` : touche commune (Tab, q, filtre...).
    pub(super) fn fx_key(&mut self, key: Key) -> Option<Action> {
        let fx = &mut self.fx;
        let n_params = fx.selected_slot().map_or(0, |s| s.spec.params().len());
        let changed = match (fx.focus, key) {
            (_, Key::Char('a')) => {
                self.message = Some(fx.add(Place::Pre));
                true
            }
            (_, Key::Char('A')) => {
                self.message = Some(fx.add(Place::Post));
                true
            }
            (_, Key::Char('r')) => {
                fx.bypass_all = !fx.bypass_all;
                self.message = Some(
                    if fx.bypass_all {
                        "A/B : sans effets (r pour les remettre)"
                    } else {
                        "A/B : avec effets"
                    }
                    .into(),
                );
                true
            }
            (FxFocus::Catalog, Key::Up) => {
                fx.catalog_cursor = fx.catalog_cursor.saturating_sub(1);
                false
            }
            (FxFocus::Catalog, Key::Down) => {
                fx.catalog_cursor = (fx.catalog_cursor + 1).min(fx.catalog.len() - 1);
                false
            }
            (FxFocus::Catalog, Key::Right | Key::Enter) if fx.len() > 0 => {
                fx.focus = FxFocus::Chain;
                false
            }
            (FxFocus::Chain, Key::Left) => {
                fx.focus = FxFocus::Catalog;
                false
            }
            (FxFocus::Chain, Key::Up) => {
                fx.selected = fx.selected.saturating_sub(1);
                fx.param = 0;
                false
            }
            (FxFocus::Chain, Key::Down) => {
                fx.selected = (fx.selected + 1).min(fx.len().saturating_sub(1));
                fx.param = 0;
                false
            }
            (FxFocus::Chain, Key::Right | Key::Enter) if n_params > 0 => {
                fx.focus = FxFocus::Params;
                false
            }
            (FxFocus::Params, Key::Enter | Key::Esc) => {
                fx.focus = FxFocus::Chain;
                false
            }
            (FxFocus::Params, Key::Up) => {
                fx.param = fx.param.saturating_sub(1);
                false
            }
            (FxFocus::Params, Key::Down) => {
                fx.param = (fx.param + 1).min(n_params.saturating_sub(1));
                false
            }
            (FxFocus::Params, Key::Left | Key::Right | Key::ShiftLeft | Key::ShiftRight) => {
                let steps = match key {
                    Key::Left => -1.0,
                    Key::Right => 1.0,
                    Key::ShiftLeft => -0.1,
                    _ => 0.1,
                };
                match fx.adjust(steps) {
                    Some(m) => {
                        self.message = Some(m);
                        true
                    }
                    None => false,
                }
            }
            (FxFocus::Chain | FxFocus::Params, Key::Char('x') | Key::Backspace) => match fx.remove() {
                Some(m) => {
                    self.message = Some(m);
                    true
                }
                None => false,
            },
            (FxFocus::Chain | FxFocus::Params, Key::Char('[')) => fx.shift(-1),
            (FxFocus::Chain | FxFocus::Params, Key::Char(']')) => fx.shift(1),
            (FxFocus::Chain | FxFocus::Params, Key::Char('b')) => {
                let i = fx.selected;
                match fx.slot_mut(i) {
                    Some(s) => {
                        s.on = !s.on;
                        self.message =
                            Some(format!("{} {}", s.spec.kind(), if s.on { "remis" } else { "coupé (b)" }));
                        true
                    }
                    None => false,
                }
            }
            (_, Key::Esc | Key::Char('q') | Key::Tab) => return None,
            _ => false,
        };
        if changed {
            // écoute différée : la chaîne se reconstruit après AUDITION_DELAY_MS sans touche
            self.fx_due = Some(self.now);
        }
        Some(Action::None)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::sample;
    use super::*;

    fn fx_tab() -> State {
        let mut s = sample();
        for _ in 0..3 {
            s.on_key(Key::Tab);
        }
        assert_eq!(s.tab, Tab::Fx);
        s
    }

    /// Met le curseur du catalogue sur `kind`.
    fn pick(s: &mut State, kind: &str) {
        let i = s.fx.catalog.iter().position(|f| f.kind() == kind).expect("au catalogue");
        while s.fx.catalog_cursor > i {
            s.on_key(Key::Up);
        }
        while s.fx.catalog_cursor < i {
            s.on_key(Key::Down);
        }
    }

    /// Revient au catalogue (← depuis la chaîne, Entrée puis ← depuis les réglages).
    fn to_catalog(s: &mut State) {
        if s.fx.focus == FxFocus::Params {
            s.on_key(Key::Enter);
        }
        if s.fx.focus == FxFocus::Chain {
            s.on_key(Key::Left);
        }
        assert_eq!(s.fx.focus, FxFocus::Catalog);
    }

    /// La ligne de commande affichée se ré-analyse en la même chaîne que celle écoutée.
    fn reparse(args: &str) -> (Vec<FxSpec>, Vec<FxSpec>) {
        let (mut pre, mut post) = (Vec::new(), Vec::new());
        let mut it = args.split_whitespace();
        while let (Some(flag), Some(v)) = (it.next(), it.next()) {
            let f: FxSpec = v.parse().unwrap();
            if flag == "--pre" {
                pre.push(f)
            } else {
                post.push(f)
            }
        }
        (pre, post)
    }

    #[test]
    fn add_remove_move_and_command_line() {
        let mut s = fx_tab();
        pick(&mut s, "gate");
        s.on_key(Key::Char('a'));
        assert_eq!(s.fx.focus, FxFocus::Params, "réglages ouverts sur l'effet ajouté");
        to_catalog(&mut s);
        pick(&mut s, "comp");
        s.on_key(Key::Char('a'));
        s.on_key(Key::Down); // ratio
        s.on_key(Key::Right);
        to_catalog(&mut s);
        pick(&mut s, "delay");
        s.on_key(Key::Char('A'));
        assert_eq!(s.fx.args(), "--pre gate --pre comp:ratio=4.5 --post delay");
        assert_eq!(reparse(&s.fx.args()), s.fx.active());
        // delay remonte : passe en fin de pré, puis devant comp
        s.on_key(Key::Enter); // réglages -> chaîne
        s.on_key(Key::Char('['));
        assert_eq!(s.fx.args(), "--pre gate --pre comp:ratio=4.5 --pre delay");
        s.on_key(Key::Char('['));
        assert_eq!(s.fx.args(), "--pre gate --pre delay --pre comp:ratio=4.5");
        assert_eq!(s.fx.selected, 1);
        s.on_key(Key::Char(']'));
        s.on_key(Key::Char(']'));
        assert_eq!(s.fx.args(), "--pre gate --pre comp:ratio=4.5 --post delay", "et redescend en post");
        // retrait
        s.on_key(Key::Up);
        s.on_key(Key::Char('x'));
        assert_eq!(s.fx.args(), "--pre gate --post delay");
        assert_eq!(reparse(&s.fx.args()), s.fx.active());
        assert_eq!(s.on_key(Key::Char('q')), Action::Quit);
    }

    #[test]
    fn values_stop_at_the_bounds() {
        let mut s = fx_tab();
        pick(&mut s, "gain");
        s.on_key(Key::Char('A'));
        let p = s.fx.selected_slot().unwrap().spec.params()[0];
        for _ in 0..200 {
            s.on_key(Key::Right);
        }
        assert_eq!(s.fx.selected_slot().unwrap().spec.get("db"), Some(p.max));
        for _ in 0..400 {
            s.on_key(Key::Left);
        }
        assert_eq!(s.fx.selected_slot().unwrap().spec.get("db"), Some(p.min));
        s.on_key(Key::ShiftRight);
        let v = s.fx.selected_slot().unwrap().spec.get("db").unwrap();
        assert!((v - (p.min + p.step * 0.1)).abs() < 1e-4, "pas fin : {v}");
        // le texte se ré-analyse en la même valeur
        assert_eq!(reparse(&s.fx.args()).1, s.fx.active().1);
    }

    #[test]
    fn audition_is_deferred_and_bypasses_work() {
        let mut s = fx_tab();
        s.tick(1000);
        pick(&mut s, "chorus");
        assert_eq!(s.tick(1500), Action::None, "parcourir le catalogue ne recharge rien");
        s.on_key(Key::Char('A'));
        assert_eq!(s.tick(1600), Action::None, "pas avant 150 ms");
        assert_eq!(s.tick(1660), Action::Play { model: None, ir: IrChoice::None });
        assert_eq!(s.tick(2000), Action::None, "une seule écoute");
        assert_eq!(s.fx.active().1.len(), 1);
        s.on_key(Key::Char('b'));
        assert!(s.fx.active().1.is_empty() && s.fx.args().is_empty(), "effet coupé : ni écouté ni copié");
        assert!(matches!(s.tick(3000), Action::Play { .. }));
        s.on_key(Key::Char('b'));
        s.on_key(Key::Char('r'));
        assert!(s.fx.bypass_all && s.fx.active().1.is_empty());
        assert_eq!(s.fx.args(), "--post chorus", "la ligne garde la chaîne réglée");
        assert!(matches!(s.tick(4000), Action::Play { .. }));
    }

    #[test]
    fn command_line_includes_effects() {
        let mut s = fx_tab();
        pick(&mut s, "reverb");
        s.on_key(Key::Char('A'));
        assert_eq!(s.command_line(None).as_deref(), Some("namfoop run --post reverb"));
    }

    #[test]
    fn step_value_rounds_to_the_fine_step() {
        let p = ParamInfo {
            name: "x",
            unit: dsp::fx::Unit::Hz,
            min: 0.05,
            max: 5.0,
            default: 0.8,
            step: 0.05,
            help: "-",
        };
        assert_eq!(step_value(&p, 0.8, 1.0), 0.85);
        assert_eq!(step_value(&p, 0.8, -0.1), 0.795);
        assert_eq!(step_value(&p, 4.99, 1.0), 5.0);
        assert_eq!(step_value(&p, 0.05, -1.0), 0.05);
    }
}
