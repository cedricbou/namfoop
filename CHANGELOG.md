# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/). Chaque fonctionnalité ou
correction visible par l'utilisateur ajoute une ligne dans « Non publié » ; à la publication, la
section prend le numéro de version.

## Non publié

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
