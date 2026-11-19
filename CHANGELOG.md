# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/). Chaque fonctionnalité ou
correction visible par l'utilisateur ajoute une ligne dans « Non publié » ; à la publication, la
section prend le numéro de version.

## Non publié

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
