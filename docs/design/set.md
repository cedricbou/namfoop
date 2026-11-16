# Spécification · le mode Set : la set list de concert

Référence de comportement pour l'épique #215 « Mode Set » (née de l'échange du 15/11/2026). Elle
ajoute un **troisième espace** à `browse` (après les deux espaces de
[`browse-espaces.md`](browse-espaces.md)) et un mode scène à `run`. Elle ne change rien aux espaces
Bibliothèque et Chaîne, sauf ce que dit le § 13.

Maquette interactive : [`set.html`](set.html), à ouvrir dans un navigateur (clavier, deux sets
d'exemple, mode `run --set`, Program Change et horloge simulés). La maquette fait foi pour le
comportement ; son code (JavaScript) n'est pas à reprendre. Presets, morceaux et textes sont fictifs.

## 1. Le besoin et les règles

Deux usages réels de l'utilisateur :

- **Groupe 1** : un preset par morceau, deux pour ceux où l'on alterne clair et saturé (ou synthé).
  Les sons se changent au pied (`PgDn` / `PgUp`) ou en MIDI (Program Change).
- **Un autre show** : trois sons seulement, qu'on alterne au pied selon les morceaux.

Et à terme : voir le morceau joué, le son joué, et un **prompteur** (accords, paroles, cues) dont le
texte viendra d'autres logiciels ; namfoop l'affiche et le surligne selon les Program Change ou
l'avancement dans le morceau. La scène se joue le plus souvent avec la suite Ableton.

Règles :

1. **Sur scène, le pied suffit.** Son suivant / précédent, morceau suivant / précédent, défilement du
   prompteur : tout a une touche (réglable) ou un message MIDI.
2. **Un set référence des presets, il ne les copie pas.** Modifier un preset le change dans tous les
   morceaux. Le morceau ne règle que ce qui lui est propre : son tempo, le volume de chaque son.
3. **On sait toujours** quel morceau on joue, quel son, et ce qui vient au prochain appui.
4. **La scène ne s'édite pas par erreur** : l'édition est un mode explicite (`e`) ; `run --set` ne
   permet aucune édition.
5. **Sans set, rien ne change** : la banque entière, en boucle, comme aujourd'hui (#110).

## 2. Vocabulaire et état de l'art

**Set** › **morceaux** (dans l'ordre) › **sons** (1 à n par morceau). Un **morceau** porte un n° de
Program Change, son tempo, sa mesure et son prompteur ; un **son** est un preset plus le volume que
ce morceau lui donne. Un son n'a **pas** de n° de PC : il est désigné par sa place (§ 3, § 5).
(Corrigé en réalisant l'US 1 : cette phrase disait l'inverse du § 3.)

C'est la hiérarchie des multi-effets du marché : la *Performance* Kemper (5 slots de rigs par morceau),
le *preset par morceau + snapshots par partie* des utilisateurs de Helix, les *setlists* du Quad
Cortex. Les logiciels de set list (BandHelper, OnSong, AbleSet pour Ableton) associent de même un
Program Change à chaque morceau et défilent le texte selon le tempo ou la durée. On s'en inspire, on
ne réinvente pas : le prompteur lit le format **ChordPro** (§ 10), commun à ces logiciels.

Un set sans titres de morceau (le show à trois sons) est un set à **un seul morceau sans titre** :
l'écran montre alors directement ses sons.

## 3. Le fichier `library/sets/<nom>.set.toml`

```toml
name = "Groupe 1 · Live Mairie"

[[song]]
title = "Le Phare"
pc = 2                               # facultatif : Program Change 1-128 qui appelle CE morceau (unique dans le set)
tempo_bpm = 124                      # facultatif : prime sur le tempo de chaque preset du morceau (à la noire)
time = "4/4"                         # facultatif : mesure par défaut ; les changements sont dans le prompteur
prompt = "prompts/le-phare.cho"      # facultatif : ChordPro, relatif à library/sets/ (§ 10)
  [[song.sound]]                     # son 1 : CC 69 valeur 0
  preset = "fuzz-face"               # identifiant du preset (celui de `namfoop presets`)
  [[song.sound]]                     # son 2 : CC 69 valeur 1
  preset = "clean-svt"
  volume_db = -1.5                   # facultatif : s'ajoute au volume du preset, dans ce morceau

[[song]]
title = "Marée basse"
pc = 4
tempo_bpm = 82
time = "6/8"
  [[song.sound]]
  preset = "clean-svt"
  [[song.sound]]
  preset = "drive-b7k"
```

Le show à trois sons :

```toml
name = "Trio acoustique"
[[song]]
pc = 1
  [[song.sound]]
  preset = "acou-di"
  [[song.sound]]
  preset = "fretless"
  [[song.sound]]
  preset = "dub-sub"
```

Un son n'a **pas** de n° de PC : il est désigné par sa place dans le morceau (§ 5), comme les
snapshots Helix ou les scènes du Quad Cortex. Réordonner les sons d'un morceau change donc leur
valeur de CC.

Contrôles à la lecture (`namfoop sets`, ouverture d'un set) :

- preset introuvable : **avertissement**, le son est montré `⚠ <id> introuvable` et sauté par
  `PgDn` / `PgUp` (le reste du set joue) ;
- même `pc` sur **deux morceaux** du set : erreur (le set ne s'ouvre pas, le message nomme les deux) ;
- tempo hors 30-300, PC hors 1-128, mesure illisible : erreur ;
- morceau sans son : permis (en cours d'écriture), sauté par la navigation ; morceau sans `pc` :
  permis, il ne se joue qu'au pied.

`namfoop sets` liste les sets comme `namfoop presets` : identifiant, nom, nombre de morceaux et de sons,
avertissements.

## 4. La banque du set

- Quand un set est **actif**, la banque préchargée (#6) devient **celle du set** : seuls ses presets
  sont préparés, une place par preset **distinct** (le volume et le tempo étant des paramètres vivants,
  ADR 0007 et 0008, un même preset utilisé dans dix morceaux n'occupe qu'une place).
- Passer d'un son à l'autre installe la place préparée (20 ms, queue de réverbe du son quitté, #172),
  puis pose le **volume du son** (volume du preset + `volume_db` du morceau) et le **tempo** (celui du
  morceau s'il en a un, sinon celui du preset) dans leurs cases vivantes, sans rebâtir de chaîne.
- Le tempo du morceau entre dans l'arbitre (`engine::tempo::Arbiter`) comme le ferait celui d'un
  preset : un tap (`*`) ou une horloge MIDI le recouvrent ensuite, comme aujourd'hui (#200).
- Le set actif est retenu d'une session à l'autre (`browse` rouvre le dernier set) ; « aucun set »
  rend la banque entière.

## 5. Se déplacer

**`PgDn` / `PgUp` : la liste à plat**, dans l'espace Set et sur scène (#247 : **pas** dans la
Chaîne, qui garde sa banque). Son suivant / précédent ; après le dernier son d'un morceau, le
premier du morceau suivant. **Pas de bouclage** : sur le dernier son du set, `PgDn` ne fait rien et
le message dit `fin du set`. Sans set, l'espace Set le dit (`aucun set : L pour en choisir un`).

**Morceau suivant / précédent** : deux touches réglables (défaut `→` / `←`), qui vont au **premier
son** du morceau. Elles servent quand un morceau finit sur son 2e son, et aux pédaliers à 3 boutons
et plus.

**↑↓ puis Entrée** (espace 3 seulement) : viser un morceau sans changer le son, puis le jouer (son 1).
Rien ne change à l'oreille tant qu'on n'a pas appuyé sur Entrée.

**MIDI** (avec #7, § 12) : adressage hiérarchique, comme tous les pédaliers du marché — Helix :
CC32 setlist › PC preset › CC69 snapshot ; Quad Cortex : CC0/32 › PC › CC43 scène ; Fractal : bank ›
PC › CC de scène ; Kemper : PC Performance › CC 50-54 slot.

- **PC n → le morceau dont le `pc` vaut n**, sur son son 1. Le n° est fixe : réordonner le set ne le
  change pas, et un rappel joué au milieu du set (PC 2 → « Le Phare ») tombe toujours juste. Un PC
  qu'aucun morceau ne porte est ignoré et le message le dit.
- **CC 69 valeur k → le son k+1 du morceau joué** (n° de CC réglable, 69 par défaut comme les
  snapshots Helix). Valeur hors du morceau : ignorée, message.
- **PC suivi d'un CC** (Ableton envoie souvent les deux en tête de clip) : après un PC, namfoop attend
  une courte fenêtre (≈ 10 ms) un CC 69 éventuel et ne fait **qu'un** changement de son.

## 6. L'espace 3 : vue scène (deux colonnes)

**La set list à gauche, le prompteur à droite**, tous deux sur toute la hauteur : la hauteur sert à
**voir venir** (les morceaux suivants, les sections suivantes), la largeur au texte du prompteur.

```
 namfoop browse  1 Bibliothèque  2 Chaîne  [3 Set] Groupe 1 · Live Mairie        ♩ 124 4/4 (du morceau)  groupe-1 ✓
┌ ♪ Groupe 1 · Live Mairie · 2/6 ──────────┐┌ 2 · Le Phare · son 1/2 Fuzz Face · PgDn : ensuite Clean SVT ──────┐
│   1 Ouverture             ♩96            ││  ██1██   2      3      4      ♩124 4/4   mes. 3/4 INTRO   ⏵        │
│ ▶ 2 Le Phare              ♩124     ≡     ││  COUPLET 1 dans 2 mes.                                             │
│      ● Fuzz Face                         ││ ────────────────────────────────────────────────────────────────── │
│      ○ Clean SVT                         ││ ▸ INTRO       4 mes.  → Fuzz Face                                  │
│   3 Néon                  ♩100     ≡     ││   ›  basse seule, 4 mesures, fuzz plein pot                        │
│   4 Marée basse           ♩82  6/8 ≡     ││      | E5 . . . | E5 . . . | G5 . E5 . | D5 C5 B4 . |             │
│      ○ Clean SVT    CC69:0               ││   COUPLET 1   8 mes.                                               │
│      ○ Drive B7K    CC69:1               ││      | E5 . . E5 | G5 . E5 . | D5 . C5 . | B4 . . . |  ×2         │
│      PC 4 · prompteur : 3 sections       ││   REFRAIN     8 mes.  → Clean SVT                                  │
│   5 Sur le fil            ♩117           ││   COUPLET 2   8 mes.  → Fuzz Face                                  │
│   6 Rappel                ♩110           ││   FIN         2 mes.                                               │
│                                          ││ ── ensuite (→) : 3 · Néon  ♩100  Synth octave                      │
│                                          ││     INTRO        | Am . . . | Am . . . | F . . . | G . . . |       │
│                                          ││  Espace battue (appui = le 1) · Fin/Début section · t éditer       │
└──────────────────────────────────────────┘└────────────────────────────────────────────────────────────────────┘
 ♪ set : 2 Le Phare › son 1 Fuzz Face · gate -55 › ◆[ direct | nam·Fuzz Face › ir·4x10 ] › peak
 PgDn/PgUp son   →/← morceau   ↑↓ Entrée jouer un morceau   i détail   Espace battue   Fin/Début section   L sets   e éditer   t prompteur   * tap   1 2 espaces   q quitter
```

- **Couleur d'accent : vert** (Bibliothèque magenta, Chaîne cyan). `3` y va depuis n'importe où, aux
  mêmes conditions que `1` / `2`.
- En-tête : le set actif, le **tempo entendu** et sa source (`du morceau`, `du preset`, `tap`,
  `horloge`), l'état d'enregistrement du **set** (`set : <fichier> ✓` / `● set non enregistré (S)`).
- **Colonne set list** (~44 colonnes) : une ligne par morceau, l'essentiel seulement — n°, titre,
  tempo (`♩---` s'il suit les presets), la mesure si elle n'est pas 4/4, `≡` s'il a un prompteur. Le morceau joué porte `▶` (en gras)
  et **déplie ses sons** sous lui, un par ligne : `●` en inverse vert pour le son joué, `○` pour les
  autres. La liste commence **un morceau avant** celui qu'on joue : tout le reste de la hauteur montre
  ce qui vient ; `… N lignes plus bas` quand elle déborde. Le curseur ↑↓ est surligné.
- **`i` (détail)** déplie ou replie le morceau visé (le morceau joué dans `run --set`) : son n° de PC,
  ses sons avec leur valeur de CC 69 et leur volume propre, et la présence d'un prompteur. Plusieurs morceaux peuvent rester dépliés.
- **Colonne prompteur** (le reste de la largeur) : en tête, la **bande de battue** (§ 11) ; titre `n · morceau · son k/N <preset> · PgDn :
  ensuite <…>` — ce que fera le prochain appui —, puis le texte à partir de deux lignes avant la
  position (§ 10). Les sections passées sont grisées, la courante `▸` en vert, les suivantes en gras.
  Au bout du texte, **le morceau suivant** : titre, tempo, sons et ses premières sections, en gris.
- Ligne d'écoute : morceau › son › la chaîne en une ligne, et le volume du morceau s'il y en a un.
- **Set sans titres** (le show à trois sons) : la colonne de gauche liste directement les sons.
- **Terminal étroit** : sous 100 colonnes, la set list se réduit à 30 colonnes (titres tronqués) ;
  sous 70, elle disparaît au profit d'une ligne `2/6 Le Phare ● Fuzz Face → 3 Néon` au-dessus du
  prompteur, qui prend toute la largeur.
- **En édition** (§ 8), la set list prend toute la largeur (PC, sons et volumes de tous les morceaux)
  et le prompteur est masqué : on n'édite pas sur scène.

## 7. Choisir un set (`L`)

```
┌ Choisir le set ───────────────────────────────────────────────────────┐
│ ▶ Groupe 1 · Live Mairie   6 morceaux · 9 sons · sets/groupe-1.set.toml │
│   Trio acoustique          — morceaux · 3 sons · sets/trio.set.toml     │
│   aucun set                la banque entière, comme aujourd'hui         │
│   ＋ nouveau set…          un set vide, à remplir en édition            │
└─────────────────────────────────────────────────────────────────────────┘
```

Entrée active le set : sa banque se précharge (§ 4), le premier son du premier morceau joue. Si le
set en cours n'est pas enregistré, la fenêtre « non enregistré » habituelle passe d'abord.

## 8. Éditer un set (`e`)

Le cadre passe en **jaune** `✎ ÉDITION — …`. ↑↓ visent un morceau, ←→ un son du morceau (cadre jaune
autour). **Viser n'écoute pas** : seul Entrée (remplacer) ou `+` ouvrent une liste où ↑↓ essaient un
preset (comme le sélecteur de la Chaîne, § 5 de `browse-espaces.md`).

| Touche | Morceau visé | Son visé |
|---|---|---|
| `a` | ajouter un morceau après (saisie du titre) | |
| `r` | renommer | |
| `T` | tempo (vide = celui des presets) | |
| `p` | n° de PC (le premier libre est proposé à la création ; refusé s'il est pris par un autre morceau) | |
| `X` | retirer (confirmation) | |
| `[` `]` | déplacer dans le set | |
| `+` | | ajouter un son après (liste des presets, ↑↓ écoute) |
| Entrée | | remplacer le preset (même liste) |
| `x` | | retirer (sans confirmation, le preset reste dans la banque) |
| `{` `}` | | déplacer dans le morceau (change sa valeur de CC 69) |
| `,` `.` | | volume propre au morceau, ±0,5 dB |
| `t` | ouvrir le prompteur du morceau dans `$EDITOR` (§ 10) | |

`S` enregistre le set, Échap ou `e` sort de l'édition (le `●` reste tant que ce n'est pas enregistré),
`q` passe par la fenêtre « non enregistré » si besoin. Une saisie (titre, tempo, PC) passe avant toutes
les touches, comme le filtre et le nom aujourd'hui (keys.rs, #175).

**Modifier un son, c'est modifier son preset** : depuis l'espace 3, `2` mène à la Chaîne du son joué ;
le titre de la Chaîne rappelle `preset utilisé par N morceaux de ce set` et `S` y enregistre le
preset, donc partout.

## 9. `namfoop run --set <id | mots-clés>` : la scène verrouillée

- Le même écran que l'espace 3, en-tête `namfoop run  🔒 SET  <nom>`, **sans** édition, choix de set,
  espaces, ni `t` : toute autre touche affiche `🔒 run --set : rien ne s'édite ici`.
- Touches : `PgDn` / `PgUp`, morceau suivant / précédent, défilement et sections du prompteur, `i`, `*`.
  `q` ouvre `Quitter la scène ? Q quitter · Échap rester`.
- `run --set` sans argument prend `[set] default` de la config : c'est ce que lancera le service de
  scène du Pi (#46).
- `run --preset` et `run --set` s'excluent.

Configuration :

```toml
[set]
default = "groupe-1"     # run --set sans argument, démarrage automatique
next_song = "Right"      # touches réglables (noms crossterm), pour coller au pédalier
prev_song = "Left"
prompt_play = "Space"
prompt_next = "End"
prompt_prev = "Home"
```

## 10. Le prompteur (ChordPro)

Le texte d'un morceau est un fichier **ChordPro** (`library/sets/prompts/<morceau>.cho`), le format
de fait des logiciels de set list. namfoop n'en est **pas l'éditeur** : `t` ouvre `$EDITOR`, et à
terme d'autres logiciels produiront ces fichiers. namfoop lit un sous-ensemble et ignore le reste
sans erreur :

| ChordPro | Rendu |
|---|---|
| `{title}`, `{duration}` | métadonnées |
| `{tempo: 92}`, `{time: 7/8}` | ChordPro les permet plusieurs fois, chacune valant à partir de là : la **carte des mesures** (§ 11) ; le premier sert si le `.set.toml` ne donne rien |
| `{start_of_verse: Couplet 1}` … `{end_of_verse}` (et `chorus`, `bridge`, sections libres, formes courtes `sov` / `soc`) | une **section** étiquetée |
| `[Am]paroles` | accords en ligne, en couleur, au-dessus des paroles quand la largeur le permet |
| `{comment: …}` / `{c: …}` | une cue, en jaune |
| `{x_namfoop_sound: clean-svt}` (dans une section) | **lie la section à un son** : passer sur ce son y amène le prompteur |
| `{x_namfoop_bars: 8}` (dans une section) | durée de la section en mesures (de la mesure en cours) |
| `{x_namfoop_beats: 2+2+3}` (après un `{time}`) | regroupement des temps d'une mesure composée ou impaire |

Les directives propres à namfoop suivent la convention `x_` de ChordPro (les autres logiciels les
ignorent).

Défilement : il suit la **position** de la battue (§ 11). En marche, chaque section dure ses
mesures, et ses lignes se partagent ces mesures. **À l'arrêt, la position suit le son** : changer de
son amène à la prochaine section liée à ce son. **En marche, elle suit le temps** : changer de son
ne déplace rien (sinon un changement de son un temps trop tôt décalerait tout). Sans
`x_namfoop_bars`, `{duration}` est réparti sur les lignes ; sans l'un ni l'autre, pas de défilement
automatique. La ligne courante est marquée `›` et surlignée, la section courante `▸` en vert.

## 11. Battue et métrique

**État de l'art.** Les pédaliers gèrent un tempo (global, par preset, par snapshot chez Helix), le
tap et l'horloge MIDI, jamais la mesure ; c'est le terrain des logiciels de scène (MainStage :
signature par patch). Et l'horloge MIDI ne transmet pas la mesure : le Song Position Pointer compte
des doubles croches depuis le début, MIDI Start annonce le premier temps. C'est donc **namfoop qui
tient la carte des mesures**, tirée du prompteur.

**Carte des mesures** d'un morceau : la mesure (`time` du `.set.toml`, 4/4 par défaut) et le tempo
(`tempo_bpm`, sinon celui du preset), puis, section par section, les `{time}` / `{tempo}` /
`{x_namfoop_beats}` du `.cho` et la durée `{x_namfoop_bars}`. Le **BPM se compte toujours à la
noire**, comme Live et l'horloge (24 tops par noire) : en 6/8 à ♩ = 82, on bat 2 noires pointées.
Le tempo de la section en cours est le **tempo entendu** (delays), au même rang que celui du morceau
dans l'arbitre (#200) : un tap ou l'horloge le recouvrent.

**La bande de battue**, en tête de la colonne prompteur :

```
  ██1██   2      3      4      ♩124 4/4   mes. 8/8 COUPLET 1   ⏵
  ▌→ CLEAN SVT▐  ▌4▐ · 3 · 2 · 1   REFRAIN
```

| Mesure | Bande |
|---|---|
| 4/4 | `██1██   2      3      4` |
| 6/8 | `██1██ ·   ·   2   ·   ·` (2 pulsations, croches en points) |
| 7/8 (2+2+3) | `██1██ ·   2   ·   3   ·   ·` |

- le **temps 1 en jaune**, les autres en vert, les subdivisions en points gris ; à l'arrêt, rien ne
  s'allume et la bande dit `⏸ Espace` ;
- à droite : tempo et mesure, `mes. n/N SECTION` ;
- 2e ligne : `SECTION SUIVANTE dans N mes.`, et l'annonce d'un changement de mesure ou de tempo
  (`passe en 4/4  ♩92`) ;
- **compte à rebours de changement de son** : si la section suivante est liée à un autre son que
  celui joué, `→ Clean SVT dans N mes.` (jaune), puis sur la dernière mesure `→ CLEAN SVT` et le
  décompte des temps `4 · 3 · 2 · 1` ;
- un morceau sans prompteur a sa bande quand même (tempo et mesure du morceau, mesures comptées).

**Calage du temps 1** :

| Source | Comment |
|---|---|
| Pied, `Espace` | lance / arrête ; l'appui est le 1 de la mesure en cours |
| `Fin` / `Début` | section suivante / précédente, recalée sur son 1 (en marche comme à l'arrêt) |
| Tap `*` | recale le tempo et le temps, pas la mesure |
| Horloge MIDI (US 7) | Start = le 1 de la mesure 1 du morceau ; SPP = position, convertie en mesure et temps par la carte des mesures ; Stop arrête |

Changer de morceau remet la position au début du morceau (et arrête la battue, sauf horloge).

**Temps réel** : la bande est dessinée par le fil de l'interface à partir de l'arbitre de tempo et
de la position ; rien dans le fil audio. Un écart visuel de 10-30 ms est invisible ; à mesurer sur le
Pi (rafraîchissement ratatui). Un **clic audio** dans les in-ears serait possible plus tard (hors
épique : sur scène, le clic vient en général d'Ableton).

## 12. MIDI (avec #7)

- **PC / CC 69** : § 5 (PC = morceau, CC 69 = son du morceau, fenêtre PC + CC). Sans set : PC n =
  n-ième preset de la banque (à confirmer dans #7).
- **Horloge** : Start lance la battue sur le 1 de la mesure 1, Stop l'arrête, le tempo reçu devient
  le tempo entendu (#200).
- **Song Position Pointer** (Ableton l'envoie en mode « Song ») : position en doubles croches,
  convertie en section / mesure / temps par la carte des mesures (§ 11).
- Plus tard, hors épique : CC32 pour choisir le set (comme les setlists Helix), CC « son / morceau
  suivant / précédent » pour les pédaliers MIDI simples, OSC (AbleSet publie le morceau et la section
  en cours), CC de slot à la Kemper.

## 13. Ce qui change ailleurs, ce qui ne change pas

- **Les espaces restent indépendants** (règle générale du § 2 de `browse-espaces.md`) : choisir un
  espace, c'est travailler sur ce qu'il représente. Un set actif ne change donc **rien** au clavier
  des autres espaces.
  - dans la **Chaîne**, `PgDn` / `PgUp` parcourent la **banque**, en boucle, set actif ou non : on y
    travaille son preset ;
  - le set ne se joue que dans **son** espace et sur scène (`run --set`).

  > Corrigé le 16/11/2026 (#247) : cette section demandait l'inverse (« `PgDn` / `PgUp` dans la
  > Chaîne suivent le set actif »), et c'était faux — on ne pouvait plus travailler à la maille
  > preset dans l'espace fait pour ça. La banque **préchargée** reste celle du set (§ 4) : c'est en
  > Set et sur scène que la bascule doit être instantanée ; dans la Chaîne, un preset hors du set
  > se charge par l'ancien chemin (~80 ms, sans coupure).
- La Bibliothèque est inchangée (ses `PgDn` / `PgUp` déplacent la liste de 10).
- Sans set actif : `browse`, `run --preset`, la banque, `P`, `S`, `n` sont **exactement** ceux
  d'aujourd'hui (tests de non-régression de #110 et #131).

## 14. Découpage (épique et US)

| US | Issue | Contenu | Crates |
|---|---|---|---|
| 1 | #217 | Format `.set.toml` (dont `pc` de morceau et `time`), lecture, contrôles du § 3, `Kind::Set` dans la bibliothèque, commande `namfoop sets` | library, namfoop |
| 2 | #218 | Banque du set (§ 4) et navigation (§ 5 sans MIDI) : préchargement des seuls presets du set, volume et tempo du morceau, liste à plat sans bouclage, touches de morceau réglables (`[set]`) | namfoop, model |
| 3 | #219 | Espace 3, vue scène en deux colonnes (§ 6, dont `i` et le terminal étroit) et choix du set `L` (§ 7), set actif retenu | namfoop |
| 4 | #220 | Édition à l'écran (§ 8) et enregistrement du set | namfoop |
| 5 | #221 | `run --set` verrouillé et `[set]` dans la config (§ 9) | model, namfoop |
| 6a | #222 | Prompteur ChordPro (§ 10) : lecture du sous-ensemble, rendu, suivi du son, sections au pied, `t` | library, namfoop |
| 6b | #223 | Carte des mesures et bande de battue (§ 11) : `{time}` / `{tempo}` successifs, `x_namfoop_bars` / `x_namfoop_beats`, `Espace`, compte à rebours, défilement au tempo, tempo de section dans l'arbitre | library, namfoop |
| 7 | #224 | MIDI du Set (§ 5, § 12), une fois #7 livré : PC morceau, CC 69 son, fenêtre PC + CC, Start/Stop, SPP sur la carte des mesures | engine, namfoop |

Ordre : 1 → 5 (le set se joue au pied), puis 6a, 6b, puis 7. Chaque US garde le cycle habituel (issue,
branche, PR, CI, test matériel quand l'audio est touché : US 2, 5, 7). Décision du 15/11/2026 : pour
cette épique, les essais matériels se font **sur ALSA seulement** (JACK et sim restent couverts par
les tests automatiques).

## 15. Décisions et points à valider

Décidé le 15/11/2026 :

1. vue scène en deux colonnes, set list compacte à gauche, `i` pour le détail (§ 6) ;
2. MIDI à la Helix : PC = morceau (unique dans le set), CC 69 = son du morceau (§ 5) ;
3. métrique dans le ChordPro, BPM toujours à la noire, bande de battue avec temps 1 et compte à
   rebours de changement de son (§ 11).

À confirmer sur la maquette :

1. pas de bouclage dans un set (§ 5) ;
2. le tempo est réglé par morceau (puis par section), le volume son par son (§ 3) ;
3. `S` dans la Chaîne modifie le preset partout où il sert, le set le rappelle (§ 8) ;
4. en défilement la position suit le temps, à l'arrêt elle suit le son (§ 10).
