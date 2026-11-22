# Spécification · le prompteur en grille

**Ce que couvre ce fichier** : ce que le prompteur du mode Set fait d'un fichier ChordPro écrit en
**grilles d'accords** — la lecture du fichier, l'écran, le suivi à la mesure —, le **son de chaque
section** choisi dans le set, et l'**ajout d'un morceau** à partir de son prompteur. Le mode Set
lui-même (le set, sa banque, la vue scène, la battue) est dans [`set.md`](set.md) : ses § 10 et 11
restent la référence de ce que le prompteur lit et de la carte des mesures, et ce fichier s'y
ajoute sans rien en retirer.

**État** : spécification d'un comportement **à construire**. Rien ici n'est vérifié par un test :
c'est la cible des stories de l'épique « Jouer un concert avec ma set list » qui portent sur le
prompteur. À mesure qu'elles sont livrées, le code et l'oracle du clavier font foi, [`set.md`](set.md)
reprend ce qui le concerne, et ce paragraphe se retire. Deux choix écrits ici se confirment avec
leur story : la touche `l` (§ 5), et le tempo d'un morceau ajouté, lu dans son prompteur (§ 6).

La maquette [`prompteur.html`](prompteur.html) est une **illustration actionnable** validée avec
l'utilisateur : à ouvrir dans un navigateur, elle se pilote au clavier et montre la mesure qui
s'allume, les deux fenêtres de l'édition et l'écran étroit. Le morceau, ses paroles et ses sons
sont fictifs, et son JavaScript n'est pas à reprendre.

**Couleurs** : « magenta », « cyan », « vert », « jaune », « rouge » et « gris » désignent ici le
magenta électrique, le cyan néon, le vert signal, le jaune ambre, le rouge clip et le gris acier de la
[charte](../identite/charte.md), qui dit aussi quand le terminal les remplace par ses couleurs de base.

## 1. Le besoin et les règles

Sur scène, un bassiste **compte des mesures**. Le prompteur dit d'abord où on en est dans la
grille, puis ce qui arrive : la section suivante, le changement de son. Les paroles ne sont qu'un
**repère** à côté de la grille.

Les fichiers viennent d'ailleurs. Un autre logiciel de l'utilisateur les écrit à partir du
morceau qu'il a travaillé — la structure que joue le groupe, les accords mesure par mesure, les
paroles —, et les **réécrit en entier** à chaque mise à jour du morceau. namfoop ne dépend pas de
ce logiciel : il lit du ChordPro, d'où qu'il vienne.

Quatre règles en découlent :

1. **Le standard d'abord.** Une grille est celle de ChordPro (`{start_of_grid}`), et le fichier
   attendu ne porte aucune directive `x_namfoop_…`. Celles-ci restent lues (§ 10 de `set.md`),
   pour les fichiers écrits à la main.
2. **Rien de propre à namfoop ne vit dans le fichier.** Il serait perdu à la prochaine réécriture :
   le son d'une section se choisit dans le set (§ 5).
3. **Un fichier bancal s'affiche quand même.** Une grille aux cases en trop, une barre manquante,
   un signe inconnu : la ligne se montre telle qu'elle est écrite, sans erreur.
4. **Les fichiers en paroles et accords en ligne** (`[Am]paroles`) se lisent et défilent comme le
   décrit le § 10 de `set.md`. La grille s'ajoute, elle ne remplace rien.

## 2. Le fichier : la grille de ChordPro

Le fichier de référence. Il sert de modèle à l'autre logiciel et de jeu d'essai aux tests :

```
{title: Le Phare}
{artist: Les Balises}
{key: Em}
{tempo: 124}
{time: 4/4}
{duration: 1:36}

{comment: basse seule les 4 premières}
{start_of_grid label="Intro" shape="4x4"}
| Em . . . | Em . . . | G . . . | D . . . |
| Em . . . | Em . . . | G . . . | D . . . |
{end_of_grid}

{start_of_grid label="Couplet 1" shape="4x4"}
| Em . . . | Em . . . | G . . . | D . . . | La lampe tourne et rien ne bouge / Sur le quai les filets sèchent
| Em . . . | Em . . . | G . . . | D . . . | On compte les bateaux qui rentrent / Il en manque un depuis hier
| C . . . | C . . . | G . . . | D . . . | Ta veste est restée sur la chaise / Le café refroidit tout seul
| C . . . | D . . . | Em . . . | Em . . . | Je garde un œil sur la passe / Et l'autre sur le téléphone
{end_of_grid}

{start_of_grid label="Refrain" shape="4x4"}
| C . . . | G . . . | D . . . | Em . . . | Allume, allume encore / Que la nuit sache où tu es
| C . . . | G . . . | D . C . | B7 . . . | Allume, allume encore / Je reste en haut, je veille
{end_of_grid}

{comment: regarde le batteur}
{start_of_grid label="Pont" shape="3x4"}
| Am . . . | F . . . | C . . . |
{end_of_grid}
{time: 2/4}
{start_of_grid label="Pont" shape="1x2"}
| G . |
{end_of_grid}
{time: 4/4}
{start_of_grid label="Pont" shape="4x4"}
| Am . . . | F . . . | C . . . | G . . . |
{end_of_grid}

{start_of_grid label="Refrain" shape="4x4"}
| C . . . | G . . . | D . . . | Em . . . | Allume, allume encore / Que la nuit sache où tu es
| C . . . | G . . . | D . C . | B7 . . . | Allume, allume encore / Je reste en haut, je veille
{end_of_grid}

{comment: fin sèche sur le 1}
{start_of_grid label="Fin" shape="2x4"}
| Em . . . | Em . . . |
{end_of_grid}
```

Ce que namfoop en lit, en plus du tableau du § 10 de `set.md` :

| ChordPro | Rendu |
|---|---|
| `{start_of_grid label="Couplet 1" shape="4x4"}` … `{end_of_grid}` | une **section en grille**, étiquetée par `label` |
| `{start_of_grid: 4x4 Couplet 1}` | la même, dans la forme sans attributs : la forme d'abord, le nom ensuite |
| `label="…"` sur tout `{start_of_…}` | l'étiquette de la section, comme le texte après `:` |
| `shape="4x4"` | ignoré : les mesures se comptent entre les barres de chaque ligne |
| `\| Em . . . \| G . . . \|` | une **ligne de mesures** : une case par temps, l'accord là où il commence |
| le texte après la dernière barre | le **repère** de la ligne : les paroles chantées sur ces mesures |
| le texte avant la première barre | affiché en gris devant la ligne |
| une ligne sans aucune barre, dans une grille | une ligne de texte |
| `{comment: …}` juste avant un `{start_of_…}` | la **cue de cette section**, en jaune sous son titre |
| `{key: Em}` | la tonalité, dans le titre du prompteur |
| `{time: 2/4}` entre deux grilles | un changement de mesure, valable à partir de là (§ 11 de `set.md`) |

Dans une ligne de mesures :

| Signe | Sens |
|---|---|
| `\|`, `\|\|`, `\|.` | une barre de mesure, quelle que soit sa forme |
| `.` | une case où l'accord précédent tient |
| `/` | une case où l'accord se rejoue |
| `%` | la mesure précédente, répétée : elle compte pour une mesure |
| `Am~C` | deux accords dans une même case |
| `\|:`, `:\|`, les voltas | affichés tels quels, **sans être dépliés** (§ 7) |

## 3. La durée et le suivi

- **La durée d'une section en grille est la somme des mesures de ses lignes.** La bande de battue
  et le compte à rebours du § 11 de `set.md` en vivent, sans `{x_namfoop_bars}`. Si la section
  porte quand même un `{x_namfoop_bars}`, c'est lui qui gagne.
- **Chaque ligne dure ses mesures.** La ligne courante est celle qui contient la mesure jouée, et
  dans cette ligne la mesure jouée est **allumée**. Une ligne de trois mesures suivie d'une ligne
  d'une seule se suivent donc exactement.
- Une section **sans grille** garde le partage du § 10 de `set.md` : ses lignes se répartissent ses
  mesures.
- **À l'arrêt, la position suit le son** et se pose sur la première mesure de la section liée ; **en
  marche, elle suit le temps**. C'est la règle du § 10 de `set.md`, inchangée.
- **Une mesure de longueur différente** a sa propre grille, entre deux `{time}` : la grille n'a pas
  de notation pour elle, et un `{time}` vaut à partir de l'endroit où il est écrit. Le fichier de
  référence l'écrit ainsi pour son pont. Ce sont trois sections pour la carte des mesures, chacune
  avec sa métrique ; `Fin` et `Début` vont de l'une à l'autre.

## 4. L'écran

La colonne du prompteur de la vue scène (§ 6 de `set.md`), à 160 colonnes :

```
┌ 2 · Le Phare · Em · son 1/2 Clean SVT · PgDn : ensuite Fuzz Face ─────────────────────────────────────────┐
│  ██1██   2      3      4      ♩124 4/4   mes. 3/16 COUPLET 1   ⏵                                          │
│  → Fuzz Face dans 14 mes.   REFRAIN                                                                       │
│ ────────────────────────────────────────────────────────────────────────────────────────────────────      │
│     | Em . . . | Em . . . | G . . .  | D . . .  |                                                         │
│                                                                                                           │
│ ▸ COUPLET 1    16 mes.  → Clean SVT                                                                       │
│   › | Em . . . | Em . . . |▌G . . . ▐| D . . .  |  La lampe tourne et rien ne bouge / Sur le quai les…    │
│     | Em . . . | Em . . . | G . . .  | D . . .  |  On compte les bateaux qui rentrent / Il en manque un…  │
│     | C . . .  | C . . .  | G . . .  | D . . .  |  Ta veste est restée sur la chaise / Le café refroidit… │
│     | C . . .  | D . . .  | Em . . . | Em . . . |  Je garde un œil sur la passe / Et l'autre sur le…      │
│                                                                                                           │
│   REFRAIN       8 mes.  → Fuzz Face                                                                       │
│     | C . . .  | G . . .  | D . . .  | Em . . . |  Allume, allume encore / Que la nuit sache où tu es     │
│     | C . . .  | G . . .  | D . C .  | B7 . . . |  Allume, allume encore / Je reste en haut, je veille    │
│                                                                                                           │
│   PONT          8 mes.  → Clean SVT                                                                       │
│       regarde le batteur                                                                                  │
│     | Am . . . | F . . .  | C . . .  |                                                                    │
│ 2/4 | G .      |                                                                                          │
│ 4/4 | Am . . . | F . . .  | C . . .  | G . . .  |                                                         │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Le titre** porte la tonalité après le nom du morceau, quand le fichier la donne.
- **Une ligne de mesures** : les accords en cyan gras, les barres et les points en gris. Les cases
  sont alignées d'une ligne à l'autre, sur la plus large de la section.
- **La mesure jouée** est en inverse vert (`▌G . . . ▐` ci-dessus) ; les mesures déjà jouées de la
  ligne courante sont grisées ; la ligne courante porte `›`.
- **Le repère** suit la ligne, à droite, tronqué par `…` s'il dépasse. Il est en gras sur la ligne
  courante.
- **La cue** d'une section s'affiche en jaune sous son titre.
- **Des sections voisines de même nom** s'affichent comme une seule : un titre, la somme de leurs
  mesures, et devant chaque grille dont la mesure change, sa mesure en gris (`2/4`, puis `4/4`). La
  bande de battue, elle, dit la mesure en cours comme d'habitude.
- Les sections passées sont grisées, la courante `▸` en vert, les suivantes en gras : § 6 de
  `set.md`.

**Terminal étroit.** La grille passe avant le reste, dans cet ordre :

1. quand la plus large ligne de mesures du morceau ne tient pas dans la colonne du prompteur, **la
   set list se replie** en une ligne au-dessus de lui, comme sous 70 colonnes (§ 6 de `set.md`) :
   c'est le cas à 80 colonnes pour une ligne de quatre mesures ;
2. quand il reste moins de vingt colonnes à droite de la ligne, **le repère passe dessous**, en gris ;
3. si la ligne ne tient toujours pas, elle se coupe **à une barre de mesure** et continue dessous.

À 64 colonnes :

```
┌ 2 · Le Phare · Em · son 1/2 Clean SVT ───────────────────────┐
│  ██1██   2      3      4    ♩124 4/4  mes. 3/16 COUPLET 1 ⏵  │
│  → Fuzz Face dans 14 mes.   REFRAIN                          │
│ ──────────────────────────────────────────────────────────── │
│ ▸ COUPLET 1    16 mes.  → Clean SVT                          │
│   › | Em . . . | Em . . . |▌G . . . ▐| D . . .  |            │
│       La lampe tourne et rien ne bouge / Sur le quai les…    │
│     | Em . . . | Em . . . | G . . .  | D . . .  |            │
│       On compte les bateaux qui rentrent / Il en manque un…  │
└──────────────────────────────────────────────────────────────┘
```

## 5. Le son de chaque section, dans le set

Le lien entre une section et un son est rangé dans le `.set.toml`, **par nom de section** :

```toml
[[song.sound]]
preset = "clean-svt"
sections = ["Intro", "Couplet 1", "Pont"]

[[song.sound]]
preset = "fuzz-face"
sections = ["Refrain"]
```

- Un nom vaut pour **toutes** les sections qui le portent : les deux refrains d'un coup. La
  comparaison ignore la casse et les espaces de bord.
- Une section que personne ne nomme **garde le son en cours** : ni compte à rebours, ni saut.
- **Le set gagne sur le fichier** : un `{x_namfoop_sound}` n'est lu que pour une section dont le set
  ne dit rien.
- Une section nommée dans le set et absente du prompteur est un **avertissement** : le set s'ouvre
  et le reste joue (§ 3 de `set.md`). Un même nom sous deux sons : avertissement, le premier gagne.
- `sections` ne s'écrit pas quand il est vide : le fichier reste lisible à la main.

En édition (§ 8 de `set.md`), `l` sur un morceau ouvre la fenêtre :

```
┌ Le Phare · quel son pour quelle section ───────────────┐
│   section        mes.   son                            │
│   Intro            8    1 Clean SVT                    │
│   Couplet 1       16    1 Clean SVT                    │
│ ▶ Refrain          8    2 Fuzz Face     × 2            │
│   Pont             8    1 Clean SVT                    │
│   Fin              2    —  garde le son en cours       │
│                                                        │
│ ←→ changer le son · Entrée valider · Échap annuler     │
└────────────────────────────────────────────────────────┘
```

Une ligne par **nom** de section, dans l'ordre du morceau, avec la durée d'une occurrence et leur
nombre s'il y en a plusieurs. `↑↓` visent, `←→` font défiler « garde le son en cours » puis les
sons du morceau, `Entrée` valide et le set devient « non enregistré », `Échap` annule sans rien
changer. Un morceau sans prompteur le dit et renvoie à `t`.

## 6. Ajouter un morceau à partir de son prompteur

En édition, `a` ouvre la saisie du titre (§ 8 de `set.md`) et, sous elle, les prompteurs du dossier
`library/sets/prompts/`, sous-dossiers compris :

```
┌ Ajouter un morceau ────────────────────────────────────────┐
│ titre : ▏                                                  │
│ ou ↓ un prompteur de prompts/                              │
│                                                            │
│ ▶ Le Phare         Em   ♩124  4/4   6 sections   1:36      │
│   Néon             Gm   ♩100  4/4   7 sections   3:10      │
│   Marée basse      D    ♩82   6/8   5 sections   4:02      │
│   sur-le-fil.cho   illisible : pas de l'UTF-8              │
│   · Ouverture      déjà dans le set                        │
│                                                            │
│ ↑↓ choisir · Entrée ajouter · Échap annuler                │
└────────────────────────────────────────────────────────────┘
```

- La saisie du titre garde la main : **tout caractère tapé est un titre**, et `Entrée` ajoute alors
  le morceau comme le décrit le § 8 de `set.md`.
- `↓` entre dans la liste. Chaque prompteur montre son titre (son nom de fichier s'il n'en a pas),
  sa tonalité, son tempo, sa mesure, son nombre de sections et sa durée.
- `Entrée` sur un prompteur ajoute le morceau après celui qu'on vise : son titre et son prompteur
  sont écrits dans le set ; **son tempo et sa mesure sont ceux du prompteur**, lus à chaque
  ouverture, et la set list les affiche. `T` les fixe dans le set si on veut autre chose.
- Un prompteur **déjà utilisé** par un morceau du set est marqué et ne s'ajoute pas ; un fichier
  **illisible** est listé avec sa raison et n'empêche pas la fenêtre de s'ouvrir.
- Sans aucun prompteur dans le dossier, la fenêtre est celle du § 8 de `set.md`.
- Le morceau ajouté n'a pas de son : il reste à lui en donner (`+`).

## 7. Les décisions de forme, et leur raison

- **La grille d'abord, les paroles en repère.** L'autre logiciel connaît les accords à la mesure,
  et les paroles à la ligne seulement : poser un accord sur une syllabe serait une estimation,
  alors que la mesure est sûre. Et c'est la mesure que compte un bassiste.
- **La grille de ChordPro plutôt qu'une écriture maison.** C'est la forme standard d'une grille de
  mesures (https://www.chordpro.org/chordpro/directives-env_grid/) : nom, cases, texte en marge
  droite. Le fichier s'imprime en grille dans le programme de référence et reste lisible comme du
  texte dans les logiciels qui ne dessinent pas les grilles.
- **Aucune directive commune ne lie un son à une section.** La convention de ChordPro pour une
  extension est le préfixe `x_` suivi du nom de l'application
  (https://www.chordpro.org/chordpro/chordpro-directives/), et les logiciels de set list règlent ce
  lien chez eux, pas dans le fichier. D'où le lien dans le set.
- **Par nom de section, pas par position.** Le fichier est réécrit quand le groupe change sa
  structure : un refrain ajouté est déjà lié, une section déplacée le reste.
- **Une mesure courte dans sa propre grille.** La grille a une seule forme, et `{time}` vaut à
  partir de l'endroit où il est écrit : c'est la seule écriture que le standard offre.
- **Les reprises ne se déplient pas.** L'autre logiciel écrit le morceau déroulé, dans l'ordre joué ;
  déplier `|:` `:|` et les voltas demanderait de décider ce que vaut une reprise mal fermée, pour
  un cas que le fichier attendu ne contient pas.
