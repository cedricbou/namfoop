# Spécification · éditeur de chaîne de `browse` (espace **Chaîne**)

Référence de comportement pour l'épique #103 (issues #104 à #111). Décisions : [ADR 0006](../adr/0006-chaine-en-blocs-et-parallele.md).
Maquette interactive validée par l'utilisateur : [`browse-chaine.html`](browse-chaine.html), à ouvrir dans un navigateur
(clavier, exemples, vumètres sur un signal simulé). La maquette fait foi pour le comportement ; son code
(JavaScript) n'est pas à reprendre. Les noms de modèles et d'IR qu'elle montre (`b7k`, `svt`, `fuzz`,
`sm57`, `8x10`…) sont fictifs.

## 1. Ce qu'on édite

- Une **chaîne** = une suite de **blocs** entre `IN` et `OUT` : effets du catalogue, `nam` (modèle),
  `ir`, et **parallèles**.
- Un **parallèle** (`◆`) : 2 à **4 branches**, chacune une suite de blocs (vide = son direct), avec
  gain, polarité, muet, et un nom en mode copie. Modes : **copie** ou **bandes** (2 à 4 bandes, N-1
  coupures). Pas de parallèle dans une branche. Plusieurs parallèles peuvent se suivre.
- Le niveau, le master et le limiteur restent hors de l'éditeur (fin de chaîne, comme aujourd'hui).

## 2. Synoptique

Une ligne de blocs par branche, une ligne de vumètres au-dessus de chaque ligne de blocs (§ 3).

```
IN ─ gate -60 ─ ◆─┬─ < 150 Hz ─ son direct ──────── -2 dB ─┬─ peak ─ reverb ─ OUT
                  └─ > 150 Hz ─ nam·b7k ─ ir·sm57 ─  0 dB ─┘

IN ─ hpf 30 ─ comp 4:1 ─ ◆─┬─ propre ─ lpf 250 ─ comp 8:1 ─────────────   0 dB ─┬─ gate -50 ─ OUT
                           ├─ fuzz   ─ nam·fuzz ─ ir·4x10 ─ hpf 150 ─ -3 dB S ─┤
                           └─ ampli  ─ nam·svt ─ ir·8x10 ─ peak ─────    -4 dB ─┘
```

- Séries : blocs séparés par ` ─ `. Entrée d'un parallèle : `◆─` puis `┬─` (branche 1), `├─`
  (branches du milieu), `└─` (dernière). Sortie : ` ─┬` (branche 1, la ligne continue à droite),
  ` ─┤`, ` ─┘`. Sur les lignes de vumètres, sous un `├`/`└`/`┤`/`┘`, un `│` relie les branches.
- **Toutes les lignes d'un parallèle ont la même largeur** : étiquettes complétées à droite, blocs
  suivis de `─` jusqu'à la colonne des gains, gains alignés à droite. Les traits verticaux sont dans
  la même colonne sur toutes les lignes.
- Étiquette de branche : en copie, son nom (défaut `B1`…`B4`) ; en bandes, `< 150 Hz`,
  `150–600 Hz`, `> 600 Hz`.
- Branche vide : `son direct` (en gris italique).
- Gain : `-3 dB`, `+2 dB`, suivi de ` ø` si la polarité est inversée et de ` S` si solo ; `muet` à la
  place si muette.
- Étiquettes de blocs : `nam·<modèle>`, `ir·<ir>`, `lpf <freq>`, `hpf <freq>`, `comp <ratio>:1`,
  `gate <seuil>`, sinon le nom de l'effet.
- Couleurs : effets en clair, `nam` / `ir` en vert, `◆` en jaune gras, en-têtes de branche en bleu,
  traits en gris. Bloc coupé : gris barré. Branche muette, ou hors solo quand une branche est en
  solo : toute la ligne assombrie. Curseur : fond cyan.
- Chaîne vide : `IN ─ (chaîne vide : a pour ajouter) ─ OUT`.
- Titre du cadre : `chaîne — NAM actifs : N · parallèles : P`, plus ` ⚠ CPU ≈ ×N (à mesurer sur Pi 5)` en
  jaune au-delà du seuil (#111 le fixe ; la maquette alerte dès 2). NAM actifs = blocs `nam` non coupés,
  dans une branche audible.
- Terminal trop étroit : défilement horizontal qui garde le curseur visible (hors maquette).

## 3. Vumètres

- Au-dessus de chaque bloc (largeur = celle de l'étiquette, 3 au moins), de chaque gain de branche
  (niveau de la branche avant la somme), de `IN` et de `OUT`.
- Crête de sortie, -60 à 0 dBFS sur la largeur, en huitièmes de caractère (`▏▎▍▌▋▊▉█`), barre
  fine (en terminal : choisir le rendu le plus discret lisible). Vert ; jaune au-dessus de -9 dB ;
  rouge au-dessus de -1 dB.
- Balistique : montée immédiate, descente 28 dB/s ; rafraîchissement ~25 images/s.
- `comp` : la réduction de gain remplit la barre **en orange depuis la droite** (20 dB = toute la
  barre, au moins une case dès 0,3 dB) ; montée immédiate, retour selon le `release` de l'effet.
- `gate` fermé : barre pleine **rouge sombre** ; se ferme en douceur (selon `release`), se rouvre
  tout de suite.
- Bloc coupé, branche muette ou hors solo : barre sombre.
- `v` masque / affiche toutes les lignes de vumètres.

## 4. Curseur et navigation (focus « chaîne »)

Positions possibles : un bloc hors parallèle ; le `◆` d'un parallèle ; l'**en-tête** d'une branche
(étiquette et gain, sélectionnés ensemble) ; un bloc dans une branche. Un clic (souris) sur un
élément le sélectionne si le terminal le permet.

| Touche | Effet |
|---|---|
| → | bloc suivant ; depuis `◆` : en-tête de la branche 1 ; depuis un en-tête : premier bloc de la branche ; depuis le dernier bloc d'une branche (ou un en-tête de branche vide) : bloc qui suit le parallèle |
| ← | inverse ; depuis un bloc qui suit un parallèle : dernier bloc de la branche 1 (ou son en-tête si vide) ; depuis le premier bloc d'une branche : son en-tête ; depuis un en-tête : `◆` |
| ↓ / ↑ | dans un parallèle : branche suivante / précédente en gardant la position (bornée à la longueur de la branche, en-tête si vide) ; ↓ sur `◆` : en-tête de la branche 1 ; ↑ sur la branche 1 : `◆` ; hors parallèle : rien |

## 5. Édition (focus « chaîne »)

| Touche | Effet | Message (barre du bas) |
|---|---|---|
| `a` / `i` | ouvre le catalogue ; ajoute le bloc choisi **après** / **avant** le curseur, dans sa branche. Sur `◆` : après / avant tout le parallèle. Sur un en-tête : en tête de la branche. Le curseur va sur le bloc ajouté. | `chorus ajouté` |
| `s` | sur un bloc hors parallèle : **le met en parallèle**. Si c'est un `nam` suivi d'un `ir`, les deux partent ensemble et le parallèle est en **bandes à 150 Hz** (grave : son direct ; aigu : `nam › ir`). Sinon : **copie**, B1 = son direct, B2 = le bloc. Curseur sur le bloc déplacé. Dans un parallèle : refusé. | `parallèle créé : crossover 150 Hz, grave = son direct, aigu = nam·b7k › ir·sm57` / `parallèle créé (copie) : B1 = son direct, B2 = chorus` / `un seul niveau de parallèle : sors le bloc avec [ ou ], ou ajoute une branche avec +` |
| `+` | dans un parallèle : ajoute une branche vide à la fin, curseur sur son en-tête. Bandes : ajoute une coupure = dernière × 4 (≤ 12 kHz). Au-delà de 4 : refusé. Hors parallèle : refusé. | `branche 3 ajoutée (son direct) : a pour y mettre un bloc` / `4 branches au maximum` / `+ s'utilise dans un parallèle (s pour en créer un)` |
| `x` / Suppr | bloc : le retire. En-tête : retire la branche (bandes : retire la coupure voisine) ; s'il ne reste qu'une branche, le parallèle est défait et ses blocs remis en série. `◆` : défait le parallèle en gardant la branche qui contient un `nam`, sinon la plus remplie. | `nam·b7k retiré` / `branche retirée : plus de parallèle, nam·b7k › ir·sm57 remis en série` / `parallèle défait : branche 2 gardée (nam·b7k › ir·sm57)` |
| `[` / `]` | déplace le bloc d'un cran dans la série. Un bloc hors parallèle voisin d'un parallèle y **entre** (branche 1 : au début avec `]`, à la fin avec `[`). Un bloc au bout d'une branche en **sort** (avant le parallèle avec `[`, après avec `]`). Sur `◆` : déplace tout le parallèle (sans entrer dans un autre). | `lpf 250 entre dans la branche 1 ({ } pour changer de branche)` / `lpf 250 sort du parallèle` |
| `{` / `}` | bloc dans une branche : le passe dans la branche du dessus / dessous, à la même position (bornée). | |
| `b` | bloc : coupé / actif. En-tête : muet. | `lpf 250 coupé` |
| `m` / `o` / `p` | branche (en-tête ou bloc de la branche) : muet / solo / polarité inversée. | `solo : oui` |
| Entrée, Tab | focus « réglages » (§ 6) si l'élément en a. Sur un bloc `nam` / `ir`, Entrée ouvre le **sélecteur sur place** (§ 5 de `browse-espaces.md`) | |
| Échap | ne quitte **jamais**. | `Échap revient en arrière mais ne quitte jamais : q pour quitter` |
| `v` | vumètres affichés / masqués. | `vumètres masqués` |
| `t` | déplie / replie le TOML du preset (replié par défaut, une ligne `▸ preset TOML (t)`). | |
| `S` | enregistrer le preset (§ 8). | |
| `q` | quitter (§ 8). | |

**Catalogue** (fenêtre au-dessus du synoptique) : titre `ajouter après le curseur` (ou `avant`),
filtre tapé au clavier (les noms qui **commencent** par le filtre d'abord, puis ceux qui le
contiennent dans le nom ou l'aide), ↑↓, Entrée ajoute, Échap annule, Retour arrière efface. Le
catalogue contient les effets, `nam` et `ir` ; pas de parallèle (on le crée avec `s`).

Chaque changement s'écoute après le délai d'écoute actuel (reconstruction de la chaîne + fondu).

## 6. Réglages (focus « réglages »)

↑↓ choisit le réglage, ←→ change la valeur (Maj : pas fin = pas / 10), Échap / Entrée / Tab
reviennent à la chaîne. Une barre (20 cases) montre la position dans la plage ; l'aide du réglage
s'affiche dessous.

- **Bloc** : ses paramètres (`ParamInfo`, comme aujourd'hui). `nam` : modèle (et niveau) ; `ir` : IR
  (et mix). Titre : `comp : compresseur · branche 2 (> 150 Hz)` pour un bloc dans une branche.
- **`◆`** : titre `parallèle : 3 / 4 branches, retards alignés automatiquement`.
  - `mode` : copie / bandes (←→). Passer en bandes fixe les coupures à 150 / 600 / 2400 Hz
    (selon le nombre de branches) ; passer en copie nomme les branches sans nom `B1`…
  - en bandes, `coupure 1`… : pas multiplicatif ×1,06 (Maj : ×1,01), arrondi à 10 Hz au-dessus de
    200 Hz, borné à 20 Hz–12 kHz et à ×1,25 des coupures voisines ; barre en échelle log.
  - dessous : `Entrée pour régler · + ajoute une branche · x défait le parallèle (garde la branche de l'ampli)`.
- **En-tête de branche** : `gain` (-60 à +12 dB, pas 0,5), `phase` (0° / 180°), `mute`, `solo`.

## 7. En-tête et barre d'aide

- En haut à droite : `preset : <nom> ✓`, ou `preset : <nom> ● non enregistré (S)` dès que la chaîne
  diffère du preset enregistré (comparaison d'états).
- Barre d'aide selon l'élément sous le curseur :
  - bloc : `←→ série  a/i ajouter  s mettre en parallèle  [ ] déplacer (entre dans un parallèle)  x retirer  b couper  Entrée régler`
  - `◆` : `↓ branches  + branche  Entrée mode et coupures  [ ] déplacer le parallèle  x défaire  a/i ajouter après / avant`
  - en-tête : `↑↓ branche  Entrée gain, phase  m muet  o solo  p phase  + branche  x retirer la branche  a ajouter au début`
  - bloc de branche : `←→ série  ↑↓ branche  { } changer de branche  [ ] déplacer (sort aux extrémités)  a/i ajouter  x retirer  Entrée régler`
  - réglages : `↑↓ réglage  ←→ valeur  Maj+←→ pas fin  Échap retour à la chaîne`
- Ligne de commande équivalente : affichée si la chaîne s'exprime en options (`-m`, `-i`, `--pre`,
  `--post`, `--crossover`), sinon `chaîne à enregistrer en preset (S)`.

## 8. Enregistrer et quitter

- `S` : fenêtre `Enregistrer le preset`, `nom : <nom actuel>` modifiable ; Entrée enregistre
  (`preset « <nom> » enregistré dans la bibliothèque`), Échap annule (`enregistrement annulé`).
- `q` sans modification : quitte. Avec modification : fenêtre
  `Modifications non enregistrées` / `Le preset « <nom> » a changé depuis le dernier enregistrement.`
  avec `S` enregistrer puis quitter, `Q` quitter sans enregistrer, `Échap` rester (défaut).
  **Toute autre touche reste** ; Maj, Ctrl, Alt seuls n'ont aucun effet.
- Avant les presets (#104) : même fenêtre sans `S`.

## 9. Exemples (boutons de la maquette)

Crossover DI (cas courant) :
```toml
name = "crossover di"
[[chain]]
fx = "gate"
[[chain]]
split = "crossover"
freqs = [150]
  [[chain.branches]]
  gain_db = -2
  fx = []
  [[chain.branches]]
  gain_db = 0
  fx = ["nam:model=b7k", "ir:file=sm57"]
[[chain]]
fx = "peak:gain=2"
[[chain]]
fx = "reverb:mix=15"
```

Trois branches, inspiré du son de Hysteria (approximation avec le catalogue actuel : la fuzz est une
capture NAM) : `hpf 30 › comp 4:1 › ◆ copie [propre : lpf 250 › comp 8:1 | fuzz -3 dB : nam fuzz › ir
4x10 › hpf 150 | ampli -4 dB : nam svt › ir 8x10 › peak 1200 +3] › gate -50`.

Quatre bandes : `comp › ◆ bandes 100 / 400 / 1600 [direct | nam svt › ir 8x10 | -3 dB : nam fuzz › ir
4x10 | -6 dB : chorus] › delay`.

Format TOML : proposition, à fixer dans #6. Le solo n'est jamais enregistré ; un bloc coupé
s'écrit `{ fx = "...", on = false }`.

## 10. Remplacé par l'épique #131 (deux espaces)

Ce paragraphe décrivait les onglets Modèles / IR agissant sur le bloc `nam` / `ir` « visé » dans la
chaîne, et Entrée sur un `nam` ouvrant l'onglet Modèles. **Abandonné** : voir
[`browse-espaces.md`](browse-espaces.md) — la Bibliothèque (banc d'essai, qui n'écrit jamais dans la
chaîne) et la Chaîne (où le modèle et l'IR d'un bloc se choisissent **sur place**, § 5). Le reste de
cette spécification est inchangé.

Les autres points laissés ouverts ici ont été tranchés ailleurs : défilement horizontal du
synoptique dans un terminal étroit (#108, fait), seuil d'alerte de charge CPU (#111, fait),
brouillon automatique en quittant sans enregistrer (#110, à faire).

## 11. Tests attendus

Logique dans `browse::state` (pure, horloge fournie) : chaque règle des § 4 à 8. Rendu avec
`TestBackend` / `screen_text` : les exemples du § 9, alignement des colonnes, assombrissements.
De bout en bout : pseudo-terminal, `--no-config --backend sim`, bibliothèque jetable,
`NAMFOOP_BROWSE_TRACE` pour vérifier la chaîne réellement installée.
