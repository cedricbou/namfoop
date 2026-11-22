//! Les couleurs des écrans, nommées par ce qu'elles **signifient**.
//!
//! C'est la liste unique : aucune couleur en dur ailleurs dans le binaire, pour qu'un changement de
//! teinte porte partout où la couleur a ce sens, et nulle part ailleurs. Deux usages qui partagent
//! une teinte aujourd'hui gardent deux noms dès qu'ils peuvent diverger demain — le jaune du mode
//! Édition et le jaune d'un vumètre qui chauffe, par exemple.
//!
//! Des **fonctions** et non des constantes : la teinte se choisit à l'exécution, selon ce que le
//! terminal sait afficher, sans retoucher un seul appel.
//!
//! Les noms et les familles viennent de [`docs/identite/charte.md`](../../../../docs/identite/charte.md).

use ratatui::style::Color;

// --- Les espaces et les modes ---------------------------------------------------------------

/// Espace Bibliothèque, et ce qu'on entend au banc d'essai.
pub fn library() -> Color {
    Color::Magenta
}

/// Espace Chaîne, et l'écoute A d'une comparaison.
pub fn chain() -> Color {
    Color::Cyan
}

/// Espace Set et vue scène.
pub fn set() -> Color {
    Color::Green
}

/// Mode édition : le champ qu'on règle, la ligne qu'on saisit.
pub fn edit() -> Color {
    Color::Yellow
}

/// La référence B d'une comparaison A/B.
pub fn reference_b() -> Color {
    Color::Yellow
}

/// Saturation, écrêtage, IR coupée : ce qui abîme le son.
pub fn clip() -> Color {
    Color::Red
}

// --- Le texte -------------------------------------------------------------------------------

/// Texte courant d'une liste ou d'un panneau.
pub fn text() -> Color {
    Color::Gray
}

/// Texte secondaire, élément inactif, aide en bas de panneau.
pub fn dim() -> Color {
    Color::DarkGray
}

/// Texte posé sur un aplat de couleur (les inversions).
pub fn on_accent() -> Color {
    Color::Black
}

/// La couleur que le terminal donnerait de lui-même.
pub fn inherit() -> Color {
    Color::Reset
}

// --- Les états ------------------------------------------------------------------------------

/// Ce qui est en place, validé, sain.
pub fn ok() -> Color {
    Color::Green
}

/// Avertissement, attente, ce qui demande un geste.
pub fn warn() -> Color {
    Color::Yellow
}

/// La touche d'une action sans retour : quitter, abandonner.
pub fn danger() -> Color {
    Color::Red
}

/// Erreur, refus, ce qui est cassé.
pub fn error() -> Color {
    Color::Red
}

// --- Les commandes et les fenêtres ------------------------------------------------------------

/// Une touche à presser, le curseur « ▶ », le cadre et le titre d'une fenêtre, l'invite de filtre.
pub fn key() -> Color {
    Color::Cyan
}

/// Le texte qu'on est en train de taper dans un filtre.
pub fn typed() -> Color {
    Color::Green
}

/// Le fond opaque d'une fenêtre modale, qui doit masquer l'écran du dessous.
pub fn popup_bg() -> Color {
    Color::Black
}

// --- Les vumètres ---------------------------------------------------------------------------

/// Niveau sain.
pub fn meter_ok() -> Color {
    Color::Green
}

/// Niveau qui approche du plafond.
pub fn meter_warn() -> Color {
    Color::Yellow
}

/// Niveau au plafond.
pub fn meter_clip() -> Color {
    Color::Red
}

/// La part de la barre que le compresseur réduit, prise sur la droite.
pub fn meter_action() -> Color {
    Color::LightRed
}

/// Le gate fermé : la barre entière.
pub fn meter_gate() -> Color {
    Color::Red
}

/// La part non remplie de la barre, et la barre d'un bloc coupé ou d'une branche muette.
pub fn meter_track() -> Color {
    Color::DarkGray
}

/// Le repère de calibration posé sur la barre.
pub fn meter_mark() -> Color {
    Color::White
}

// --- La chaîne en blocs -----------------------------------------------------------------------

/// Les nœuds d'un parallèle : « ◆ » où il s'ouvre, « ◇ » où les branches se somment.
pub fn split() -> Color {
    Color::Yellow
}

/// En-tête et gains d'une branche d'un parallèle.
pub fn branch() -> Color {
    Color::Blue
}

/// Le bloc qui fait le son : modèle d'ampli.
pub fn amp() -> Color {
    Color::Green
}

/// La jauge du réglage sous le curseur.
pub fn gauge_on() -> Color {
    Color::Cyan
}

/// La jauge d'un réglage au repos.
pub fn gauge_off() -> Color {
    Color::Blue
}

/// Le grain d'un modèle : clair, crunch, saturé.
pub fn grain_clean() -> Color {
    Color::Green
}

/// Voir [`grain_clean`].
pub fn grain_crunch() -> Color {
    Color::Yellow
}

/// Voir [`grain_clean`].
pub fn grain_hot() -> Color {
    Color::Red
}

/// L'écart entre deux courbes : dans la tolérance, à la limite, loin.
pub fn delta_ok() -> Color {
    Color::Green
}

/// Voir [`delta_ok`].
pub fn delta_warn() -> Color {
    Color::Yellow
}

/// Voir [`delta_ok`].
pub fn delta_far() -> Color {
    Color::Red
}

/// Le mini-spectre d'une ligne et les courbes de réponse.
pub fn curve() -> Color {
    Color::Cyan
}

/// Une valeur posée à la main : le PC d'un morceau, son tempo, le tempo tapé au pied.
pub fn value() -> Color {
    Color::Cyan
}

/// Une cue du prompteur : ce qui se lit sans se chanter.
pub fn cue() -> Color {
    Color::Yellow
}

/// Les accords posés au-dessus des paroles.
pub fn chord() -> Color {
    Color::Cyan
}

/// Le son qu'on essaie sans l'avoir adopté.
pub fn trial() -> Color {
    Color::Yellow
}

/// Un modèle dont le profil a été corrigé à la main.
pub fn corrected() -> Color {
    Color::Magenta
}

/// Une IR conseillée pour le modèle en cours.
pub fn recommended() -> Color {
    Color::Yellow
}

/// Le mélange d'IR : sa part dans une ligne, sa courbe.
pub fn blend() -> Color {
    Color::Magenta
}

// --- L'assistant de configuration ---------------------------------------------------------------

/// L'entrée où la basse est branchée.
pub fn input_bass() -> Color {
    Color::Cyan
}

/// L'entrée où un micro est branché.
pub fn input_mic() -> Color {
    Color::Magenta
}

/// Une entrée où du signal arrive.
pub fn signal() -> Color {
    Color::Green
}

/// La sortie qu'on écoute.
pub fn monitored() -> Color {
    Color::Cyan
}
