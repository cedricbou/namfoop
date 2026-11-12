# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/). Chaque fonctionnalité ou
correction visible par l'utilisateur ajoute une ligne dans « Non publié » ; à la publication, la
section prend le numéro de version.

## Non publié

### Ajouté (#159)
- **`namfoop presets`** liste la banque enregistrée (identifiant court, nom, la chaîne en une
  ligne, parallèles compris), comme `namfoop models` pour les sons.
- **`namfoop run --preset <id | mots-clés>`** (et `render --preset`) joue un preset sans passer par
  `browse` : c'est ce qui permet de démarrer sur un son ou de le rejouer dans un script. La
  résolution est celle de `-m` / `-i` (identifiant, nom, mots-clés, ambiguïté listée).
- Un preset décrit toute la chaîne : le mélanger à `-m`, `-i`, `--pre`, `--post` ou `--crossover`
  est refusé avec un message qui dit quoi faire. Le cadre (trim, niveau, master, limiteur,
  `--input-db`) reste celui de la config et des options.
- Vérifié : `render --preset` et les options équivalentes donnent un WAV **identique au bit près**.

### Ajouté (#158)
- `browse`, espace Chaîne : **PgDn / PgUp passent au preset suivant / précédent**, dans l'ordre de
  la banque et en boucle — le geste du pédalier, pour essayer ou répéter sans passer par la banque.
  L'en-tête donne la place (`preset 2/7 : Chill verse ✓`), et une chaîne non enregistrée fait
  demander une fois (`S` enregistrer puis charger, Entrée charger quand même, Échap rester). Dans
  la Bibliothèque, PgUp / PgDn continuent de défiler les listes.
- Mesuré (x86, en pseudo-terminal, de la touche à la chaîne installée) : **81 ms** pour `nam › ir`,
  **204 ms** pour un preset à deux modèles en parallèle — c'est le chargement des modèles qui
  domine. Le son ne se coupe pas (la chaîne précédente joue jusqu'au fondu de 20 ms) : c'est un
  retard, pas un trou. Pour changer de son sur un temps, il faudra la banque préchargée de #6.

### Ajouté (#110)
- `browse` : **enregistrer et retrouver ses chaînes**. **`S`** écrit la chaîne en preset dans
  `library/presets/` (nom proposé, modifiable), **`P`** ouvre la **banque** — la liste des presets
  avec leur identifiant court, leur chaîne en une ligne, le filtre `/` et Entrée pour en charger un.
  L'en-tête affiche `preset : <nom> ✓` ou `● non enregistré (S)` dès que la chaîne diffère de ce qui
  est enregistré (comparaison d'états : défaire le changement efface l'alerte).
- Charger un preset avec une chaîne non enregistrée demande **une fois** (`S` enregistrer puis
  charger, Entrée charger quand même, Échap rester) ; la fenêtre de `q` gagne le même `S`
  (enregistrer puis quitter).
- **`t`** déplie le TOML du preset sous le synoptique (replié par défaut).
- Un preset dont un son manque reste listé, en rouge, avec la raison ; s'il a seulement été
  déplacé, l'identifiant court enregistré le retrouve. Épique #103, format : #157.

### Ajouté (#157)
- **Format de preset** : une chaîne enregistrée vit dans `library/presets/<nom>.preset.toml` et est
  **indexée comme les mélanges d'IR** — identifiant court `p…`, recherche par mots-clés. Le fichier
  décrit les blocs dans l'ordre, les parallèles (mode, coupures) et leurs branches (nom, gain,
  polarité, muet ; le solo n'est jamais enregistré), les blocs coupés compris.
- Les effets y sont écrits **au complet** (tous leurs paramètres) et non dans la syntaxe compacte de
  la ligne de commande : un preset doit sonner pareil dans un an, même si une valeur par défaut
  change entre deux versions. Les modèles et les IR sont cités par **chemin relatif** à la
  bibliothèque, avec leur **identifiant court en filet** : si un fichier a bougé, le son est
  retrouvé par son identifiant et un avertissement le dit ; sinon le message nomme le preset, le
  fichier et l'identifiant pour pouvoir réparer le TOML à la main.
- Le § 9 de `docs/design/browse-chaine.md` (format « à fixer dans #6 ») est tranché en conséquence.
  Refs #6 ; débloque #110 (enregistrer et ouvrir un preset depuis `browse`).

### Ajouté (#152)
- `browse`, Chaîne : la **sortie d'un parallèle** devient une case du curseur, dessinée `◇` à son
  bord droit (`IN ─ ◆─┬─ … ─┬─ ◇ ─ OUT`). **`a` y ajoute après le parallèle** : c'est la façon
  directe de poser un bloc derrière un parallèle en bout de chaîne, là où il fallait le mettre dans
  une branche puis l'en sortir avec `]`. `i` ajoute à la fin de la première branche, `↑` remonte au
  `◆` (d'où le parallèle se règle, se déplace et se défait), et `→` depuis le bout d'une branche —
  ou `←` depuis le bloc qui suit — y amène. Le panneau du bas montre ce qu'elle somme (branches,
  mode, gains). Inspiré du marché, où le point de fusion est un objet sélectionnable (bloc **Merge**
  de Helix, **Mixer** de Quad Cortex et Fractal) ; ses réglages propres (niveau, mix) viendront
  (#153).

### Ajouté (#150)
- `browse` : **`h` (ou `?`) ouvre l'aide des touches**, contextuelle — Bibliothèque (selon
  l'onglet), Chaîne (synoptique ou réglages), sélecteur d'un bloc, catalogue, fenêtre « mettre dans
  la chaîne », mode pose, éditeur `e`. N'importe quelle touche la referme ; pendant une saisie, `h`
  s'écrit. Elle se dessine par-dessus tout l'écran et tient en 80×25.

### Corrigé (#150)
- `browse`, Chaîne : la barre d'aide du bas **avait perdu les touches qui permettent d'en sortir**
  (`q` quitter, `1` Bibliothèque, `v` vumètres) et annonçait encore « Tab onglets » sur une chaîne
  vide. Chaque barre finit maintenant par `v vumètres   r sans effets   h aide   1 Bibliothèque
  q quitter`. Elle est aussi coupée **aux séparateurs de touches** et non au milieu d'un mot : sur
  un terminal étroit, les touches du milieu sautent (un `…` le signale) et les trois dernières
  restent lisibles. Deux textes contenaient un trou de 16 espaces (continuation de littéral
  recollée par `cargo fmt`).

### Corrigé (#148)
- `browse`, Chaîne : **la fenêtre du catalogue (`a` / `i`) ne s'affichait pas** quand le curseur
  était sur un bloc `nam` ou `ir` — le panneau du bloc sortait du rendu avant elle. Le catalogue
  était bien ouvert : Entrée ajoutait l'entrée surlignée (`nam`), d'où l'impression que `a` ne
  faisait rien puis ajoutait un modèle en double. Il se dessine maintenant **toujours**, par-dessus
  le reste, quel que soit le bloc sous le curseur.
- `browse`, sélecteur d'un bloc `nam` / `ir` : **plus aucune touche muette**. Les touches qu'il ne
  traite pas (`a`, `i`, `x`, `s`…) répondent `valide (Entrée) ou annule (Échap) le choix d'abord`,
  et `e` dit où se trouve la calibration (la Bibliothèque) au lieu de ne rien faire.

### Ajouté (#137)
- `browse` : **`q` signale une chaîne vide alors que le banc joue un son**, même s'il n'y a rien
  d'autre à enregistrer — la commande affichée en quittant ne jouerait pas ce son. La fenêtre
  propose les trois issues : **Entrée** le met dans la chaîne et on reste (`2` pour la voir), `Q`
  quitte quand même, Échap reste. Banc sur « son direct » sans IR : rien à signaler.
- Entrée sur une **chaîne vide** installe désormais **tout ce qu'on entend au banc** depuis
  n'importe quel onglet (`nam › ir`), au lieu de l'IR seule depuis l'onglet IR : le parcours nominal
  « je choisis un modèle, puis son baffle, Entrée » perdait le modèle. Spécification § 4 corrigée
  (décision du 12/11/2026).
- **Corrigé** : la commande affichée en quittant (et le cadre « à copier dans namfoop run »)
  décrivait le modèle et l'IR du **banc d'essai** avec les effets de la **chaîne**. Depuis la
  séparation des deux espaces (#134), le banc peut jouer autre chose : la commande décrit maintenant
  la chaîne, c'est-à-dire ce qu'on rejoue.
- Nettoyage : plus aucune mention de « l'onglet Effets » ni du bloc « visé » dans les aides, les
  messages, le guide et les spécifications ; `docs/design/browse-chaine.md` § 10 renvoie à
  `browse-espaces.md`. Le guide décrit les deux espaces partout, parcours nominal compris.
  Ferme #125 (épique #131, spécification § 8, § 9 et § 10).

### Changé (#138)
- `browse` : le **panneau Spectre s'affiche à partir de 128 colonnes** (au lieu de 130). L'écran
  prévu sur le Pi 5 (10,1 pouces, 1280×800) donne 128×40 en police 10×20, celle que l'utilisateur
  pense retenir : il manquait deux colonnes. Vérifié avec `TestBackend` qu'à 128 la liste et le
  panneau restent aussi lisibles qu'à 130 (libellés, packs, axes, légende).

### Ajouté (#136)
- `browse`, sélecteur d'un bloc `nam` / `ir` : **`c` bascule l'écoute entre l'actuel et l'essai**
  sans bouger le curseur — l'A/B dans le contexte de la chaîne, avec ses effets. La ligne porte
  `● actuel ◀ écouté (c)`, la ligne d'écoute dit `tu réentends l'actuel : c pour revenir à l'essai`,
  et Entrée pendant qu'on écoute l'actuel garde l'actuel. Sans essai en cours, `c` explique quoi
  faire ; sur un bloc tout juste posé par le catalogue (pas d'actuel), il ne fait rien. En mode
  « match », le volume est réaligné à chaque bascule, comme pour `r`.
- Le **panneau Spectre** s'ouvre aussi dans le sélecteur (même seuil de largeur) : courbe de la
  ligne sous le curseur, **actuel** en référence (`●`), écarts par zone, ligne `grain`.
  Épique #131, spécification `docs/design/browse-espaces.md` § 5.

### Ajouté (#135)
- `browse`, Bibliothèque : **Entrée met le son du banc dans la chaîne**. Chaîne vide, elle devient
  `nam › ir` ; sinon une **fenêtre** demande quel bloc remplacer (nom complet du modèle en place et
  sa branche, dans l'ordre du synoptique) ou d'**ajouter** un bloc en choisissant l'endroit. La case
  **« avec son IR »** (cochée par défaut, seulement pour un modèle qui a besoin d'une IR quand le
  banc en joue une) pose l'IR du banc dans le bloc `ir` voisin, ou l'ajoute juste après. Remplacer
  garde la Bibliothèque ; ajouter passe dans la Chaîne en **mode pose** (`♪ CHAÎNE · où poser … ?`,
  Entrée pose après le curseur, Échap annule). Un mélange pas encore enregistré l'est d'abord.
  Fini la création implicite d'un bloc « à la fin de la chaîne » (#125).
  Épique #131, spécification `docs/design/browse-espaces.md` § 4.

### Changé (#134)
- `browse` est désormais en **deux espaces** au lieu de quatre onglets : la **Bibliothèque** (`1` :
  Modèles, IR, Mélange) et la **Chaîne** (`2` : l'ancien onglet Effets). **Ce qu'on entend suit
  l'espace** : dans la Bibliothèque, le **banc d'essai** `basse › modèle › IR` **sans aucun bloc de
  la chaîne**, pas même le gate ; dans la Chaîne, la chaîne éditée. Le fondu habituel au passage.
- **Parcourir n'écrit plus jamais dans la chaîne** (le constat de #125 : « quand je reviens sur cet
  écran et que je change, ça change mes effets ») : ↑↓, l'IR conseillée posée d'office, `c`, `r`,
  `b`, l'éditeur `e` et le Mélange n'agissent plus que sur le banc. Écrire est un geste explicite :
  **Entrée** met le son du banc dans la chaîne (chaîne vide : elle devient `nam › ir`).
- L'en-tête montre les deux espaces et le nombre de blocs (`2 Chaîne ● 5 blocs`), le cadre porte un
  bandeau (`♪ BANC D'ESSAI · … · sans effets, la chaîne n'est pas touchée` / `♪ CHAÎNE — NAM
  actifs : …`), et une **ligne d'écoute** dit en clair ce qui sort (`♪ banc : …` avec la chaîne
  rappelée en gris, ou `♪ chaîne : …`). Dans les listes, `◀ écouté` / `◀ écoutée` remplace
  `◀ en cours`, et un `●` vert rappelle ce que la chaîne utilise.
- Tab, Maj+Tab et → / ← restent dans la Bibliothèque (→ sur Mélange : `→ ne va pas plus loin : 2
  pour la Chaîne`). Dans la Chaîne, Tab garde son rôle (les réglages).
  Épique #131, spécification `docs/design/browse-espaces.md` § 2, § 3, § 4 et § 6.

### Ajouté (#133)
- `browse`, onglet Effets : **Entrée sur un bloc `nam` ou `ir` ouvre la liste de la bibliothèque
  sur place**, dans le panneau sous le synoptique (cadre jaune), au lieu d'envoyer dans l'onglet
  Modèles ou IR. ↑↓ **essaie le son dans la chaîne, avec ses effets** (le synoptique montre
  l'essai, la liste marque `● actuel` et `◀ à l'essai`), Entrée garde, Échap remet exactement
  l'ancien. `o` n'écoute que ce bloc et son baffle voisin, `r` coupe les effets, `/` filtre. Un
  essai n'est **pas** une modification : `q` ne demande rien de plus, et il est refusé tant qu'un
  choix est en cours. Le panneau montre le **nom complet** du modèle (variante, pack, identifiant,
  badge IR, mini-spectre, grain), que l'étiquette du synoptique tronque. Choisir `nam` ou `ir` dans
  le catalogue pose `nam·(à choisir)` et ouvre aussitôt la liste (Échap retire le bloc). Les
  mélanges enregistrés sont en tête de la liste des IR, les conseillées marquées `★`.
  Épique #131, spécification `docs/design/browse-espaces.md` § 5.

### Corrigé (#70)
- `namfoop latency` mesure à n'importe quel niveau de retour. Le retour était cherché avec un
  **seuil fixe** de 0,05 (-26 dBFS) : à travers PipeWire, l'étage de volume atténue et **étale**
  l'impulsion d'un échantillon, et avec la sortie iTwo à 85 % (-4,2 dB) la commande répondait
  « aucune mesure » alors qu'à 100 % elle donnait 7,7 ms. Le retour est maintenant le **maximum**
  du signal dans les 250 ms qui suivent l'impulsion, comparé au **bruit de fond** mesuré juste
  avant (critère relatif, ≈ +14 dB) : mesuré jusqu'à -60 dBFS dans les tests, là où l'ancien seuil
  ne voyait plus rien. Chaque mesure affiche le niveau du retour et le bruit de fond, et quand rien
  ne sort du bruit le message dit quoi vérifier (câble, bouton Mixer, volume de la sortie) et
  propose `--backend alsa`.

### Interne (#114, #117)
- CI : les dépendances système s'installent en 3 tentatives (`.github/actions/deps`) — un miroir
  Ubuntu injoignable ne fait plus échouer toute la PR —, et « Statistiques sccache » ne masque plus
  la vraie cause d'un échec.
- Le test `sim_session_swaps_chains` n'exige plus 0 xrun dans l'absolu : la carte simulée en compte
  un dès que son thread a une période de retard, ce qui mesure la machine et pas le code. Il prend
  une référence sans échange de chaîne : si le flux décroche déjà là, il ne conclut pas ; sinon, un
  échange ne doit ajouter aucun décrochage. Le 0 xrun strict reste sur le matériel.

### Mesuré (#111)
- Coût de plusieurs modèles NAM en parallèle, sur x86 : **linéaire** en nombre de modèles (le
  parallèle lui-même ne coûte que 0,06 à 3 µs). Par bloc de 64, moyenne / pire cas : le b7k du
  bassiste 45 µs (3,4 %) / 138 µs seul, 184 µs (14 %) / 256 µs à quatre ; le modèle le plus lourd de
  la bibliothèque 104 µs (7,8 %) / 248 µs seul, 417 µs (31 %) / 509 µs à quatre. Bloc 48 : même coût
  absolu, part un peu plus grande. Mesure reproductible (`cargo test --release -p dsp --test chain
  -- --nocapture cost_of_several_models`, `NAMFOOP_BENCH_MODEL` pour un modèle de la bibliothèque) et
  notée dans les faits mesurés. Seuil retenu : `browse` **alerte dès 2 modèles actifs** et ne refuse
  rien. **Le Pi 5 reste à mesurer** (#44 / #45) : lui seul dira s'il faut une limite dure.

### Changé (#109)
- `browse`, onglet Effets : **le synoptique est l'éditeur**. La colonne du catalogue devient une
  fenêtre (`a` ajoute après le curseur, `i` avant ; on tape pour filtrer, les noms qui commencent
  par le filtre d'abord) et le catalogue contient maintenant **`nam` et `ir`** en plus des effets.
  Navigation **2D** : ←→ le long de la chaîne (en entrant dans un parallèle et en en sortant), ↑↓
  d'une branche à l'autre ; le curseur se pose sur un bloc, sur le `◆` d'un parallèle ou sur
  l'en-tête d'une branche.
- Construire un parallèle : **`s`** met un bloc en parallèle — sur un `nam` suivi de son `ir`, les
  deux partent ensemble dans un **crossover à 150 Hz** (grave = son direct), soit le blend grave
  clair en une touche ; **`+`** ajoute une branche (4 au maximum, et une coupure en bandes) ;
  **`x`** retire un bloc, une branche, ou défait le parallèle en gardant la branche de l'ampli ;
  **`[` / `]`** déplacent un bloc, le font entrer dans un parallèle ou en sortir ; **`{` / `}`** le
  passent d'une branche à l'autre ; **`b`** coupe un bloc ; **`m` / `o` / `p`** règlent le muet, le
  solo et la polarité d'une branche (le solo se traduit en muets à la construction et ne
  s'enregistre jamais). Chaque message dit ce qui s'est passé, ou pourquoi c'est refusé.
- **Entrée** ouvre les réglages de ce qui est sous le curseur : les paramètres d'un effet, le
  **mode** (copie / bandes) et les **coupures** d'un parallèle (pas multiplicatif ×1,06, ×1,01 en
  pas fin, écart minimal ×1,25 entre voisines), le **gain**, la **phase**, le **muet** et le
  **solo** d'une branche. Sur un bloc `nam` / `ir`, Entrée ouvre l'onglet Modèles (ou IR), qui agit
  alors sur **ce** bloc : plusieurs modèles dans une chaîne se choisissent donc chacun chez eux.
- Le panneau du bas donne la ligne de commande tant que la chaîne s'exprime en options ; dès qu'il
  y a un parallèle (ou plus d'un modèle), il affiche « chaîne à enregistrer en preset (S) ».
  L'en-tête de l'onglet compte les blocs de la chaîne.

### Changé (#108)
- `browse`, onglet Effets : la liste « pré / [modèle › IR] / post » devient le **synoptique** de la
  chaîne, `IN ─ gate -60 ─ nam·b7k ─ ir·sm57 ─ delay ─ OUT`, avec un **vumètre au-dessus de chaque
  bloc** (crête -60 … 0 dBFS en huitièmes de caractère ; vert, jaune au-dessus de -9 dB, rouge
  au-dessus de -1 ; montée immédiate, descente 28 dB/s). Le compresseur montre sa réduction de gain
  en orange depuis la droite (20 dB = toute la barre), le gate fermé une barre pleine rouge sombre ;
  un bloc coupé, une branche muette ou hors solo sont assombris. **`v`** masque et réaffiche les
  vumètres. Le titre du cadre compte les modèles actifs et les parallèles, et alerte sur la charge
  au-delà de 2 modèles (seuil provisoire, à fixer par la mesure sur Pi 5).
- Le synoptique sait déjà dessiner les **parallèles** (crossover à 2 bandes, copie à 3 branches,
  bandes à 4) : une ligne par branche, `◆─┬─` / `├─` / `└─` à l'entrée, `┬` / `┤` / `┘` à la sortie,
  étiquette de branche, remplissage `─` et gain aligné à droite (`-3 dB`, `ø` si la polarité est
  inversée, `S` en solo, `muet`) — toutes les lignes d'un parallèle ont la même largeur et les
  traits verticaux tombent dans la même colonne. Il reste en **lecture seule** : l'édition est celle
  d'avant (catalogue, `a` / `A`, `[` `]`, `x`, `b`), l'éditeur 2D vient ensuite.
- Terminal étroit : défilement horizontal qui garde le bloc choisi visible, sans retour à la ligne.

### Ajouté (#107)
- `dsp::meter` : **vumètres par bloc et par branche**, préalloués à la construction de la chaîne
  (`ChainSpec::build_with_meters`) et qui voyagent avec elle. Pour chaque bloc, la crête de sortie ;
  pour `comp`, sa réduction de gain (dB) ; pour `gate`, sa fermeture (0 ouvert … 1 fermé) ; pour
  chaque branche, ce qu'elle envoie à la somme (gain et polarité compris, donc zéro si elle est
  muette). Même modèle que le vumètre du limiteur : un `f32` dans un atomique, gardé au maximum
  jusqu'à la lecture, qui le remet à zéro — **aucune allocation, aucun verrou, aucun log** en temps
  réel. `browse` reçoit la table avec la chaîne qu'il installe ; l'affichage vient avec #108.
- Mesuré : une chaîne de 4 blocs avec un parallèle à 3 branches coûte 4,63 µs par bloc de 64 sans
  mesures et 4,87 µs avec (+0,24 µs, +0,02 % du budget). `run`, `render` et `bench` construisent
  sans sondes et ne paient rien. Rendus de référence inchangés ; même sortie au bit près avec et
  sans mesures.

### Changé (#106)
- **La chaîne est une liste de blocs** (ADR 0006) : le modèle NAM et l'IR sont des blocs comme les
  effets, placés librement, y compris dans la branche d'un parallèle. On peut donc décrire
  « son direct en grave | NAM › IR en aigu », mais aussi « propre | fuzz capturée › IR |
  ampli › IR » — **plusieurs modèles sont permis**. `-m`, `-i`, `--pre`, `--post` et
  `--crossover HZ` (raccourci : bandes [HZ], grave = son direct, aigu = NAM › IR) se traduisent en
  blocs : **le son ne change pas d'un bit** (vérifié sur `--crossover`, avec et sans trim, avec et
  sans effets `pre`, blocs 48 / 64 / 1000 ; rendus de référence inchangés).
- Le **parallèle n'est plus un effet** : `split` quitte le catalogue (`namfoop fx` ne le liste plus,
  11 effets) et devient un bloc de structure. `--pre split` / `--post split` répondent « le
  parallèle n'est pas un effet mais un bloc de la chaîne : ses branches ne se décrivent pas en
  texte, seulement dans un preset (#6) ; --crossover HZ pour garder le grave clair sous le son
  saturé ». Un parallèle dans une branche est refusé (un seul niveau), avec un message qui dit quoi
  faire.
- Une branche **sans** modèle est mise au niveau de la première branche qui en contient un
  (généralise le grave clair de `--crossover`) ; son gain s'ajoute ensuite. Le trim de l'instrument
  est en tête de chaîne, sauf quand il n'y a qu'un modèle et que rien ne le précède : il reste alors
  dans l'étage NAM, au bit près comme avant.
- Le log `chaîne :` montre les branches : `◆ bandes 150 Hz [< 150 Hz : direct | > 150 Hz : NAM b7k ›
  IR sm57] › normalisé -18 dB › limiteur` (le modèle et l'IR ne sont plus entre crochets).

### Ajouté (#105)
- `dsp` : le parallèle (`split`) accepte **2 à 4 branches** et un crossover de **2 à 4 bandes**
  (Linkwitz-Riley 24 dB/oct, N-1 coupures croissantes). La somme reste plate : chaque bande traverse
  aussi le **passe-tout** des coupures qui sont au-dessus d'elle (nouvelle forme `Shape::AllPass`),
  mesuré à 0,000 dB d'écart de 20 Hz à 20 kHz pour 2, 3 et 4 bandes. Par branche : gain,
  **polarité inversée** et **muet** (une branche muette n'est pas traitée du tout). Les réglages
  impossibles sont refusés à la construction, jamais dans le thread audio (nombre de branches,
  coupures hors de 20 Hz–12 kHz ou plus proches que ×1,25). `--crossover HZ` est désormais un
  raccourci pour « bandes [HZ] » : rendu **identique au bit près** (3 tailles de bloc comparées).
  Coût par bloc de 64 (x86, release) : 2 bandes 0,67 µs, 3 bandes 1,74 µs, 4 bandes 3,23 µs.

### Changé (#104)
- `browse` : **Échap ne quitte plus**. Il revient d'un niveau (filtre, saisie du nom d'un mélange,
  éditeur `e`, réglages d'un effet -> chaîne -> catalogue) et, quand il n'y a plus rien à remonter,
  affiche « Échap ne quitte pas : q pour quitter ». **`q`** quitte tout de suite si rien n'est en
  jeu ; sinon une fenêtre « Modifications non enregistrées » liste ce qui serait perdu (chaîne
  d'effets changée depuis le lancement, mélange d'IR pas enregistré, brouillon de l'éditeur `e`) :
  **`Q`** quitte sans enregistrer, Échap ou toute autre touche fait rester, et les touches de
  modification seules (Maj, Ctrl, Alt) n'ont aucun effet. C'est l'état qui est comparé, pas un
  drapeau : défaire un changement ou l'enregistrer efface l'alerte.

### Corrigé (#102)
- `split` n'est plus proposé comme un effet réglable : ajouté depuis `browse` ou `--pre` / `--post`,
  il n'avait que deux branches vides, donc un passe-tout qui ne faisait rien (la phase tournait et la
  crête du DI passait de -6,0 à -5,1 dBFS). Le catalogue de `browse` et de `namfoop fx` ne liste plus
  que les effets réglables en texte (« Effets (11) ») ; `--pre split` / `--post split` sont refusés
  avec « les branches ne se décrivent pas en texte, seulement dans un preset (#6) ; --crossover HZ
  pour garder le grave clair sous le son saturé ». `namfoop fx split` le dit aussi. Les branches
  restent décrites en TOML et le parallèle deviendra un bloc de structure (ADR 0006, épique #103).

### Corrigé (#100)
- `browse` : l'onglet Effets s'atteint aussi aux flèches (→ depuis le Mélange, ← pour revenir) et
  avec Maj+Tab ; l'en-tête compte les effets du catalogue (« Effets (12) », « ● n » pour la
  chaîne) au lieu d'afficher « Effets (0) » ; aides de bas d'écran commençant par la navigation ;
  indicateur A/B avec / sans effets dans l'onglet.

### Documentation (#98)
- Guide : section « 10. Effets » (rôle et ordre conseillé de chaque effet, parcours type de
  `browse` à `run`) ; effets dans le schéma de principe, « En bref », le README et les limites.

### Ajouté (#86)
- `browse` : onglet Effets (catalogue, chaîne pré / post autour du modèle, réglages générés depuis
  la table des paramètres, `r` avec / sans effets) ; la ligne de commande équivalente s'affiche et
  la commande de sortie inclut les effets ; `NAMFOOP_BROWSE_TRACE` trace les effets installés.

### Ajouté (#85)
- Effet `reverb` : réseau de 8 lignes à retard (FDN, Householder), `decay` (RT60) de 0,2 à 10 s,
  pré-délai, amortissement des aigus, grave sec sous `lowcut`.

### Ajouté (#84)
- Effet `delay` : écho jusqu'à 2 s, réinjection filtrée (`lowcut`, `highcut`), 90 % au plus sans
  emballement ; changement de temps par fondu entre deux têtes de lecture (sans glissement).
- Effet `chorus` : retard modulé (LFO sinus, interpolation Hermite), grave sec sous `lowcut`.
- Les effets déclarent la durée de leur queue (`tail_seconds`) : le test générique de silence
  l'attend (au plus 90 s) au lieu de 10 s fixes.

### Ajouté (#83)
- Blend grave clair : `--crossover HZ` (+ `--crossover-low-db`, `--crossover-high-db`) garde le grave
  du DI sous un crossover Linkwitz-Riley 4, retard du modèle et de l'IR compensé, niveau aligné.
- Étage parallèle `split` (copie ou crossover, branches d'effets, imbricable) pour les presets.

### Ajouté (#82)
- Filtres et EQ paramétrique, une bande par effet : `hpf`, `lpf` (12 ou 24 dB/oct), `peak`,
  `lowshelf`, `highshelf` (biquads RBJ).

### Ajouté (#81)
- Effets `gate` (seuil, hystérésis, attaque, maintien, relâche, plage) et `comp` (seuil, ratio,
  genou doux, attaque, relâche, makeup, mix pour la compression parallèle).

### Ajouté (#80)
- Effets en ligne de commande : `--pre` / `--post` (répétables, `nom:param=valeur`) pour `run`
  et `render`, `namfoop fx [nom]` (catalogue et paramètres), `namfoop bench --fx` ; ligne
  `chaîne :` au chargement. Sans effet, le son ne change pas (au bit près).

### Interne (#79)
- Socle des effets `dsp::fx` (`FxSpec`, paramètres décrits, catalogue, mini-syntaxe, TOML) avec
  l'effet `gain` et des tests génériques sur tout le catalogue ; ADR 0005 « Effets et chaînes ».

### Ajouté (#78)
- Flush-to-zero des dénormalisés au premier callback d'`engine`, dans `render` et `bench` (ligne
  « dénormalisés » au démarrage) ; son inchangé.

### Ajouté (#3)
- Avertissement quand le thread audio n'a que la priorité de rtkit (FIFO 20) : c'est la cause
  mesurée des pics de charge ; le guide explique le groupe `realtime`.

### Ajouté (#3)
- `run` : répartition de la charge DSP par buffer à l'arrêt (médiane, p99, p99,9, buffers au-delà
  de 50 / 80 / 100 %).
- Réglages temps réel au démarrage : mémoire verrouillée si la limite le permet, avertissements
  (mémoire non verrouillée, gouverneur CPU), préchauffage de chaque chaîne au buffer réel, pile du
  thread audio préchargée ; `--cpu` / `[audio] cpu` épingle le thread audio sur un cœur.

### Corrigé (#24)
- `scripts/feature.sh merge` lancé juste après `pr` attend que les checks de la CI apparaissent au
  lieu d'échouer (« no checks reported »).

### Interne (#74)
- `library::import::ImportArgs` n'a plus le champ `review`, propre à la commande du binaire.

### Interne (#71)
- `library::import::run` et `library::overrides::run` ne prennent plus la config, inutilisée.

### Ajouté (#58)
- Profils de carte (`profiles/itwo.toml`, `~/.config/namfoop/profiles/`) : device, buffer,
  périodes, entrée basse et contrôles mixer `[[mixer]]` réglés à l'ouverture (ALSA). Reconnus
  d'après la carte ou choisis par `[audio] profile` ; priorité CLI > config > profil > défaut.
  `setup` reprend le profil reconnu ou en écrit un nouveau.

### Interne (#57)
- Un `CLAUDE.md` par crate, `CLAUDE.md` racine allégé (carte, commandes, cycle) ; `check-docs.sh`
  vérifie un `CLAUDE.md` par crate et la taille des fichiers (> 1 000 lignes refusé) ; champ
  « Crates touchées » dans les modèles d'issue.

### Interne (#56)
- `browse/state.rs` et `browse/ui.rs` découpés par sous-fonction (aucun fichier au-delà de 600
  lignes) ; comportement et rendu inchangés.

### Interne (#55)
- Nouvelle crate `library` (index, import, corrections, recommandations, profils) ; `import.rs`
  découpé par responsabilité. Sorties de `models`, `meta` et `import` identiques.

### Interne (#54)
- Nouvelle crate `model` : types de la config (`config.toml`) et du routage, lecture / écriture
  TOML. Format du fichier inchangé (test de relecture à l'octet près).

### Interne (#53)
- `browse`, `calibrate`, `setup` et `latency` passent par `engine` (tap d'entrée pour
  `calibrate`, porte `engine::raw` pour les outils de mesure) ; seule `engine` ouvre la carte.

### Corrigé (#53)
- `setup`, test des buffers : l'échec affiche à nouveau sa cause (perdue depuis #50).
- `calibrate` signale les échantillons perdus si la machine n'a pas suivi l'enregistrement.

### Interne (#52)
- Nouvelle crate `engine` (session audio : carte, routage, chaîne échangeable, vumètres) ; `run`
  passe par elle. Le routeur y déménage. Aucun changement de son ni d'affichage.

### Ajouté (#51)
- ALSA : négociation du format d'échantillon (`S32_LE`, `S24_3LE`, `S24_LE`, `S16_LE`, le premier
  accepté gagne, logué au démarrage) : cartes USB en 24 bits sur 3 octets et HAT I2S en 16 bits.
- `--capture-device` (config `[audio] capture_device`) : capture ALSA sur un autre device que la
  lecture.

### Interne (#50)
- Les backends audio (ALSA, JACK, sim) sortent du binaire dans la crate `audio-io`, sans clap ni
  config ; aucun changement de comportement.

### Interne (#49)
- `docs/architecture.md`, décisions d'architecture (`docs/adr/`) et contrôle du sens des
  dépendances entre crates (`scripts/check-arch.sh`, appelé par `check.sh`).

### Interne (#48)
- Rendus de référence du son (`crates/dsp/tests/golden.rs`) : filet de non-régression avant le
  découpage en crates.

### Ajouté (#42)
- `browse`, onglet Modèles : panneau Spectre (courbe du modèle avec son IR, référence, écarts par
  zone, grain comparé) et **A/B entre modèles** (`c` épingle, `r` bascule ; le modèle B garde
  l'IR si besoin). Indicateur « A/B modèles » dans l'en-tête ; marques A / B à côté des noms.
- Éditeur `e` : le grain est remesuré avec ta basse quand l'entrée change.
- `NAMFOOP_BROWSE_TRACE` trace aussi les mesures de grain.

### Ajouté (#33)
- Profil sonore et grain des modèles, mesurés avec ta basse (enregistrement de `calibrate`) au
  niveau réel : grain (clair / crunch / saturé / très saturé, compressé, « casse à +n dB »),
  courbe tonale (avec l'IR préférée pour les modèles sans baffle), mini-spectre et étiquettes
  relatives au pack. Affichés par `models`, l'onglet Modèles de `browse`, l'import et `meta`.
  Cache `library/.cache/model-profiles.json`, recalculé après `calibrate` ou un changement
  d'entrée.

### Corrigé (#39)
- `browse`, A/B : indicateur permanent en haut de l'écran (A / B ◆ / IR COUPÉE, celui qu'on
  entend surligné) et marques `◀ en cours (A)` / `(B)` dans la liste ; `r` explique pourquoi il
  ne fait rien quand la référence est l'IR en cours ou une copie identique ; une chaîne identique
  à la précédente n'est plus reconstruite (pas de fondu inutile).

### Ajouté (#37)
- Vumètres : repère `┃` sur `in` à la crête de ta calibration (cible − trim) et niveau reçu par le
  modèle (`→ modèle -30 dB`), dans `browse` et la ligne d'état de `run`. `calibrate` enregistre sa
  cible (`calibration_target_dbfs`). Guide : section « Lire les vumètres ».

### Ajouté (#32)
- `browse` : panneau Spectre à droite de la liste (onglets IR et Mélange, terminal large) avec la
  courbe de l'IR sous le curseur, celle du mélange résultant, et une **référence épinglée** (`c`)
  superposée avec les écarts par zone.
- Écoute A/B : `b` coupe / remet l'IR (aussi dans l'éditeur), `r` bascule entre ton choix et la
  référence ; à volume égal en mode `match`.

### Ajouté (#34)
- Profil sonore des IR et mélanges : réponse par tiers d'octave (IR telle qu'elle est jouée),
  mini-spectre de 12 colonnes et étiquettes relatives au pack (« sub +4 », « sombre », « air +3 »,
  « aigus coupés à 3,9 kHz »), affichés par `models` et dans l'onglet IR de `browse`. Calculé à
  l'import, gardé dans `library/.cache/ir-profiles.json` (par empreinte du contenu).
- Ordre naturel des fichiers (Mix 2 avant Mix 10).
- `browse` accepte une bibliothèque sans modèle (écoute des IR sur le son direct).

### Ajouté (#30)
- `browse` : ligne « son direct (DI, sans modèle) » en tête de l'onglet Modèles, pour écouter les
  IR seules sur le son de la basse ; la commande affichée en sortie est alors `namfoop run -i <id>`.

### Corrigé (#28)
- `import` : un fichier identique à un autre (même contenu, comparé octet par octet après
  l'empreinte) est désormais importé et signalé au lieu d'être écarté, pour que chaque pack reste
  complet (série de micros d'un baffle). `--skip-duplicates` rétablit l'ancien comportement. Un
  réimport compte les fichiers « déjà présents » ; un fichier différent du même nom est refusé.

### Ajouté (#26)
- Guide : tutoriel d'import pas à pas (import et revue, type de capture à l'oreille, gain
  d'entrée par rapport à la calibration, volume en mode `match`, corrections en masse).

### Modifié (#26)
- `browse`, éditeur : `a` n'applique aux modèles filtrés que les champs modifiés (type et/ou
  entrée), au lieu de recopier les deux.
- Guide : `--output-db` est sans effet en mode `match` (l'alignement de niveau vient après).

### Ajouté (#21)
- `browse` : touche `e`, éditeur du modèle en écoute — type de capture, gain d'entrée propre
  (entendu tout de suite : calibration à l'oreille), IR préférée (les conseillées défilent en
  direct), marque, nom ; `s` enregistre dans `library/overrides.toml`, `a` applique le type et
  l'entrée à tous les modèles filtrés. Modèles corrigés marqués ✎, réduction du limiteur affichée.
- `namfoop import --review` : ouvre le navigateur sur les modèles importés, éditeur ouvert.
- `namfoop browse --filter` : filtre de départ.

### Ajouté (#20)
- `namfoop import` : importe zips, dossiers et fichiers (rangement par pack avec sous-dossiers,
  notices dans `library/docs/`, fichiers parasites ignorés), écarte les doublons par contenu,
  vérifie chaque modèle (chargé et joué) et chaque IR, déduit le type de capture du nom et des
  dossiers (`DI`, `FullRig`…) quand les métadonnées sont fausses, retient l'IR fournie avec le
  pack comme IR préférée. Remplace `scripts/import-tone3000.sh`.
- `namfoop meta` : affiche et corrige les métadonnées d'un modèle (type de capture, marque, nom,
  gain d'entrée propre, IR préférée), aussi en groupe (`--all`). Corrections dans
  `library/overrides.toml`, par empreinte du contenu : les fichiers d'origine ne sont jamais modifiés.
- Les valeurs de remplissage des métadonnées (`T3K-Null`, `tz-make`…) sont traitées comme absentes.

### Modifié (#20)
- Le « pack » d'un fichier inclut ses sous-dossiers (`PopPunkBass_NAMA2/DI`).

### Ajouté (#16)
- `namfoop browse` : navigateur plein écran qui joue en direct — modèles (badge IR incluse /
  requise), IR conseillées en tête, filtre, écoute après 150 ms d'arrêt, création de mélanges à
  l'oreille (gain, polarité, alignement) sauvés dans `library/cabs/`, commande équivalente à la
  sortie.
- Changement de chaîne à chaud sans clic (`dsp::swap`, fondu de 20 ms), réutilisable par les presets.

### Ajouté (#15)
- Mélanges d'IR (`library/cabs/*.cab.toml`) : plusieurs micros / positions avec gain, alignement
  temporel automatique et polarité, calculés au chargement (coût live = une IR) ; identifiants
  `c…`, conseillés en premier par `models` / `-i auto`.

### Ajouté (#14)
- Identifiants courts et stables pour chaque modèle (`m…`) et IR (`i…`), affichés par `models`,
  acceptés par `-m` / `-i`.
- `models` indique si chaque modèle a besoin d'une IR et conseille la meilleure (même marque,
  baffle basse) ; `-i auto` ; `run` avertit quand une capture sans baffle est jouée sans IR ;
  `default_ir` dans la config.

### Modifié (#14)
- Recherche par mots-clés : mots entiers prioritaires, et version « de base » préférée
  (`angel` -> « Angel », `angel v2` -> « Angel_v2 ») ; les candidats ambigus affichent leur
  identifiant.

### Ajouté
- `namfoop setup` : assistant plein écran (backend, carte, vumètres par entrée, bip par sortie,
  test automatique des buffers), qui enregistre `[audio]` et `[routing]` dans la config.
- Cartes multicanaux : namfoop ouvre tous les canaux de la carte (ex. Soundcraft MTK 14/12).
- Routage : `--bass-in`, `--mic-in`, `--route N=processed|di|mic|mute` (remplace `--out2`).
- Backend `sim` : carte simulée (basse synthétique) pour tester sans matériel.
- Options globales `--config` / `--no-config` (communes à toutes les commandes).
- Harnais : cycle issue -> branche -> PR -> merge (`scripts/feature.sh`), hooks git qui refusent
  commits et pushs directs sur `main`, règles de PR vérifiées en CI, modèles d'issue et de PR.
- CI plus rapide : plus de double exécution par PR, jobs lint et check en parallèle, profil `ci`
  (tests et binaire compilés une seule fois, sans LTO), cache sccache pour Rust et le C++ NAM.

### Modifié
- Plus besoin de `pw-jack` : namfoop trouve la libjack de PipeWire et se relance avec si besoin.
- Les réglages audio (`--backend`, `--device`, `--buffer`...) viennent de la config s'ils ne sont
  pas donnés.
- `calibrate --channel` prend par défaut l'entrée basse du routage ; `calibrate --config` devient
  l'option globale `--config`.

### Supprimé
- `run --out2` (remplacé par `--route 2=di|mute|processed`).

## 0.1.0 — 2026-11-08

Premier prototype.

### Ajouté
- Chaîne ampli : trim d'entrée -> modèle NAM -> IR -> niveau -> master -> limiteur.
- Modèles NAM via NeuralAmpModelerCore v0.6.0 (WaveNet, LSTM, A2, format 0.7 `SlimmableContainer`,
  choix de taille `--size`).
- IR de baffle : convolution sans latence, rééchantillonnage automatique, troncature (`--ir-ms`),
  normalisation sur 80 Hz – 5 kHz.
- Backends JACK (PipeWire) et ALSA direct (flux liés, priorité temps réel via rtkit, libération
  automatique de la carte tenue par PipeWire).
- `run` : ligne d'état (crêtes, limiteur, charge DSP, xruns, écrêtages), sortie 2 `same|di|mute`.
- `calibrate` : mesure du niveau de la basse, trim d'entrée, volume « match » aligné sur le DI.
- `render`, `bench`, `models`, `ports`, `latency`.
- Recherche des modèles et IR par mots-clés dans `library/`.
- Configuration `~/.config/namfoop/config.toml`.
- Harnais : `scripts/check.sh` (format, clippy, tests dont absence d'allocation temps réel et
  indépendance au bloc, cohérence de la documentation, rendu de la bibliothèque),
  `scripts/test-backends.sh` (tests matériels sur les deux backends).
