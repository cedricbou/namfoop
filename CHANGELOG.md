# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/). Chaque fonctionnalité ou
correction visible par l'utilisateur ajoute une ligne dans « Non publié » ; à la publication, la
section prend le numéro de version.

## Non publié

### Mesures (#351)
- **La charge DSP du tableau de latence est celle de la chaîne en régime établi.** Chaque passe
  chronométrait aussi ses premiers blocs, quand le son venait de changer et que le poids du modèle et
  les tampons de l'IR étaient encore froids. La p99,9 d'une passe de 3 000 blocs, ce sont ses trois
  pires : c'étaient exactement ceux-là. Deux lignes de la même chaîne pouvaient ainsi annoncer des
  charges dans un rapport de 2,5 avec des latences identiques. Les blocs des fenêtres de
  stabilisation ne sont plus comptés dans la charge, comme ils ne l'étaient déjà plus dans la
  latence.
- **La charge n'est donnée que sur la ligne « à côté »**, la seule où la chaîne travaille sur du
  signal : dans le trajet, elle ne reçoit que l'impulsion, donc du silence le reste du temps. La
  ligne de conclusion — celle qui juge la passe — lit cette passe-là.
- **Une colonne « > 80 % »** compte les blocs au-delà de 80 % du budget. Ce sont des blocs réels, là
  où la p99,9 d'une passe courte n'est qu'une estimation.
- **Un retour trop faible donne « pas de retour », et non une latence courte.** Il faut environ 26 dB
  entre la crête du retour et le bruit de fond pour que son début soit cherché : en dessous, le
  premier pic de bruit de la fenêtre était pris pour le début du son, et un câble à moitié branché
  rapportait une latence courte et aléatoire comme une mesure.
- Le guide dit ce que mesure la colonne « dans le trajet » : le temps que l'**attaque** du son met à
  monter, et non un retard que la chaîne ajouterait au signal. C'est pour cela qu'il vaut +16 à
  +18 frames qu'il y ait un modèle, une IR, six effets ou quatre modèles en parallèle.
- Vérifié sur la Signature 12 MTK (buffer 64, 3 périodes, boucle AUX -> entrée 2) : aller-retour
  **305 frames (6,35 ms)**, dispersion 0, les cinq lignes « à côté » à **+0 frame**, **0 xrun**, et
  une charge qui ne dépend plus du nombre de mesures demandées — de 2 / 3 % par bloc pour un modèle
  seul à 6 / 10 % pour quatre `nam › ir` en parallèle (médiane / p99,9), **aucun bloc au-delà de
  80 %** du budget.

### Interne (#349)
- **Les tops d'horloge MIDI se comptent dans le fil qui lit le câble**, et le compteur s'expose pour
  que le fil audio le lise directement. Placer un temps demande la milliseconde, alors que la file
  des messages est vidée par une boucle qui repasse toutes les 20 à 30 ms — un top vaut 20,8 ms à
  120 BPM. Rien de visible pour l'instant : c'est la pièce qui manquait pour caler le clic sur
  l'horloge reçue (#347).

### MIDI (#347)
- **Le clic tombe sur le temps de l'horloge reçue.** Il avait le bon tempo mais tombait à
  contretemps : sa grille était ancrée au démarrage de namfoop, et seuls un tap ou un `Start` la
  recalaient. Il **compte maintenant les tops** de l'horloge — une noire en vaut 24 — au lieu de
  compter des échantillons. Sa position est donc absolue : rien ne dérive (0,3 BPM d'écart valaient
  une demi-seconde en quatre minutes) et un changement de tempo ne le décale plus, ni une
  accélération progressive ni les `+` / `-` d'un clavier.
- **Quel top porte le temps** s'apprend d'un `Start` ou d'un Song Position Pointer, du début d'un
  flux quand namfoop écoutait déjà, ou d'un appui sur `*`. Comme la grille reste ensuite verrouillée
  sur les tops, **un seul appui suffit** pour toute la session.
- **Un tap se cale sur le top le plus proche** quand une horloge tourne, donc à moins d'un
  demi-intervalle — 8,8 ms à 142 BPM — au lieu de l'intervalle entier que coûtait « le dernier top
  reçu ». Et comme la grille reste verrouillée sur les tops, ce tap-là tient pour la session.
- **Le clic ne double plus et ne décale plus.** Premier essai sur le matériel : des doubles clics,
  d'autant plus nombreux que le tempo montait, et des décalages qui persistaient. La position du
  temps était déduite d'une durée, donc d'un arrondi posé là où la gigue du câble passe et repasse
  — 656 basculements en 20 secondes à 142 BPM. Elle est maintenant une soustraction de deux nombres
  de tops, donc exacte, et le clic la verrouille au lieu de la relire à chaque bloc.
- **La bande de mesure suit la même grille que le clic** : elle compte les tops dès qu'une horloge
  tourne, au lieu d'intégrer le tempo. Elle dérivait sinon exactement comme le clic dérivait.
- **Une position dans le morceau (Song Position Pointer) place le temps à la double-croche.** Elle
  ne tombe pas forcément sur une noire : sans en tenir compte, le clic sonnait faux d'un quart de
  temps sans que rien le signale.
- **L'en-tête dit quand le temps est deviné.** Raccroché au milieu d'un flux déjà en cours, namfoop
  ne peut que deviner lequel des 24 tops porte le temps : il affiche alors
  `♩ 142 (MIDI, temps à confirmer : *)`, parce que c'est à l'oreille indiscernable d'un tempo faux.
- **`namfoop run --click` suit l'horloge et le tap** (CC 14), alors qu'il les ignorait : un clic qui
  ne sait rien de l'horloge branchée est un piège. Le tempo de la **chaîne**, lui, reste figé dans
  `run` — c'est ce qui garde `run` et `render` reproductibles.
- **Le tempo entendu s'affiche sur la ligne d'état de `run`** (`♩ 145 (MIDI)`) au lieu de
  s'écrire dans le journal. Une ligne par valeur publiée en donnait des dizaines par seconde avec
  l'horloge d'un clavier — le tempo sort de sa bande d'hystérésis à chaque gigue — et elles se
  mêlaient à la ligne d'état. Le journal ne garde que les changements de **source** : l'horloge qui
  prend la main, qui se perd, le tap qui passe devant.
- **`*` recale le temps 1 depuis n'importe quel écran**, banc d'essai compris. Il y était refusé en
  entier pour une raison qui ne concerne que le tempo du preset, alors que le clic s'allume partout :
  on se retrouvait avec un clic impossible à recaler. Le tempo, lui, se tape toujours dans la Chaîne.

### MIDI (#224)
- **Ableton peut mener le concert.** Avec un set ouvert (dans `browse` comme sur l'écran de scène
  `run --set`), un **Program Change** appelle le morceau dont le `pc` vaut ce numéro — sur son
  premier son —, et un **CC 34** de valeur k appelle le son k+1 du morceau joué. Le numéro d'un
  morceau est fixe : un rappel en fin de concert tombe toujours juste.
- **Un PC suivi d'un CC ne fait qu'un seul changement de son** : Ableton envoie souvent les deux en
  tête de clip, et deux changements s'entendraient. namfoop attend 15 ms que le CC précise le son.
- **Le transport place la battue** : `Start` la remet sur le 1 de la mesure 1 du morceau et la
  lance, `Stop` l'arrête, et un **Song Position Pointer** la pose à la bonne mesure — juste même
  après un changement de mesure dans le morceau, la carte des mesures faisant la conversion.
- **Rien ne change le son sans le dire** : un PC qu'aucun morceau ne porte, un CC au-delà des sons
  du morceau (là où un Fractal écrêterait sur le dernier) et une position au-delà de ce que le
  morceau déclare sont ignorés avec un message.
- Le guide dit quoi régler dans Ableton : un clip de Program Change en tête de morceau, un CC 34 aux
  changements de son, l'horloge et la position.

### MIDI (#313)
- **Les effets suivent l'horloge d'Ableton.** Le Beat Clock donne le tempo entendu dès qu'il est
  verrouillé (une noire de tops cohérents) ; `Start` remet la battue sur le temps 1, `Continue`
  reprend sans la recaler, `Stop` garde le dernier tempo. Une horloge qui se tait (500 ms sans top)
  **garde son tempo** et l'en-tête passe en « horloge perdue » en jaune : jamais de retour
  silencieux au tempo du preset, qui serait un changement de son causé par une panne de câble.
- **Le tap au pied**, sur le CC 14 (le tap Fractal), fait exactement ce que fait la touche `*` — et
  comme elle, il passe devant l'horloge jusqu'au prochain événement net. Chaque message porte
  l'instant où il est arrivé, sans quoi un tap se tromperait de 7 BPM à 120.
- **Un grand changement de tempo est suivi.** Le rejet de gigue à ±25 % de l'arbitre bloquait tout
  saut plus grand (120 → 92 BPM fait 30 % d'intervalle) : le tempo restait sur l'ancien jusqu'à
  l'arrêt de l'horloge. Après huit rejets de suite, la fenêtre repart du nouvel intervalle, sans que
  le tempo publié bouge pendant la remesure ni qu'une panne soit annoncée.
- `[midi] clock = false` coupe l'écoute de l'horloge **sans** couper le tap : en jam, une horloge
  qui traîne est un risque, pas un service. `[midi] cc_tap` règle le numéro du tap.

### MIDI (#314)
- **Un Program Change choisit un preset.** Sans set list, `PC n` installe le preset de la place `n`
  de la banque — celle que `P` montre et que `PgDn` / `PgUp` parcourent —, par le même chemin que le
  pied sur `PgDn` : banque préchargée, aucune coupure, et la réverbe du preset quitté qui finit de
  sonner. Un pédalier de gabarit Fractal n'a donc rien à régler de particulier.
- **Rien ne change le son sans le dire** (ADR 0011) : une place vide, un preset illisible, un
  Program Change reçu pendant une saisie ou alors qu'un set est ouvert sont **ignorés** avec un
  message. Le **CC 34** (son du morceau) et le **Bank Select** (CC 0) le sont aussi hors set, et un
  Bank Select n'est jamais pris pour autre chose.
- Les **numéros de message se règlent** : `[midi] cc_sound` (défaut 34). Le défaut évite de
  configurer, il ne l'interdit pas.
- **Les Program Change se comptent de 1 à 128**, comme le `pc` d'un morceau dans un set : une seule
  numérotation dans namfoop. Un éditeur de pédalier qui compte de 0 à 127 affiche donc un numéro de
  moins — le guide le dit.

### MIDI (#312)
- **namfoop écoute le MIDI.** Un pédalier branché en USB ou une application comme Ableton sont lus
  dès le démarrage : sans rien préciser, c'est le **premier port d'entrée** de la machine, et
  namfoop dit lequel. `--midi-in <port>` en désigne un autre par son adresse de séquenceur (`28:0`)
  ou par un morceau de son nom (`MC6`), `--no-midi` coupe l'écoute, et `namfoop setup` gagne une
  étape « pédalier MIDI » qui enregistre le port choisi dans `[midi] in`.
- **`namfoop midi`** liste les ports puis affiche en clair ce qui arrive (`PC 3`, `CC 34 2`,
  `horloge 120.0 BPM`), sans ouvrir la carte son : c'est la commande qui répond à « est-ce que mon
  pédalier parle ? » avant un concert. Les tops d'horloge se résument à une ligne par noire.
- **Un pédalier débranché n'empêche jamais de démarrer** : port absent, nom introuvable ou
  séquenceur indisponible donnent un message, et tout le reste marche.
- **Sans matériel**, un port `sim:CHEMIN` lit des lignes de texte dans un fichier ou un tube nommé
  (`pc 3`, `cc 34 2`, `clock`, `start`, `spp 64`) : de quoi essayer le MIDI sans pédalier — c'est
  aussi ce dont se servent les tests de bout en bout. Le vocabulaire est celui qu'affiche
  `namfoop midi`.
- Ce que namfoop **fait** des messages reçus (choisir un preset, taper le tempo, suivre l'horloge)
  vient ensuite : ici, l'écoute existe, et les messages sont affichés et tracés. Conventions :
  [ADR 0011](docs/adr/0011-midi-conventions-fractal.md).

### Mesures (#341)
- **La mesure de latence tient maintenant à la frame près.** Deux défauts se cumulaient : les
  premières mesures étaient comptées avant que le flux de la carte soit établi, et la latence était
  cherchée à la **crête** du retour, dont l'instant saute d'une bosse à l'autre quand le retour
  sonne. Résultat sur la Signature 12 MTK : la ligne témoin de chaque chaîne annonçait jusqu'à
  **+42 frames d'écart** au lieu de +0, et la référence du tableau pouvait être fausse.
- La mesure attend donc **3 s** que le flux s'établisse, puis chaque passe commence par deux
  fenêtres marquées « (stabilisation) », affichées mais non comptées ; et c'est le **début** du
  retour — le premier échantillon qui monte au quart de la crête — qui donne la latence. Vérifié sur
  la carte : aller-retour 262 frames (5,46 ms), **dispersion 0**, les cinq lignes « à côté » à
  **+0 frame**, et de +17 à +19 frames de retard réellement ajouté par les chaînes.
- Les latences affichées baissent donc un peu : elles disaient quand la crête du retour arrivait,
  elles disent maintenant quand le son arrive. `namfoop latency` sans option en profite aussi.

### Mesures (#339)
- **`namfoop latency --chains` mesure la latence avec des chaînes d'effets**, sur le même câble de
  bouclage et dans la même session : d'abord le trajet nu, puis, pour chacune des cinq chaînes de
  référence, une passe où l'impulsion la **traverse** et une passe où elle tourne **à côté** sur un
  bruit interne — la seconde est le témoin de la première. Chaque passe rend aussi sa charge DSP
  (médiane, p99,9, blocs au-delà du budget) et ses xruns, ce que `bench` ne voit pas : lui mesure
  hors ligne, sans carte. `--chain <nom>` n'en prend qu'une, `-m` / `-i` y mettent un modèle et une
  IR de la bibliothèque. Sans eux, le modèle d'exemple du moteur NAM et une IR de synthèse : la
  mesure marche sans bibliothèque, y compris sur une machine fraîchement installée.
- Les chaînes de référence sont **décrites en un seul endroit** (`dsp::bench`), partagé avec le
  repère de coût par bloc, et le catalogue gagne le pire cas connu : quatre modèles › IR en
  parallèle.
- `namfoop latency` sans option mesure ce qu'il mesurait : même signal, même détection, même rapport.
- Mesuré sur la Signature 12 MTK (buffer 64) : l'aller-retour est de **305 frames (6,35 ms)** et **la
  chaîne ne le décale pas**, même à quatre modèles en parallèle (p99,9 à 8 %, 0 xrun). Les chiffres
  sont dans [`crates/audio-io/CLAUDE.md`](crates/audio-io/CLAUDE.md).

### Interne (#327)
- **Une issue à cadrer se marque, et ne se code pas.** L'étiquette `needs-refinement` s'ajoute au
  type d'une story ou d'une épique qui n'est pas assez approfondie ; sa rubrique « À trancher »,
  alors obligatoire, dit ce qu'il reste à décider avec l'utilisateur.
- `scripts/feature.sh start` **refuse** une issue à cadrer, sans passe-droit ; `issue --refine` en
  crée une, `refine` les liste. `docs/style-issues.md`, `CLAUDE.md` et la skill `ecrire-issue`
  décrivent le cadrage : une proposition par question, chaque décision reportée dans l'issue, puis
  l'étiquette retirée.

### Interne (#325)
- **Les issues s'écrivent pour le bassiste.** La backlog a été refondue en épiques, stories et
  chores lisibles sans ouvrir le code, et le harnais suit : [`docs/style-issues.md`](docs/style-issues.md)
  fixe le titre, les rubriques de chaque type et ce qui n'entre pas dans le corps ; le détail
  technique (crates, ADR, tests, mesures) va dans un commentaire « Cadrage technique ».
- `scripts/feature.sh issue <type> "Titre" corps.md [cadrage.md]` crée l'issue avec son étiquette et
  poste le cadrage en commentaire ; `lint <n°>` contrôle une issue existante. La création et
  `start` **refusent** un titre préfixé d'une crate, un corps sans les rubriques de son type, ou qui
  nomme une crate, un fichier source ou un chemin Rust ; `start` refuse aussi une épique.
  `start "Titre"`, qui créait une issue vide étiquetée `enhancement`, renvoie à `issue`.
- Les modèles d'issue de GitHub deviennent story, épique, chore et bug ; la skill `ecrire-issue`
  porte la méthode ; la feuille de route de `CLAUDE.md` renvoie aux issues `epic`.

### Corrigé (#318)
- **Sans carte branchée, namfoop ne part plus sur « hw:iTwo ».** Sur une machine sans config — le
  portable, un PC prêté, le Pi avant son HAT —, toute commande qui ouvre la carte s'arrêtait sur
  `ouverture de hw:iTwo` : la carte de l'auteur, device par défaut en dur depuis la 0.1.0, devenue
  atteignable au retrait du backend JACK, qui choisissait lui-même la première carte de la machine.
  namfoop ne choisit **aucune** carte à votre place : il liste les cartes qu'il voit et dit quoi
  faire — `namfoop setup`, `--device hw:<id>`, ou `--backend sim --device 4x6` pour travailler sans
  matériel. Même message et même liste quand la carte de la config n'est pas branchée, ou que son
  nom ALSA a changé.
- L'aide de `--device` promettait « sinon 1ère carte / hw:iTwo » : ce repli était celui de JACK et
  n'existe plus.

### Ajouté (#223)
- **La battue et la bande de mesure dans l'espace Set.** `Espace` lance et arrête la battue, et
  **l'appui est le 1** de la mesure en cours : on se raccroche au groupe sans compter. En tête de
  la colonne du prompteur, une bande montre les temps de la mesure — **temps 1 en jaune**, temps en
  vert, subdivisions en points gris, le temps qui sonne en inversé —, le tempo entendu, la mesure,
  `mes. n/N SECTION`, et ce qui vient.
- **Le compte à rebours de changement de son** : quand la section suivante est liée à un autre son
  que celui qu'on joue, la bande annonce `→ fuzz-face dans 4 mes.`, puis sur la dernière mesure
  `→ FUZZ-FACE` et le décompte des temps `4 · 3 · 2 · 1`. Un changement de mesure
  (`passe en 7/8 (2+2+3)`) ou de tempo (`♩92`) est annoncé de la même façon.
- **namfoop tient la carte des mesures** du morceau, que ne tient aucun pédalier du marché : la
  mesure, le tempo, le regroupement des temps et la durée de chaque section, lus dans le
  `.set.toml` et le prompteur. Le **BPM se compte toujours à la noire**, comme Ableton Live et
  l'horloge MIDI ; en x/8 le tick de la bande est la croche, et 6/8, 9/8, 12/8 se battent par
  trois. Sans `{x_namfoop_bars}`, `{duration}` se répartit sur les sections.
- Le texte du prompteur **défile au tempo** : les lignes d'une section se partagent ses mesures.
  **En marche la position suit le temps** — changer de son ne la déplace pas —, **à l'arrêt elle
  suit le son**. `Fin` / `Début` recalent sur le 1 de la section, `*` recale le temps mais pas la
  mesure, et changer de morceau remet au début en arrêtant la battue.
- Un morceau **sans prompteur** a sa bande quand même : tempo et mesure du morceau, mesures
  comptées.
- `Espace` est **réglable** (`[set] prompt_play`, défaut `Space`), comme les autres touches du
  pied, et elle agit aussi sur scène (`run --set`) : elle ne change ni le son ni le set.
- Le tempo d'une section entre dans l'arbitre de tempo **comme celui d'un preset** : les delays le
  suivent, un tap ou une horloge MIDI le recouvrent, et le preset n'est pas marqué « non
  enregistré ». La bande est dessinée par le fil de l'interface : **rien n'entre dans le fil
  audio**, et le harnais de bout en bout le vérifie en comptant les chaînes installées.

### Documentation (#311)
- **[ADR 0011](docs/adr/0011-midi-conventions-fractal.md) : le MIDI suit les conventions des
  pédaliers Fractal par défaut, et Ableton pour le transport.** Program Change = le morceau,
  **CC 34** = le son du morceau (la scene Fractal), **CC 14** = le tap (le tap Fractal), horloge
  Beat Clock avec `Start` / `Stop` / `Continue` et Song Position Pointer. But : brancher un pédalier
  de gabarit Fractal et Ableton **sans rien configurer** — et les numéros restent réglables.
- Cela **amende** la décision du 15/11/2026 « MIDI à la Helix » : le son d'un morceau était appelé
  par CC 69 (le snapshot Helix), il l'est par **CC 34**. `docs/design/set.md` et sa maquette sont à
  jour.
- Deux divergences volontaires, écrites avec leur raison : une valeur hors du morceau est
  **ignorée** et le dit (Fractal écrête — jouer un autre son que celui demandé est pire qu'un
  message), et un **tap détrône l'horloge** (Fractal, Helix et le Quad Cortex verrouillent le tap :
  on tape justement pour rattraper le groupe).
- L'**étape 5** est découpée : #312 l'entrée MIDI (avec une **source simulée**, un agent ne branche
  pas de pédalier), #313 l'horloge et le tap, #314 le MIDI hors set, #224 le MIDI du set, et #315 le
  traitement du micro — sorti du MIDI, avec lequel il n'avait rien à voir.

### Interne (#254)
- **La table des touches porte les étages.** Les communes (`S` `P` `n`), la pédale (`*` `k`) et les
  sorties (`h`, l'autre espace, `q`) étaient recopiées dans **cinq** tables : ajouter une touche
  d'étage demandait cinq éditions, et une divergence entre deux copies ne faisait échouer aucun
  test. Elles sont maintenant déclarées **une fois** et ajoutées à chaque espace dans un **ordre
  canonique** : le bassiste les retrouve au même endroit d'un écran à l'autre, et un espace ne peut
  plus en oublier une.
- Ce qu'un écran garde : **ajouter** ses touches à lui, **dire autrement** une touche d'étage, la
  **retirer de la barre** sans la retirer de l'aide `h`, ou l'**éclipser**. Ce qu'il ne peut plus :
  la replacer, ou en perdre une en silence. Trois tests le tiennent, dont « une surcharge qui ne
  correspond à aucun étage » — sans quoi on croirait avoir changé un texte que l'étage continue de
  dire.
- **Ce qui change à l'écran** : dans la Chaîne et sur la sortie d'un parallèle, quatre segments de
  la barre du bas changent de place (`PgDn/PgUp preset` et `t TOML` passent avant `S enregistrer`,
  `* tap` et `k clic` après `n vierge`) — **rien** à 80 colonnes, où la barre est coupée avant d'y
  arriver. La Bibliothèque et l'espace Set sont inchangés. L'aide `h` de la sortie d'un parallèle
  gagne les textes longs de la Chaîne.
- **Ce qui est protégé dans la barre** reste le défaut, partout le même : les trois derniers
  segments. Le jour où un écran devra en garder d'autres — l'édition d'un set aimerait protéger
  `Échap sortir` —, c'est sa table qui le dira, et la couture est nommée dans `ui/mod.rs`. Rien
  n'est écrit d'avance : le code le plus stable est celui qu'on n'écrit pas.

### Interne (#265)
- **L'exécution des actions sort de la boucle d'événements.** `browse/mod.rs` était le plus gros
  fichier du dépôt (855 lignes, à 145 du seuil d'échec) et ne portait **qu'un seul test**, alors que
  c'est lui qui écrit les fichiers, rend la main à `$EDITOR` et décide de ce qu'on entend. Il tombe à
  **510 lignes** et ne garde que la boucle : les touches, l'horloge, les cases vivantes, les
  vumètres, le dessin.
- Le `match` des 13 actions devient `actions/`, une famille par fichier — `sound.rs` (la chaîne
  demandée, le grain d'un modèle), `files.rs` (correction d'un modèle, preset, mélange d'IR),
  `set.rs` (le fichier du set, ses prompteurs, `$EDITOR`, le set du moment) — derrière une
  **frontière nommée** : la structure `Ctx` énumère ce qu'une action a le droit de toucher, si bien
  qu'en voir un de plus signale une décision en train de descendre dans l'exécution. Le `match`
  d'aiguillage reste **exhaustif** : une action nouvelle ne compile pas sans dire qui la prend.
- Les `continue` et les `return` au milieu de la boucle deviennent un `Flow` : finir le tour, passer
  au suivant, ou quitter.
- **Quatre tests là où il n'y en avait aucun**, sur ce qui se passe mal : un preset illisible ne
  cache pas les autres, un set illisible reste dans la liste `L` **marqué** (le cacher laisserait
  croire que le fichier n'existe pas), un prompteur absent se garde en « illisible » sans paniquer,
  et `--no-config` n'écrit rien.
- Aucun changement de comportement : l'oracle du clavier est inchangé, et le harnais de bout en bout
  passe aux deux tailles d'écran.

### Interne (#254)
- **La table des touches ne tient plus dans un seul fichier de 854 lignes** : elle devient un
  module, un fichier par famille d'écrans (`stage.rs`, `windows.rs`, `set.rs`, `chain.rs`,
  `library.rs`, `set_edit.rs`), aucun au-delà de **147** lignes. Une macro `bind!` ramène chaque
  déclaration — quatre lignes de littéral `struct` telles que `rustfmt` les étale — à **une ligne**,
  si bien qu'une table se lit comme une table. `scripts/check-docs.sh` ne signale plus `bindings`.
- **`all_keys()`, la sixième copie, est dérivée de la table** : l'oracle mesure désormais l'union de
  ce que déclarent les 24 contextes, touches réglables résolues, plus le témoin `é` (la lettre qui
  ne veut rien dire). Écrite à la main, cette liste laissait un caractère lié échapper à toute
  mesure. L'oracle lui-même a prouvé l'équivalence : « les tables diffèrent mais **aucun (contexte,
  touche) ne change** » — seul l'ordre d'énumération des touches inertes bouge.
- `keys.rs` (880 lignes) se sépare de ses tests, 448 et 436 lignes.

### Corrigé (#253)
- **Les gestes du pied et de l'oreille répondent maintenant devant une fenêtre ouverte.** Avec la
  banque, le catalogue, « mettre dans la chaîne », le mode pose, le sélecteur ou le choix du set
  ouverts, `q` ne quittait pas, `*` ne tapait pas, `k` n'allumait rien et `1` / `2` / `3` ne
  changeaient pas d'espace : la fenêtre éclipsait l'étage **pédale** en entier. L'ADR 0009 promet le
  contraire — « des gestes physiques, qui répondent **pareil partout** », parce que rester coincé
  est un risque en concert, pas un désagrément.
- Une touche de pédale **referme** la fenêtre, avec l'annulation que cette fenêtre connaît (le
  sélecteur retire le bloc qu'il venait d'ajouter, le mode pose ramène à la Bibliothèque), **puis**
  agit. Elle n'agit donc jamais *derrière* la fenêtre : la règle des modales tient.
- Restent au-dessus de la pédale les **saisies de texte** — une lettre y est une lettre — et les
  **murs** : confirmations, aide, éditeur, sous-modes de l'édition d'un set. Une confirmation à
  laquelle `q` échapperait ne confirmerait plus rien.
- Trouvé en route : le **filtre de la banque** n'était pas compté comme une saisie, si bien que `h`
  y ouvrait l'aide au lieu de s'écrire. Il l'est maintenant.

### Interne (#251)
- **L'ordre du clavier n'est plus écrit qu'à un endroit.** Le code en tenait **deux** copies —
  la cascade de `on_key` et celle de `context()` — et elles ne s'accordaient déjà plus sur trois
  points (la saisie du filtre, le sélecteur sur place, l'aide face à la confirmation de sortie).
  C'était inoffensif **par accident** : rien ne prouvait que les états croisés étaient
  inatteignables. `context()` est désormais la seule autorité d'ordre, et `on_key` un `match` sur
  elle : une ligne par contexte, qui dit qui le traite.
- Le `match` est **exhaustif** : ajouter un contexte sans dire qui le prend ne compile plus — la
  garantie qu'un test approchait à la main. Et le bras d'une modale rendant `Action::None` à défaut,
  **il n'y a plus d'étage en dessous où tomber** : le piège du `?` sur un champ interne à une
  fenêtre, qui faisait agir une touche derrière elle, est devenu inoffensif.
- Les quatre étages de l'ADR 0009 ne concernent plus que les contextes d'**espace** ; l'ADR est
  reformulé en conséquence. Aucune nouvelle abstraction : pas de trait, pas de `Box<dyn>`, pas de
  priorité numérique — un `match` droit à la place d'une chaîne de `if` dupliquée.
- **`keyboard.ref.txt` est strictement inchangé** : c'est tout le contrat de ce déplacement, dans le
  même esprit que les rendus de référence de `dsp`.

### Interne (#252)
- **La jointure du clavier va maintenant dans les deux sens.** « Déclaré ⇒ possédé » était gardé ;
  **« agit ⇒ déclaré »** ne l'était par rien, alors que trois des cinq divergences d'origine étaient
  de cette forme — une touche qui agit sans que la barre du bas ni l'aide `h` la nomment. Le nouveau
  test l'a prouvé en trouvant **cinq fuites réelles** : `b` (avec / sans IR) agissait dans l'onglet
  Mélange sans y être annoncé, `P` refermait la banque sans le dire, et `S` / `P` agissaient dans
  l'espace Set sans y être déclarés. Les quatre sont maintenant dans la table, donc dans l'aide.
- Le prédicat distingue **agir** de **refuser** : un message seul est un refus, et l'ADR 0009 le
  demande (« une touche possédée mais inerte dit pourquoi ») ; une action ou un champ d'état changé,
  c'est un geste, et il doit être annoncé. Trois classes d'exception, nommées dans le test : les
  touches de la **pédale** (elles valent partout, et la barre permanente les donne), les **saisies
  de texte** (toute lettre va au texte) et les **murs** qui se referment sur n'importe quelle touche.
- **Les cinq contextes hors de l'oracle sont mesurés** : la fenêtre de sortie de la scène, la
  confirmation « chaîne vide », la saisie d'un champ de l'éditeur, le filtre du sélecteur, et
  surtout **la sortie d'un parallèle** (`◇`) — 15 touches déclarées, aucune mesurée. Un test vérifie
  désormais que **tout** `Context` a son cas d'oracle : en oublier un, c'est ne rien vérifier de lui.

### Interne (#280)
- **La relecture d'une zone coûte une commande** : la skill de projet `relire-docs`
  (`.claude/skills/`) applique `docs/style-doc.md` à un fichier ou un répertoire et rend un **diff
  proposé**, jamais un commit. Elle est écrite pour un contexte neuf : la marche à suivre, la liste
  de **ce qui se garde** (en-têtes `//!`, règles temps réel, mesures actuelles, renvois `ADR 000x`),
  les genres exemptés, et — c'est le cœur — **comment vérifier chaque fait** avant de le garder :
  un chemin avec `git ls-files`, un nombre dans le code, une mesure en rejouant sa commande.
- **La procédure de publication prévoit une zone relue par version**, en tournant : les `CLAUDE.md`
  de crate, puis `docs/architecture.md` et les ADR, puis le guide et le README, puis `docs/design/`,
  puis les commentaires d'une crate. Les contrôles au commit attrapent la dérive mécanisable ; cette
  relecture attrape le reste.
- **Le coût d'un ajout est inscrit dans `docs/architecture.md`**, parce que c'est lui, et non le
  nombre de lignes, qui mesure une architecture : **6** fichiers pour un effet (tous dans `dsp`,
  avec un `match` et un test qui refusent l'oubli), **6** pour un backend (trois `match` exhaustifs,
  donc le compilateur nomme les endroits), **9** pour un contexte clavier — dont **cinq** que rien
  ne signale. Un écran coûte donc plus qu'un effet, alors que le modèle qui marche est celui des
  effets.

### Interne (#291)
- **La dérive documentaire est refusée au commit, et plus seulement signalée.** Les quatre contrôles
  de style de `scripts/check-docs.sh` — numéro d'issue dans un commentaire, le même dans une chaîne
  littérale, mot-signal temporel, chemin cité qui n'existe plus — **font échouer** `check.sh` et la
  CI. Ils étaient livrés en avertissement le temps des huit nettoyages ; leurs quatre compteurs sont
  à **zéro** (410 occurrences de numéros d'issue et 31 lignes de récit au départ), et chacun est
  vérifié en échec sur un cas construit.
- La taille d'un document reste un **avertissement** : découper `docs/guide.md` est un chantier
  (#297), pas une correction.
- Une exception, et une seule, est inscrite dans le script avec sa raison : les chemins sous
  `library/` et `nam/` sont les fichiers de l'utilisateur, **volontairement non versionnés** (droits
  sur les modèles et IR) et absents de la CI, alors que le guide doit les nommer —
  `library/overrides.toml` est le fichier qu'il édite. Un chemin qui ne peut pas exister à HEAD ne
  se vérifie pas, et n'est donc pas compté comme vérifié.

### Documentation (#270)
- **Connaître le comportement d'un écran demande de lire un fichier, au présent.** Les trois
  spécifications de `docs/design/` se lisaient en pile, dans l'ordre chronologique :
  `browse-espaces.md` « remplace le § 10 de `browse-chaine.md` », `set.md` « ajoute un troisième
  espace ». Chacune dit maintenant **ce qu'elle couvre** en tête, et décrit son écran sans renvoyer
  à l'histoire des autres.
- **Les huit errata datés disparaissent** : la règle est corrigée, pas annotée. Avec eux partent la
  proposition de format TOML conservée **au-dessus** de la décision qui la remplace, la section
  « ce qui disparaît » (des symboles qui n'existent plus), le découpage en US avec ses numéros
  d'issue, et la liste « à confirmer » — devenue les **décisions de forme et leur raison**.
- **Les maquettes HTML redeviennent des illustrations.** Ce qui fait foi pour le comportement est le
  code et l'**oracle du clavier**, qui sont vérifiés à chaque commit ; une maquette n'est vérifiée
  par rien. Les trois fichiers le disent en tête.
- Divergences trouvées en confrontant les spécifications au code, et corrigées : `browse` a **trois**
  espaces, pas deux — l'en-tête, le guide et six commentaires l'annonçaient encore à deux.

### Interne (#279)
- **Les en-têtes de module disent ce que le code garantit, sans se comparer à un code qui n'existe
  plus.** `swap.rs` annonçait « `trails_ms = 0` rend exactement l'ancien comportement », `split.rs`
  « exactement le crossover d'avant », `block.rs` « identique au bit près à celui d'avant les
  blocs » : le fait mesuré — une égalité **au bit près** — se garde, mais rapporté à ce qui existe
  (une chaîne sans parallèle, le crossover à deux bandes de `--crossover`). Le contrat de
  `Stage::tail_ms` et sa justification ne sont plus écrits deux fois.
- **Les 90 derniers numéros d'issue du code disparaissent** : le dépôt en compte **zéro** dans ses
  commentaires, contre 410 occurrences au départ du nettoyage. Les 104 renvois `ADR 000x`, eux,
  restent — ils pointent vers un document vivant. Partent aussi ceux qui s'affichaient : l'erreur de
  `--pre split` renvoyait à « un preset (#6) », et le rapport des sons de clic à « #230 ».
- Les cinq commentaires-titres d'un seul mot de `library` (`// Signal`, `// Cache`, `// Commande`)
  disent ce que leur section contient ; `metronome.rs` et `model::config` nomment le son `bip` au
  lieu de « l'ancien son », en gardant ce qui compte : son attaque de 12,6 ms, qui est la raison
  pour laquelle il se noie dans un retour de basse.
- Aucun changement de comportement : hors les deux textes ci-dessus et des messages d'assertion,
  seuls des commentaires bougent ; les rendus de référence et l'oracle du clavier sont inchangés.

### Interne (#278)
- **Les commentaires de `namfoop` décrivent ce que le code garantit, et non les bugs qui l'ont fait
  naître.** Les sept blocs de récit de `browse/state/keys.rs` — le journal de la mise en place des
  quatre étages du clavier — deviennent des contrats au présent : « cette couche rend un `Option`,
  un `match` exhaustif avalerait les étages du dessous » plutôt que « son ancien `_ => {}` avalait
  tout ». Même traitement pour les en-têtes de `context.rs`, `bindings.rs`, `footer.rs` et
  `windows.rs`, qui racontaient leur propre naissance avant de dire ce qu'ils garantissent.
- **Les 313 numéros d'issue de la crate disparaissent** (le compte du dépôt tombe de 394 à 96
  commentaires, tous dans les crates de bibliothèque), et avec eux les quatre qui s'**affichaient** :
  trois dans la fenêtre d'aide (« les lettres vont au filtre (#175) ») et un sur l'écran Set, en
  concert — la ligne de tempo annonçait « (battue : #223) ». L'historique est dans git et le
  changelog ; les renvois `ADR 000x`, eux, restent.
- Aucun changement de comportement : hors les quatre textes d'interface ci-dessus, seuls des
  commentaires bougent, et l'oracle du clavier (`keyboard.ref.txt`) ne change que par la ligne
  d'en-tête qui portait un numéro.

### Documentation (#275)
- **Le guide dit ce que fait namfoop, et plus comment il en est venu là.** Trois passages
  s'adressaient au développeur plutôt qu'au bassiste : l'alignement de niveau racontait son
  changement de mesure et une pondération écartée, la queue de preset expliquait d'où elle tire sa
  durée, et le volume du preset citait les millisecondes qu'il coûtait avant d'être vivant. Ils
  disent maintenant ce qu'on **entend**.
- Les numéros d'issue disparaissent du guide : un manque s'annonce sans renvoyer à un dépôt que le
  lecteur n'ouvrira pas. Chaque sous-commande, chaque option et les 75 exemples vérifiés par
  `check-docs.sh` sont inchangés.

### Documentation (#277)
- **Les `CLAUDE.md` de crate ne recopient plus le racine.** Les quatre « Carte détaillée (reprise
  du `CLAUDE.md` racine) » qui restaient — `model`, `nam-sys`, `audio-io`, `library` — reproduisaient
  une version **antérieure** de la carte : celle de `model` omettait `card.rs` que le corps du même
  fichier décrit, sur 24 % de son volume. Supprimées ; chaque fichier renvoie à la carte racine et à
  `docs/architecture.md`.
- **`docs/architecture.md` décrit l'architecture sans raconter comment elle a été faite** : la
  section « Où en est-on », instantané périmé par construction, devient une table **des décisions**
  (une ligne par ADR, avec ce que chacune fixe) ; la colonne « Depuis » de la table des règles, dix
  lignes de numéros de PR, disparaît ; et la table dit maintenant exactement ce que
  `scripts/check-arch.sh` vérifie (`dsp` interdit aussi `model`, `audio-io` aussi `serde`).
- Le `CLAUDE.md` racine garde la **règle** de publication — rouler, aligner la version, taguer —
  sans le récit de l'incident qui l'a motivée. Même traitement pour les alternatives abandonnées de
  `crates/library/CLAUDE.md`, dont la mesure reste, au présent.

### Documentation (#276)
- **La règle qui évite de conclure sur une machine lente est écrite une fois.** Le récit de la
  stabilisation de `sim_session_swaps_chains` existait en **trois exemplaires** — deux fois dans
  `crates/engine/CLAUDE.md`, une troisième en 23 lignes de rustdoc. Il reste un contrat de test, au
  présent : référence de même durée, un décrochage toléré si elle est propre, et surtout **le test
  le dit et ne conclut pas** si elle est sale. `crates/dsp/tests/budget.rs` applique la même règle à
  son repère de coût.
- `crates/engine/CLAUDE.md` passe de 154 à 108 lignes : la section API ne recopie plus en Markdown
  le rustdoc des modules (qui donne unités, défauts et garanties temps réel) et devient une carte —
  quoi vit où, et ce qu'il ne faut pas casser. Les faits mesurés de la banque préchargée et des
  dénormalisés sont inchangés.

### Documentation (#274)
- **`crates/dsp/CLAUDE.md` dit ce que la crate garantit, et non comment on y est arrivé.** Le
  paragraphe « Fichiers / API » de 80 lignes devient une **liste**, un fichier par ligne avec son
  rôle et ses invariants — le contrat détaillé reste dans le rustdoc, qui le porte déjà. Les coûts
  par bloc, jusque-là éparpillés en huit mesures isolées, tiennent dans **un tableau** avec leurs
  conditions.
- Disparaissent : la « Carte détaillée » recopiée du `CLAUDE.md` racine (périmée : elle citait
  `preset.rs`), les colonnes « avant / après » qui mesuraient du code supprimé, la pondération A
  « mesurée et écartée », la régénération d'un rendu de référence racontée comme un événement, et
  les 42 lignes qui ne portaient qu'un numéro d'issue.
- Restent intactes : les **règles temps réel non négociables**, la procédure **« Ajouter un
  effet »** et les tables de mesures actuelles (queue de preset, loudness pondérée K, coût de
  plusieurs modèles, RT60, divisions).

### Documentation (#273)
- **Les écrans ont leur propre fichier** : `crates/namfoop/src/browse/CLAUDE.md` (263 lignes) porte
  les espaces, le clavier et ses quatre étages, l'oracle, la chaîne éditée, les presets, le set et
  le dessin ; Claude Code ne le charge que quand on travaille dans ce répertoire. Le `CLAUDE.md` de
  la crate retombe à 95 lignes (514 avant) et ne parle plus que de la CLI, des commandes et de
  `setup`.
- Ce qui disparaît au passage : l'**annexe de 169 lignes** qui recopiait une version périmée du
  `CLAUDE.md` racine (33 % du fichier), le bloc « Tester » écrit **deux fois** mot pour mot, les
  16 lignes de rendus de référence qui appartiennent à `crates/dsp/CLAUDE.md`, et les 63 lignes qui
  ne portaient qu'un numéro d'issue. Les contrats, eux, sont tous conservés — reformulés au présent.

### Interne (#272)
- **`scripts/check-docs.sh` vérifie les règles mécanisables de `docs/style-doc.md`** (étapes 9 à
  12) : un numéro d'issue dans un commentaire ou un rustdoc, un mot-signal temporel dans un
  commentaire, dans `docs/` ou dans un `CLAUDE.md`, un chemin cité qui n'existe plus à HEAD, et la
  taille d'un document. Une règle non vérifiée redérive ; une règle vérifiée tient derrière le
  nettoyage qui la suit.
- Les trois premiers contrôles sont **provisoirement des avertissements** — le dépôt compte 410
  numéros d'issue dans 394 commentaires et 31 lignes de récit —, parce que le cycle de
  développement exige une CI verte pour fusionner. Le compte affiché mesure l'avancement des
  nettoyages ; une dernière PR les passe en échec quand il tombe à zéro.
- Le contrôle de **référence périmée** annonce combien de chemins il a vérifiés (153 aujourd'hui) :
  il ne trouve rien, et c'est ce chiffre qui le distingue d'un contrôle qui ne regarde rien.
- Les numéros d'issue cachés dans une **chaîne littérale** sont comptés à part (17), parce que cinq
  d'entre eux s'affichent : le champ `what` de la table des touches va dans la fenêtre d'aide, un
  message d'erreur de `FxSpec` va au terminal, et la ligne de tempo de l'écran Set montre
  « battue : #223 » en concert.
- Trois exemptions, chacune pour une raison de genre : le `CHANGELOG.md` **est** de l'histoire, un
  ADR dit ce qui motive sa décision et se supersède plutôt que se réécrire, et `docs/style-doc.md`
  doit nommer les mots qu'il proscrit. `docs/cli.md` est engendré : il n'est pas compté.

### Documentation (#271)
- **[`docs/style-doc.md`](docs/style-doc.md)** : les règles de ce qu'on écrit à côté du code
  (commentaires, rustdoc, `CLAUDE.md`, `docs/`), cité par le `CLAUDE.md` racine. Onze règles, chacune
  ancrée sur un cas réel du dépôt, plus la liste de **ce qui se garde** — sans laquelle un nettoyage
  zélé jette les contrats avec le récit.
- Le principe tient en une question : *existe-t-il un endroit qui porterait ce fait mieux que du
  texte ?* Un fait qui se vérifie va dans ce qui le vérifie, un savoir-faire qui s'exécute dans ce
  qui l'exécute, un nombre qui bouge dans ce qui l'engendre. Le texte n'est le meilleur endroit que
  par défaut.

### Interne (#268)
- **Les écrans plein écran sont testés de bout en bout** (`scripts/check-e2e.py`, lancé par
  `scripts/check.sh`) : `browse` est piloté dans un pseudo-terminal, sur la carte simulée et une
  bibliothèque jetable (le modèle d'exemple du sous-module NAM et une IR de synthèse). Jusqu'ici la
  technique était décrite en 40 lignes de prose et **aucun script n'existait** — autrement dit, la
  boucle d'événements (`browse/mod.rs`, 855 lignes, 1 test) et le thread qui décide de ce qu'on
  entend (`browse/build.rs`, 1 test) n'étaient exercés par rien.
- Les assertions portent sur la **trace** (`NAMFOOP_BROWSE_TRACE`), c'est-à-dire sur la chaîne
  réellement entrée dans le thread audio, et non sur l'écran : `↓` fait jouer « modèle › IR », puis
  **`b` coupe l'IR et garde le modèle**. Plus deux garanties de fiabilité scène : `q` **avertit** au
  lieu de quitter quand la chaîne est vide alors que le banc joue, et `Q` quitte quand même. Aux
  deux tailles d'écran du projet (80×25 et l'écran du Pi en 160×50).
- Les pièges du pilotage (un zombie répond à `os.kill`, un emoji occupe deux cellules, ratatui ne
  dessine rien sans `TIOCSWINSZ`, `Terminal::clear` attend une réponse du terminal) sont désormais
  **implémentés** dans le harnais au lieu d'être décrits dans un `CLAUDE.md`.

### Interne (#269)
- **Les frontières annoncées sont maintenant celles qui sont vérifiées.** `docs/architecture.md`
  déclarait cinq interdictions dont trois ne tenaient pas : « `dsp` interdit la config » et
  « `audio-io` interdit la config » étaient **fausses** telles qu'écrites (`dsp` lit et écrit ses
  propres formats — presets, mélanges d'IR —, et `audio-io` lit `/proc/asound` pour sonder les
  cartes), et « `dsp` interdit les threads » était vraie mais non contrôlée. Une règle
  aspirationnelle qu'un agent lit comme vraie coûte plus qu'une règle absente : elle oriente le
  travail de travers.
- Ce qui est vrai **et** désormais mécanisé : `dsp` ne dépend pas de `model`, donc le type `Config`
  lui est inaccessible — c'est ça, « `dsp` ne connaît pas la config » ; et `dsp` ne crée aucun
  thread, c'est à l'appelant de décider où le DSP tourne. Les deux refus ont été éprouvés en les
  violant.
- `scripts/check-arch.sh` dit maintenant dans son en-tête qu'il ne lit que les arêtes **directes**,
  et pourquoi c'est l'intention : `model` ne ré-exporte pas `toml`, donc `engine` ne peut pas s'en
  servir sans le déclarer.
- Le contrôle de taille de `scripts/check-docs.sh` exemptait `tests/` tout en attrapant
  `src/.../tests.rs` : un fichier de 764 lignes passait quand un autre de 730 était signalé. Les
  fichiers de test comptent désormais aussi — la règle porte sur ce qu'on lit d'un bloc, pas sur la
  nature du code.

### Interne (#267)
- **La cible du Raspberry Pi 5 est compilée par la CI** (`scripts/check-aarch64.sh`, job `aarch64`),
  alors que rien ne la compilait : `crates/audio-io/src/rt.rs` **et** `crates/engine/src/rt.rs`
  portent chacun un bloc d'assembleur aarch64 (le flush-to-zero du processeur, dans le chemin temps
  réel) qu'aucune compilation x86 ne regarde. Une faute y aurait été découverte sur le Pi.
- C'est un `cargo build` et non un `cargo check` : le contenu d'un `asm!` n'est donné à
  l'assembleur qu'à la **génération de code**, si bien qu'un `check` passe sans broncher sur une
  instruction qui n'existe pas (vérifié en cassant les deux blocs).
- Aucun paquet système en plus : le C++ NAM n'est pas compilé pour la cible (`NAMFOOP_SKIP_CPP`, une
  bibliothèque ne se liant pas) et `pkg-config` rend les drapeaux de l'ALSA de l'hôte
  (`PKG_CONFIG_ALLOW_CROSS`), qui ne servent qu'à lier. Seule la cible Rust est nécessaire.

### Interne (#266)
- **Un budget de coût par bloc, vérifié par un test** (`crates/dsp/tests/budget.rs` +
  `tests/budget.toml`) : la latence et la charge sont la priorité n°1 annoncée du projet, et rien ne
  les vérifiait — une régression de 40 % passait la CI au vert. Quatre chaînes de référence, blocs
  48 et 64, lancé par `scripts/check.sh` et par la CI.
- Le seuil est un **rapport à un repère mesuré dans la même exécution**, pas des microsecondes :
  mesuré sur cinq exécutions, le rapport d'une chaîne varie de **6 % au pire** quand ses µs varient
  de **40 %** avec la montée en fréquence du processeur. Le test **ne conclut pas** s'il n'est pas
  compilé optimisé, ni sur une machine dont le repère disperse de plus de 50 % : un runner surchargé
  ne doit pas faire échouer une PR pour une raison qui n'est pas le code.

### Supprimé (#263)
- **Le backend JACK est retiré.** Il n'était ni utilisé ni testé, et il imposait les en-têtes
  `libjack` à la compilation de **tout** le projet — sur chaque poste, en CI, et pour la future
  cross-compilation vers le Raspberry Pi. Restent **ALSA direct** (la scène) et la **carte simulée**
  (démo et tests sans matériel). Le backend par défaut devient donc `alsa`.
- **`namfoop ports` disparaît** : cette commande ne listait que des ports JACK.
- **`--client-name` disparaît** : elle ne nommait que le client JACK, et plus personne ne la lisait.
- Une config qui porte encore `backend = "jack"` n'est pas un plantage : elle dit la ligne en cause
  et quoi mettre à la place.

### Documentation (#264)
- **[ADR 0010](docs/adr/0010-retrait-de-jack.md)** : un seul backend de scène, ALSA direct, plus la
  carte simulée. Il **remplace l'ADR 0001**, qui gardait JACK « pour le développement et les essais »
  — l'ancien texte est conservé tel qu'adopté, avec un lien dans les deux sens, et dit ce que coûte
  de rajouter un backend pour que la question soit close.
- Les ADR 0001 à 0003 portent enfin une ligne `Statut`, comme les suivants : on ne pouvait pas savoir
  au premier regard si l'ADR 0001 était encore en vigueur.
- La règle « tout changement audio doit marcher sur **JACK et ALSA** » devient « **ALSA** (avec la
  carte) et **sim** (sans matériel) » dans ses cinq emplacements : `CLAUDE.md`,
  `crates/audio-io/CLAUDE.md`, le gabarit de PR, celui d'issue, et le corps de PR engendré par
  `scripts/feature.sh`.
- Le guide : la section « PipeWire (pour le backend JACK) » et ses réglages wireplumber n'avaient
  plus d'objet (ALSA direct ne passe pas par PipeWire). Elle dit maintenant ce qui reste vrai — la
  carte est rendue puis restaurée toute seule — et la seule commande encore utile, celle qui répare
  un profil resté à « off » après un plantage dur.
- Le tableau des backends, les prérequis de compilation (plus de `libjack-jackd2-dev`, de
  `pipewire-jack` ni de `pw-jack`) et les tableaux de mesures comparées de
  `crates/audio-io/CLAUDE.md`, dont il ne reste que les chiffres ALSA qui servent encore.

### Interne (#263)
- `audio_io::Kind` dérivait `Serialize` / `Deserialize` pour personne : le derive et la dépendance
  `serde` quittent `audio-io`, qui ne dépend plus que d'`alsa`, `libc`, `log` et `thiserror`. La
  règle « `audio-io` ne connaît pas la config » devient vérifiée par `scripts/check-arch.sh` au lieu
  d'être seulement écrite.

### Corrigé (#262)
- **La documentation annonçait des fonctions absentes alors qu'elles sont livrées** : le guide
  (§ 17 « Limites actuelles », § 10, § 11) et le README présentaient namfoop comme un prototype
  « sans presets », que le guide documente pourtant depuis deux versions. Ce qui manque vraiment
  est maintenant seul cité : MIDI, micro, démarrage automatique, Raspberry Pi.
- Le guide annonçait **12 effets** au catalogue (il y en a onze) et l'architecture **52 touches dans
  21 contextes** (56 et 26). Les comptes recopiés à la main sont remplacés par ce qui reste vrai :
  l'oracle mesure **chaque** touche dans **chaque** contexte, et le § 10 détaille les effets un par un.
- Chemins périmés dans les documents de référence : `preset.rs`, `send.rs`, `space.rs`, `chain.rs`,
  `state.rs` et `ui.rs` sont devenus des répertoires ; `card.rs` manquait à la carte de `model`.

## 0.6.0 — 2026-11-19

**Le mode Set.** Une set list de concert : des morceaux dans l'ordre, un à plusieurs sons par
morceau, changés au pied avec `PgDn` / `PgUp`. Un troisième espace de `browse` montre la scène,
s'édite en place (`e`) et affiche le **prompteur** du morceau joué — accords, paroles et cues lus
d'un fichier ChordPro, dont le texte suit le son. `run --set` joue tout cela verrouillé : rien ne
s'y édite. Dessous, le clavier de `browse` tient sur **quatre étages avec un propriétaire par
touche** (ADR 0009), mesurés par un oracle.

### Ajouté (#222)
- **Le prompteur** : accords, paroles et cues du morceau joué, à côté de son son. Le texte est un
  fichier **ChordPro** (`library/sets/prompts/<morceau>.cho`), le format de fait des logiciels de
  set list — namfoop le **lit** et ne l'écrit pas : le texte d'un concert ne doit pas être
  prisonnier d'un seul logiciel.
- Sections étiquetées (la courante en vert avec `▸`, les passées en gris, les suivantes en gras,
  avec leur durée en mesures et le son auquel elles sont liées), accords **au-dessus** des paroles
  quand la largeur le permet, cues en jaune, ligne courante surlignée. Le texte part deux lignes
  avant la position.
- **Le texte suit le son** : `{x_namfoop_sound: <preset>}` lie une section à un son du morceau, et
  jouer ce son amène le prompteur à la **prochaine** section qui lui est liée.
- **`Fin` / `Début`** : section suivante / précédente, **sans toucher au son** — pour les morceaux
  dont le texte et les sons ne suivent pas le même découpage. Réglables (`[set] prompt_next` /
  `prompt_prev`), et permises sur scène (`run --set`) : elles ne changent rien.
- **`t`** ouvre le `.cho` dans `$EDITOR` (en le créant avec son titre et en renseignant `prompt =`
  dans le morceau), puis le relit au retour. Pas sur scène : rien ne s'y édite.
- Tout ce que namfoop ne comprend pas d'un fichier ChordPro est **ignoré sans erreur**, et un
  prompteur absent ou illisible est dit dans le cadre : un concert ne s'arrête pas pour un fichier.

### Ajouté (#220)
- **Éditer un set à l'écran** (`e` dans l'espace Set) : le cadre passe en jaune `✎ ÉDITION` et prend
  toute la largeur — PC, tempos, sons et volumes de **tous** les morceaux à la fois. Un set se bâtit
  en écoutant : on met un preset dans un morceau, on l'entend, on le remplace.
- Les touches du § 8 : `a` ajouter un morceau (titre tapé tout de suite, **premier PC libre**
  proposé), `r` renommer, `T` tempo (vide : celui de ses presets), `p` n° de Program Change (refusé
  s'il est pris, **en nommant** le morceau qui l'a), `X` retirer (confirmation), `[` `]` déplacer ;
  `+` ajouter un son et `Entrée` remplacer son preset (la liste de la banque, `↑↓` l'**essaient** et
  on l'entend, `Échap` défait l'essai et rend le son joué), `x` retirer, `{` `}` déplacer (change sa
  valeur de CC 69), `,` `.` son volume propre (±0,5 dB, entendu tout de suite).
- **Viser n'écoute pas** : les flèches déplacent le cadre jaune sans rien changer à l'oreille. Et
  une saisie passe avant toutes les touches : dans un titre, `a`, `q` et `1` sont des caractères.
- **`S` enregistre le set** (`library/sets/<nom>.set.toml`) : il éclipse le `S` du preset, ici
  seulement. Le `●` reste tant que ce n'est pas écrit, `q` passe par la fenêtre « non enregistré »,
  et un set vide, un morceau sans son ou un set sans nom sont refusés — ils ne se joueraient pas.
- **`L` › `＋ nouveau set…`** crée un set vide : il demande son nom (celui du fichier) et ouvre
  l'édition dessus. Enregistré, il devient le set retenu (`[set] default`).
- **`2` depuis l'espace Set** mène à la Chaîne du son joué, et son titre rappelle `preset utilisé
  par N morceaux de ce set` : `S` y enregistre le preset, donc partout où il est joué.
- L'édition est **un écran à elle** : `i`, `L` et `P` y dorment (`L` changeait de set en abandonnant
  les modifications sans rien demander, même famille que #249). `PgDn` / `PgUp` répondent toujours —
  on édite en écoutant — comme `1 2 3`, `*`, `k`, `h` et `q`.

### Corrigé (#250)
- **`n` ne vide plus la chaîne depuis l'espace Set.** Touche commune, elle y vidait la chaîne,
  détachait le preset **et quittait l'écran de concert** — sans rien demander, puisqu'il n'y avait
  rien de non enregistré. L'espace Set se l'approprie et renvoie à la Chaîne (« n bâtit une
  chaîne : 2 pour la Chaîne »). Dans la Chaîne, `n` fait toujours son travail : l'éclipse est locale.

### Corrigé (#249)
- **Les réglages d'un effet n'éditent plus la chaîne derrière leur panneau.** `Retour` et `x` y
  retiraient le bloc dont on réglait justement les paramètres, `s` le mettait en parallèle, `a` et
  `i` ouvraient le catalogue, `n` vidait la chaîne — depuis un écran qui ne montre qu'une liste de
  valeurs. Le panneau refuse maintenant ce qui **édite** et le dit (« rien ne s'édite depuis les
  réglages : Échap revient à la chaîne »).
- Ce qui **écoute** ou **montre** le traverse, puisque c'est ce qu'on fait en réglant : le volume du
  preset (`,` / `.`), l'A/B sans effets (`r`), les vumètres (`v`), et `S` / `P`.
- `PgDn` / `PgUp` — le geste du pédalier — ne se bloquent **jamais** : ils changent de preset depuis
  les réglages, et le panneau se referme, son bloc ayant disparu avec la chaîne.

### Corrigé (#247)
- **Les espaces sont indépendants.** Dans l'espace **Chaîne**, `PgDn` / `PgUp` changeaient de
  morceau du set dès qu'un set était actif : on ne pouvait plus travailler à la maille preset dans
  l'espace fait pour ça. Ils parcourent de nouveau la **banque**, en boucle, set actif ou non. Le
  set ne se joue que dans **son** espace et sur scène (`run --set`).
- La banque **préchargée** reste celle du set : c'est en Set et sur scène que la bascule doit être
  instantanée. Dans la Chaîne, un preset hors du set se charge par l'ancien chemin (~80 ms, sans
  coupure) — on y travaille, on n'y joue pas.
- Sans set, `PgDn` / `PgUp` dans l'espace Set disent « aucun set : L pour en choisir un » au lieu
  de ne rien faire.

### Modifié (#243)
- **Les touches ne sont plus déclarées qu'une fois.** La barre du bas et l'aide `h` sont
  **dérivées** d'une table (`state/bindings.rs`) : une touche existait jusqu'à **six fois** (le
  dispatch, la barre, l'aide, la liste blanche du verrou, son message de refus, et une liste de
  30 touches dans un test). Deux tests font la jointure : **tout ce qui est annoncé est possédé**
  (c'est ce qui rend #238 impossible plutôt qu'improbable) et **la scène est incluse dans l'espace
  Set**.
- Cinq divergences annoncé/traité sont fermées : `S` n'est plus annoncé dans la banque (qui
  l'avale), **`3` (le Set) est enfin annoncé** dans la Bibliothèque et la Chaîne, `?` et la touche
  de **morceau** sont nommés dans l'aide, et le commentaire qui promettait `L` « comme ailleurs »
  dit maintenant la vérité.
- `run --set` : le message de refus liste les touches qui marchent **depuis la table**, donc avec
  leur libellé complet (`i détail`, `k clic`, `h aide`, `q quitter`) au lieu de `i · * · q`.

### Corrigé (#243)
- **Sans set, l'espace Set n'était plus maître de ses touches de morceau** : elles étaient
  annoncées et possédées par personne. Elles disent maintenant « aucun set : L pour en choisir un »,
  comme `↑↓`, `Entrée` et `i`. Trouvé par la jointure dès sa première exécution.

### Modifié (#242)
- **Le clavier de `browse` a quatre étages** ([ADR 0009](docs/adr/0009-clavier-de-browse.md)) : la
  **pédale** (`1 2 3 q h * k`, qu'un espace ne peut pas éclipser — se retrouver coincé dans un
  espace ou sans tap tempo est un risque en concert), **l'espace** où l'on est, le **commun**
  (`S P n`, éclipsable par déclaration) et le **dernier recours** (`Échap`). L'ordre est écrit en
  entier dans `on_key`, en un seul endroit.
- L'espace passe maintenant **avant** les communes : c'est ce qui permettra à l'espace Set de
  s'approprier `S` (« enregistrer le set ») en #220, sans brûler une des 8 lettres encore libres.

### Corrigé (#242)
- **Une fenêtre ouverte prend toutes les touches.** Avec la fenêtre `L` (choisir le set) ouverte,
  `n` **effaçait la chaîne derrière elle**, et `P`, `S`, `1`, `2`, `3`, `*`, `k`, `q` agissaient
  aussi : elle vivait dans le clavier de l'espace Set, donc **sous** les touches communes. Les
  fenêtres se classent désormais par leur priorité propre, pas par l'espace qui les tient.
- Une touche que **personne** ne possède est écrite dans `NAMFOOP_BROWSE_TRACE` (« touche sans
  propriétaire ») et **rien ne s'affiche** : sur scène, « j'ai appuyé et rien ne s'est passé »
  devient diagnosticable sans apprendre à l'utilisateur à ignorer la zone de message.

### Corrigé (#241)
- **Les touches d'un espace n'agissent plus dans un autre.** La dernière couche du clavier était en
  réalité celle de la **Bibliothèque**, mais sans garde d'espace : toute touche qu'un espace ne
  consommait pas y retombait et agissait sur un écran qu'on ne regarde même pas. Depuis la vue
  scène, `b` **coupait l'IR du son joué**, `e` ouvrait l'éditeur de modèle par-dessus la set list,
  `/` lançait un filtre invisible et `Tab`/`←→` changeaient un onglet caché ; depuis la Chaîne, `/`
  et `e` faisaient de même. C'était la cause de fond de #238, #207 et #175.
- `Échap` et `q` restent valables partout : ce sont les deux sorties.

### Ajouté (#241)
- **L'oracle du clavier** : un test qui mesure, pour chacun des **21 contextes** et des
  **52 touches**, ce qui se passe réellement — quelle couche consomme, quelle action, quel message,
  quels champs d'état changent — et le fige dans `keyboard.ref.txt`. Toute évolution du clavier
  devient un **diff lisible** au lieu d'une surprise. Régénérer :
  `NAMFOOP_UPDATE_KEYBOARD=1 cargo test -p namfoop --bin namfoop keyboard`.

### Corrigé (#238)
- **`PgDn` / `PgUp` ne jouaient pas le set dans l'espace Set de `browse`** — l'espace où la touche
  du pied compte le plus. Elles tombaient sur le défilement de la liste de la Bibliothèque, qui ne
  joue rien et ne se voit pas, alors que la barre du bas annonçait « PgDn/PgUp son ». Les touches de
  morceau suivant / précédent avaient le même trou. `run --set` n'était pas touché (sa liste de
  touches verrouillée les liait explicitement). Les autres espaces ne changent pas.

### Ajouté (#221)
- **`namfoop run --set <set>` : la scène verrouillée.** Le même écran de scène que `browse`, privé
  de tout ce qui édite : en-tête `namfoop run  🔒 SET  <nom>`, et seuls les gestes de scène
  répondent (`PgDn`/`PgUp`, `↑↓ Entrée`, `i`, `*`, `k`, `h`, `q`). Toute autre touche est refusée
  avec la liste de celles qui marchent — en concert, une touche de trop ne doit pas pouvoir
  enregistrer, charger ou vider quoi que ce soit.
- `run --set` **sans valeur** prend le set retenu (`[set] default`). Set absent, introuvable ou sans
  aucun son jouable : message clair et **code d'erreur**, émis **avant** de prendre la carte son —
  c'est ce qu'il faut pour le service de démarrage automatique du Pi.
- `q` sur scène demande confirmation (`Quitter la scène ? Q quitter · Échap rester`), sans proposer
  d'enregistrer : il n'y a rien à enregistrer là.
- `--set` et `--preset` s'excluent, avec le message qui le dit.

### Ajouté (#219)
- **L'espace Set : la vue scène.** `3` ouvre un troisième espace (en vert) : la **set list à
  gauche**, le **cadre du morceau à droite**. La colonne de gauche donne l'essentiel par morceau
  (numéro, titre, tempo, mesure si elle n'est pas 4/4, `≡` s'il a un prompteur), déplie les sons du
  morceau joué (`●` celui qu'on entend, `○` les autres) et commence **un morceau avant** celui qu'on
  joue, pour montrer ce qui vient. Le cadre de droite dit **ce que fera le prochain appui** :
  `2 · Le Phare · son 1/2 fuzz · PgDn : ensuite clean`.
- **`↑` `↓` visent, `Entrée` joue** : parcourir la set list pendant un morceau ne produit aucun son.
  C'est ce qui permet de préparer le morceau suivant sans toucher à ce qu'on entend.
- **`i`** déplie le détail du morceau visé : son PC, la valeur de CC 69 de chaque son, son volume
  propre, son prompteur. Plusieurs morceaux peuvent rester dépliés.
- **`L`** choisit le set, ou `aucun set` qui rend la banque entière. Le **set actif est retenu**
  d'une session à l'autre (`[set] default`) : `browse` rouvre le dernier set choisi.
- **Terminal étroit** : sous 100 colonnes la set list se réduit, sous 70 elle disparaît au profit
  d'une ligne qui garde l'essentiel. Vérifié en 60×25, 80×25, 110×33 et 160×50.

### Ajouté (#218)
- **Un set se joue au pied** : `namfoop browse --set <id | mots-clés>`, puis `PgDn` / `PgUp` suivent
  le set — son suivant, puis le premier son du morceau suivant. **Pas de bouclage** : sur le dernier
  son, `PgDn` ne fait rien et dit `fin du set`, parce que dépasser la fin d'un concert ne doit pas
  rejouer le premier morceau.
- Chaque son apporte la chaîne de son preset (préchargée : bascule en 20 ms, queue de réverbe
  gardée), le **volume du morceau** qui s'ajoute à celui du preset, et le **tempo du morceau** qui
  prime sur le sien — un tap le recouvre ensuite.
- **Seuls les presets du set sont préparés, une place par preset distinct.** Mesuré sur un set de
  deux morceaux et quatre sons : trois places pour trois presets, toutes préchargées, et passer
  d'un morceau à l'autre sur le **même** preset ne rebâtit rien (seuls le volume et le tempo
  changent).
- Un son dont le preset est introuvable est **sauté** : le set a été contrôlé, le son est marqué, et
  le concert continue.
- Touches de scène réglables dans `[set]` de la config (`next_song`, `prev_song`) pour coller au
  pédalier ; elles agiront dans l'espace Set, qui vient avec l'écran.
- **Sans set, rien ne change** : la banque entière en boucle, `P`, `S`, `n` exactement comme avant.

### Ajouté (#217)
- **Les sets de concert existent sur disque** : `library/sets/<nom>.set.toml` liste les morceaux
  d'un concert dans l'ordre, chacun avec ses sons (des presets **référencés**, non copiés), son n°
  de Program Change, son tempo, sa mesure et son prompteur. Un show à trois sons qu'on alterne est
  un set à un seul morceau sans titre. Le format est documenté dans le guide, section « Le mode
  Set ».
- **`namfoop sets`** liste les sets : identifiant, nom, nombre de morceaux et de sons — et surtout
  **ce qui ne va pas**. La distinction compte avant une scène : une **erreur** empêche le set de
  s'ouvrir (le même PC sur deux morceaux, un tempo hors 30-300, une mesure illisible) parce qu'elle
  rendrait la navigation imprévisible ; un **avertissement** le laisse jouer et marque seulement ce
  qui manque (un preset renommé, un prompteur absent) — un concert ne s'arrête pas pour ça.
- Premier pas de l'épique #215 : jouer un set au pied, l'écran de scène, le prompteur et la battue
  viennent ensuite.

## 0.5.0 — 2026-11-15

**Le tempo.** Les effets de temps se règlent en divisions rythmiques, chaque preset porte son
tempo, `*` le tape, un clic de métronome peut partir dans les in-ears, et la queue d'un preset
quitté dure exactement ce qu'elle doit. Plus le volume et le tempo devenus **vivants** (réglables
sans rebâtir la chaîne) et la banque de presets préchargée (bascule en 21 ms).

### Corrigé (#230)
- **Le clic de métronome est percussif.** Son enveloppe montait en **12,6 ms** (mesuré), ce qui en
  faisait un bip tonal, noyé sous une basse au lieu de la percer. Trois sons sont désormais
  proposés, `sound` dans `[metronome]` : **`bois`** (défaut — bloc de bois, attaque en 0,6 ms,
  éteint en 25 ms, et il tient sa hauteur donc l'accent s'entend), **`tic`** (bruit filtré, attaque
  en 0,3 ms, le plus sec) et **`bip`** (l'ancien son, gardé pour qui le préfère). L'accent reste
  plus brillant quel que soit le son.

### Ajouté (#201)
- **Le clic du métronome s'allume** : `k` dans `browse`, `--click` pour `run`. Éteint au démarrage,
  toujours, et il ne s'enregistre pas dans un preset — c'est une aide d'écoute, pas un son. Allumé,
  l'en-tête l'affiche à côté du tempo : `♩ 104 (tap) ·clic`.
- **Il va dans les sorties déclarées « in-ear »** par `namfoop setup`, et nulle part ailleurs : c'est
  la même information que l'assistant demande déjà, donc il n'y a rien de plus à régler, et le clic
  ne peut pas atterrir dans le sondier.
- Quand **aucune sortie ne le reçoit**, `k` le dit et renvoie à `setup` au lieu d'allumer un clic
  inaudible ; `run --click` refuse de démarrer plutôt que de jouer sans clic en silence.
- Réglages dans `[metronome]` de la config : `level_db` (-18) et `beats_per_bar` (4, accent sur le
  premier temps).

### Ajouté (#197)
- **Le socle du métronome.** Une sortie peut recevoir le **clic** en plus de sa source
  (`click = true` dans `[[routing.outputs]]`) : les in-ears veulent la basse **et** le clic, donc ce
  n'est pas une source de plus. Section `[metronome]` dans la config (`level_db`, défaut -18 ;
  `beats_per_bar`, défaut 4, avec accent sur le premier temps). Salve de 25 ms, 1 kHz et 2 kHz sur
  l'accent, enveloppe douce — aucun clic parasite.
- **Aucune sortie ne reçoit le clic sans l'avoir demandé**, et si aucune ne le demande il n'est même
  pas calculé : un clic parti dans le sondier en concert est une catastrophe, et c'est vérifié par
  un test. Éteint, la sortie est identique **au bit près**.
- Le clic suit **la même case de tempo** que les effets de temps : il suivra donc le tap et
  l'horloge MIDI. Éteint, la grille des temps continue d'avancer, si bien que rallumer tombe juste.
  Rien n'est encore réglable depuis l'application : c'est #201.
- Une config écrite par une version précédente se relit et se réécrit **à l'octet près** (le
  drapeau et la section ne s'écrivent que s'ils servent), et `setup` ne l'efface pas.

### Corrigé (#225)
- **La queue d'un preset garde son propre tempo.** Un `delay` en `1/8` avec beaucoup de réinjection
  à 60 BPM voyait ses répétitions **s'accélérer** en pleine décroissance — passant de 500 à 250 ms —
  quand on basculait vers un preset à 120 BPM : la case de tempo est unique et partagée par toutes
  les chaînes audibles, queue comprise. C'est ce qu'on veut pour un **niveau** (le volume suit, aucun
  saut pendant le fondu) et faux pour un **rythme** : la queue appartient au preset qu'on vient de
  quitter. Elle cesse donc de suivre le tempo dès le début du fondu et garde sa tête de lecture,
  comme sur une pédale à trails.

### Modifié (#173)
- **La queue de preset demande sa durée à la chaîne** au lieu de l'écouter décroître. Chaque étage
  dit ce qu'il peut encore sonner (`Stage::tail_ms`) : une série **somme** les queues de ses étages,
  un parallèle prend le **maximum** de ses branches, et un gain, un EQ, un gate ou un modèle NAM
  n'annoncent rien. Conséquences mesurées, bloc 64 à 48 kHz :
  - une chaîne **qui ne sonne pas** (`nam › ir`) n'est plus gardée en vie : **521 ms → 23 ms**, donc
    plus de double charge DSP pour rien à chaque bascule de preset ;
  - une **réverbe** n'est plus coupée à une seconde : elle obtient ses **1,1 s** annoncés ;
  - un **delay lent** n'est plus coupé entre deux répétitions (la fenêtre de silence de 500 ms se
    refermait entre elles) — ce qui comptait d'autant plus depuis les divisions rythmiques, où le
    temps n'est plus connu d'avance.

  Vérifié par la mesure pour chaque effet à queue (`render`, impulsion puis silence, seuil
  -60 dBFS) : la durée annoncée couvre toujours la décroissance réelle, de la réverbe à 5 s de
  decay au delay à 90 % de réinjection.

### Corrigé (#130)
- Le test d'échange de chaînes de la carte simulée ne décrète plus une régression quand c'est la
  machine qui a lâché une période : il échouait **3 fois sur 12** au repos, et bloquait la CI comme
  le hook de pre-commit au hasard. La fenêtre de référence dure désormais aussi longtemps que la
  phase d'échange (elle faisait la moitié), et un décrochage isolé est toléré quand la référence est
  propre — un vrai défaut décrocherait à chaque échange. **15 / 15** après correction, et deux
  décrochages simulés le font bien échouer.

### Corrigé (#207)
- **Le tap (`*`) n'agit plus dans le banc d'essai**, seulement dans l'espace Chaîne : la
  Bibliothèque n'est pas un preset, elle n'a ni volume ni tempo propres, et y taper faisait
  apparaître « non enregistré » alors qu'on écoutait seulement des modèles. La touche dit où elle
  agit.
- **Le panneau de réglages dit ce que vaut une division.** Une fraction nue se lit mal : `1/4` est
  une **noire**, donc **un temps**, et non « un quart de temps » (les fractions sont relatives à la
  ronde). La ligne d'aide, sous le curseur, l'écrit désormais en clair —
  `1/4 = une noire, soit un temps (500 ms à 120 BPM)` — et le guide ajoute la colonne des noms de
  notes. Aucun calcul ne change : à 120 BPM, `1/4` valait et vaut 500 ms, comme sur tout le marché.
- La durée dérivée affichée tient dans la colonne de 9 caractères (`(2000 ms)`) ; c'est la ligne
  d'aide, qui a la place, qui explique l'écrêtage au tampon de 2 s.

### Ajouté (#200)
- **Le tap tempo : `*` dans `browse`.** Deux frappes suffisent, puis c'est la moyenne des quatre
  derniers intervalles ; une frappe trop écartée repart une séquence, trois secondes la terminent.
  Taper pendant que le delay sonne **ne reconstruit rien** et ne coupe aucune répétition.
- **L'en-tête dit le tempo et d'où il vient** : `♩ 104 (tap)`, `♩ 92 (preset)` — sans cette
  mention, on ne saurait pas pourquoi le delay ne suit pas ce qu'on croit avoir réglé. Une horloge
  MIDI perdue s'affichera en jaune, le tempo étant gardé.
- Un tempo tapé **modifie le preset**, comme tout autre réglage : l'étoile « non enregistré »
  apparaît, `S` l'enregistre, et passer au preset suivant restaure le tempo de celui-là.
- Dans le panneau de réglages, un paramètre remplacé par une division suit le tempo **entendu** :
  `(489 ms)` à 92 BPM, `(375 ms)` à 120.

Mesuré en pseudo-terminal (backend `sim`, bibliothèque jetable) : charger un preset à 92 BPM écrit
`tempo=92.0 (preset)` dans la trace, deux frappes à 522 ms donnent `tempo=114.9 (tap)`, et l'étoile
« non enregistré » apparaît bien.

### Ajouté (#196)
- **L'arbitre de tempo** (`engine::tempo`), socle du tap et de l'horloge MIDI : trois sources
  veulent donner le tempo — le preset, le tap, l'horloge — et une seule peut écrire. La priorité est
  **tap > horloge > preset** ; un tap détrône l'horloge et on n'y revient que sur un événement net
  (bascule de preset, `Start` / `Stop`). Une horloge qui **meurt** garde le dernier tempo et le
  signale : jamais de saut de son causé par une panne. Rien n'est encore visible — le tap (`*`) et
  l'affichage viennent avec #200.

### Ajouté (#195)
- **Les effets de temps se règlent au tempo.** Un `delay` accepte désormais sa durée en **division
  rythmique** — `--post delay:division=1/8.` — au lieu de millisecondes : neuf valeurs, de `1/16` à
  `1/1`, dans l'ordre des noms de notes. `libre` est le défaut, et `time` agit alors exactement
  comme avant. Mesuré : à 120 BPM, `1/4` place les échos à 500,0 ms et `1/8.` à 375,0 ms ; à 92 BPM,
  `1/4` à 652,2 ms ; une division plus longue que le tampon de 2 s est écrêtée et le dit.
- **Un tempo par preset** : `tempo_bpm` dans le `.preset.toml` (30 à 300 BPM, 120 par défaut).
  Changer de preset change le tempo. Un preset écrit par une version précédente se relit et se
  réécrit sans ligne en plus.
- **`--tempo BPM`** pour `run` et `render` : de quoi comparer deux réglages sans rien laisser au
  hasard. Le tempo est un **paramètre vivant** (ADR 0007) : en changer ne rebâtit aucune chaîne et
  ne coupe aucune répétition, et le passage d'un temps à l'autre fond en 50 ms — pas de clic, pas de
  glissement de hauteur.
- Une division s'écrit par son **nom de note** partout (`delay:division=1/8.`), se relit de même, et
  un nom inconnu rappelle les valeurs acceptées au lieu de dire « n'est pas un nombre ».

Le tap (`*`), l'arbitrage entre preset, tap et horloge MIDI, et le métronome audible viennent
ensuite (épique #188, [ADR 0008](docs/adr/0008-tempo-et-divisions.md)).

### Modifié (#193)
- Les traces de diagnostic posées pour la chasse de #191 (une ligne par touche, l'état de la banque
  à chaque demande) sont retirées : elles n'avaient de sens que pour cette enquête. Le relevé garde
  ce qui répond à « quelle chaîne joue vraiment », horodatage compris.
- `NAMFOOP_BROWSE_TRACE` n'est plus relu dans l'environnement à chaque ligne : éteinte, la trace ne
  coûte plus qu'une lecture atomique (elle n'a jamais été appelée depuis le thread audio, où écrire
  un fichier est interdit).

### Corrigé (#191)
- **Le son suit toujours la bascule de preset.** En enchaînant très vite deux presets, on pouvait
  entendre celui d'il y a un ou deux appuis alors que l'écran affichait déjà le bon : la chaîne
  était bâtie (80 à 400 ms pour un modèle), et pendant ce temps les appuis suivants arrivaient — la
  chaîne périmée était installée quand même. Elle est maintenant **jetée** si une demande plus
  récente est arrivée entre-temps. Mesuré en pseudo-terminal, 30 bascules à 120 ms d'intervalle :
  2 désaccords écran / son avant, **0** après (trois essais).
- Le relevé de `NAMFOOP_BROWSE_TRACE` est **horodaté** et dit désormais ce qu'il taisait : la touche
  reçue et le preset que l'écran croit jouer, une demande ignorée parce qu'identique, l'état de
  chaque place de la banque, et les réponses « déjà en jeu » / « pas prête » / « occupée ».

### Corrigé (#187)
- Le **volume du preset** (`,` / `.` dans `browse`) s'entend **tout de suite**. Il rebâtissait toute
  la chaîne à chaque appui — rechargement des modèles, des IR, et du son de calibration pour
  remesurer un alignement qui ne pouvait pas changer : **143 ms** pour un modèle, **jusqu'à 390 ms**
  pour un parallèle à deux modèles. De quoi croire que le réglage ne faisait rien. Mesuré après :
  **21 ms**, le plancher de l'écran, et plus aucune chaîne reconstruite.
- Un saut d'1 dB est lissé sur 20 ms, la même durée qu'un changement de preset : pas de clic.
- Au passage, basculer entre deux presets qui ne diffèrent que par leur volume ne recharge plus
  rien non plus.

### Modifié (#181)
- Le **niveau « match »** (volume aligné sur ta basse en direct) compare maintenant des **loudness
  pondérées K** (ITU-R BS.1770, la norme de la diffusion) au lieu de RMS larges : l'oreille n'entend
  pas le fondamental d'une basse comme ses harmoniques, et un son qui a mis son énergie dans le
  médium-aigu s'entend plus fort à RMS égal. **Le volume de tes chaînes change donc un peu** — à
  écouter.
- Mesuré sur la bibliothèque : un fuzz est aligné **1,2 dB plus bas** qu'avant, un ampli à 0,1 dB
  près comme avant, soit **1,3 dB** de correction relative entre les deux. C'est le bon sens, mais
  ça ne suffit pas à faire tenir un fuzz à côté d'un clair : pour ça, c'est le **volume du preset**
  (#179) qui tranche.
- Une pondération **A** a été mesurée et écartée : elle ignore tellement le grave qu'elle
  alignerait le fuzz **plus haut** encore que le RMS large.
- Le log dit les deux (« niveau aligné sur le DI : +4.4 dB (loudness pondérée ; RMS large :
  +5.6 dB) »), de quoi comparer à l'oreille.

### Ajouté (#179)
- **Volume par preset** (`,` et `.` dans l'espace Chaîne de `browse`, par pas d'1 dB, de -24 à
  +12 dB) : chaque preset porte son propre niveau de sortie, enregistré dans son `.preset.toml`.
  C'est ce qui permet qu'un preset saturé ne saute pas à l'oreille à côté d'un clair.
- Il s'applique **après** l'alignement de niveau sur le DI (et avant le master) : c'est donc le
  seul gain d'un preset que cet alignement n'annule pas — un bloc `gain`, lui, reste sans effet sur
  la sortie (#178).
- Il se lit dans l'en-tête (« preset 2/5 : Couplet · volume +3 dB ») et en gris en bout de
  synoptique, après l'étage « niveau DI » : l'ordre du signal. Il compte comme une modification,
  donc `S` le propose à l'enregistrement.
- `namfoop presets` le montre en fin de chaîne, et `namfoop run --preset` le joue : le preset sonne
  comme il a été réglé à l'oreille. Mesuré hors ligne : un preset enregistré à +3 dB rend bien
  +3,0 dB de RMS.
- Le master (`--master-db`, `master_db` de la config) reste à part : c'est le volume de la scène.

### Ajouté (#183)
- `browse`, banc d'essai : **panneau Bandes** (`z`), un égaliseur graphique des sept zones (sub,
  grave, bas-médium, médium, haut-médium, présence, air) qui montre d'un regard ce que la chaîne
  écoutée fait à ta basse. L'information existait déjà — mini-spectre, courbe du Spectre, écarts par
  zone en texte — mais elle ne se lisait pas d'un coup d'œil, et les écarts par zone demandaient une
  référence épinglée et 128 colonnes.
- Échelle ±12 dB (3 dB par demi-bloc, `▲` / `▼` hors échelle), couleurs des vumètres. Le zéro est la
  moyenne 80 Hz – 5 kHz de la chaîne : les colonnes montrent sa **couleur**, pas son niveau. Avec
  une référence épinglée (`c`), elles deviennent l'écart entre les deux sons, et `r` les retourne.
- Lecture type : une pédale de fuzz perd 26 dB de sub et 19 dB de grave et monte de 19 dB dans
  l'air — ce qui explique qu'elle paraisse bien plus forte qu'un ampli à niveau aligné (#181).
- La mesure est **figée** : elle vient du profil déjà calculé avec ta basse, donc les colonnes
  suivent le curseur sans reconstruire aucune chaîne et sans rien coûter au son. À partir de 128
  colonnes le panneau se glisse sous la courbe du Spectre et ne prend aucune ligne de liste : il est
  affiché d'office. Plus étroit, il s'empile sous la liste et attend `z`.

### Corrigé (#178)
- `browse` : un bloc `gain` posé dans la chaîne **ne changeait pas le niveau de sortie** et rien ne
  le disait — le même bloc à -40 ou à +24 dB donnait exactement la même sortie. La cause n'est pas
  le bloc : avec le volume aligné sur le DI (`level = "match"`, après `calibrate`), l'alignement se
  mesure sur la chaîne entière et son gain est ajouté **après** tout le reste, donc il compense
  exactement ce qu'on vient d'ajouter. Le vumètre du bloc baissait pendant que celui d'`OUT` ne
  bougeait pas, de quoi croire à un vumètre cassé.
- L'alignement est désormais **visible** : un dernier étage grisé en bout de synoptique, avec sa
  valeur (« niveau DI -11,1 dB »), qui bouge en direct quand on règle un bloc. Il n'est pas
  éditable : ce n'est pas un bloc de la chaîne. `browse` prévient aussi à l'ajout d'un `gain` et
  sous ses réglages, et l'aide (`h`) l'explique.
- Le guide dit maintenant ce qui marche quand même : un `gain` **avant** le modèle (c'est un drive,
  pas un volume), `--master-db` pour la sortie. Un niveau de sortie **par preset**, appliqué après
  l'alignement, reste à faire (#179, et #25 pour le même par modèle).

### Corrigé (#175)
- `browse`, espace Chaîne : taper `nam` ou `gain` dans le catalogue des blocs (`a` / `i`) **effaçait
  la chaîne** au lieu de filtrer — la lettre `n` était prise par la chaîne vierge de #170 avant
  d'arriver au filtre, et sur une chaîne enregistrée rien ne le demandait. Le filtre du catalogue
  reçoit maintenant les lettres avant les touches communes, comme les autres saisies de texte.

### Ajouté (#172)
- **La réverbe et le delay du preset quitté finissent de sonner** pendant que le nouveau son
  attaque (la « queue », comme les pédaliers à *trails*). Jusqu'ici la chaîne quittée était coupée
  net en 20 ms : c'était le fondu, qui portait sur la **sortie** des deux chaînes ; il porte
  désormais sur leur **entrée**, si bien que la chaîne qu'on quitte peut continuer à sonner une
  fois son entrée fermée. Elle s'arrête d'elle-même dès qu'elle est éteinte — mesuré : 0,5 s pour
  une chaîne sans effet à queue, 1 s pour une réverbe aux défauts, 2 s au plus pour un delay à
  longue réinjection.
- Le prix est une deuxième chaîne qui tourne pendant la queue (mesuré ×2,1 par bloc : pour b7k,
  3,4 % du budget qui passent à 6,8 %). `namfoop run --no-trails` et `namfoop browse --no-trails`
  coupent la queue si la charge DSP pose problème (Pi, preset à deux modèles en parallèle).
- Un preset rejoué **dans la seconde qui suit son départ** est rebâti (~80 ms) : sa chaîne finit
  encore de sonner, elle n'est donc pas revenue dans la banque. Un retard, pas une panne.

### Ajouté (#170)
- `browse` : **`n` repart d'une chaîne vierge**. La chaîne se vide, le preset courant est
  **détaché** (l'en-tête n'annonce plus de preset) et `S` propose donc un **nom neuf** : c'est la
  façon de construire une chaîne de plus sans vider celle d'un preset et sans risquer de
  réenregistrer par-dessus lui. `n` marche depuis les deux espaces et depuis la banque (qu'il
  referme) ; si la chaîne en cours n'est pas enregistrée, il demande d'abord, une fois (la même
  fenêtre que la bascule de preset : `S` enregistrer puis repartir, Entrée repartir quand même,
  Échap rester).
- Le nom proposé par `S` se met au singulier quand il n'y a qu'un bloc (`800D Modern + 1 bloc`).

### Ajouté (#167)
- `browse` : **changer de preset est désormais immédiat**. Les chaînes de tous les presets sont
  bâties et préchauffées dès l'ouverture, en tâche de fond (la barre du bas affiche `banque 3/7`
  tant qu'il en reste), et `PgDn` / `PgUp` ne font plus qu'installer une chaîne déjà prête. Mesuré
  de la touche au son installé, en pseudo-terminal : **20 ms** (le fondu) pour les trois presets de
  test, dont le parallèle à deux modèles qui demandait **204 ms** — et 81 ms pour `nam › ir`.
- Un preset pas encore préchargé (juste après l'ouverture, ou juste après l'avoir enregistré) se
  charge comme avant : c'est un retard, pas une panne. Enregistrer un preset fait reprécharger la
  banque, donc la bascule suivante joue bien la **nouvelle** version.
- Revenir sur un preset ne ramène pas la queue de réverbe de son passage précédent : la chaîne qui
  sort du thread audio est remise à zéro avant d'être rangée.

### Interne (#165)
- `engine` gagne une **banque de chaînes préchargées** (`engine::bank`, premier morceau de #6) :
  les chaînes des presets peuvent être bâties et préchauffées à l'avance, et basculer n'est plus
  qu'un envoi par la file existante. Mesuré (x86, B7K + IR du bassiste) : **1 µs** pour installer
  une chaîne préchargée, contre 19 ms de construction + 26 ms de préchauffage pour `nam › ir` et
  107 + 54 ms pour un parallèle à deux modèles. Une chaîne gardée en mémoire coûte **534 kio**
  (1,2 Mio à deux modèles) : on précharge toute la banque, sans plafond.
- La chaîne qui sort du thread audio est désormais **reprise** (`dsp::swap::SwapHandle::reclaim`),
  **remise à zéro** et rangée dans sa place au lieu d'être libérée : revenir sur un preset ne
  ramène pas la queue de réverbe de son passage précédent.
- Rien ne change encore à l'écran : `browse` passera par la banque dans la suite de #6.

### Interne (#128)
- `scripts/feature.sh` ne peut plus annoncer un faux succès : `merge` ne dit « fusionnée, main à
  jour » qu'après avoir vérifié que la PR est à l'état `MERGED` **et** que son commit de fusion est
  bien un ancêtre d'`origin/main` ; il refuse une PR déjà fusionnée au lieu de la prendre pour la
  sienne, `pr` ne reprend qu'une PR **ouverte**, et `start` ne réutilise jamais un nom de branche
  déjà pris (local ou `origin`) — il ajoute un suffixe. C'est ce qui avait fait croire qu'un commit
  était livré alors qu'il était resté sur la branche.
- Nouveau `scripts/test-feature.sh` (lancé par `check.sh`) : rejoue ces scénarios **hors ligne**,
  sur un dépôt jetable avec un faux `gh` — branche déjà prise, PR déjà fusionnée, PR qui reste
  ouverte, GitHub qui annonce une fusion sans rien livrer, et le chemin nominal. Lancé contre
  l'ancienne version du script, il échoue (le faux succès y est bien reproduit).

## 0.2.0 à 0.4.0 — entrées jamais roulées

_Les tags `v0.2.0` (10/11), `v0.3.0` (11/11) et `v0.4.0` (14/11) ont été posés sans rouler la
section « Non publié » : leurs entrées sont donc rassemblées ici, dans l'ordre d'arrivée, sans être
réparties entre les trois versions — les replacer aurait demandé de deviner. Elles couvrent le
découpage en crates (#47), le temps réel (#3), les effets (#5), l'éditeur de chaîne en blocs
(#103) et les presets (#157, #110, #158, #159)._

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
