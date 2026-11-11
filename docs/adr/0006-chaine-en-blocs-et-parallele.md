# ADR 0006 · Chaîne en blocs, parallèle jusqu'à 4 branches

**Statut : adopté** (décision de l'utilisateur, 11/11/2026, épique #103). Amende l'[ADR 0005](0005-effets-et-chaines.md)
sur la place de l'ampli et du parallèle ; le reste de l'ADR 0005 tient.

**Contexte.** Le `split` de l'ADR 0005 est un effet du catalogue dont les branches ne contiennent que
des effets : ajouté depuis `browse` ou `--pre` / `--post`, il a deux branches vides et ne fait
qu'un passe-tout (#102). L'ampli (NAM › IR) est fixé entre `pre` et `post`, donc aucune branche ne
peut le contenir ; le cas le plus utile à la basse (« son direct en grave, ampli en aigu ») n'existe
que codé en dur, par `--crossover`. Deux notions de parallèle qui ne se parlent pas. Les multi-effets
à écran (Quad Cortex, Helix, GT-1000, Anagram) montrent tous la même chose : série de gauche à
droite, branches les unes sous les autres, séparation et jonction visibles et réglables, profondeur
volontairement limitée.

**Décision.**
- **NAM et IR sont des blocs** de la chaîne, comme les effets, placés librement, y compris dans une
  branche. Plusieurs NAM sont permis (une fuzz capturée et un ampli, par exemple) ; leur coût est
  mesuré (#111) et `browse` alerte au-delà d'un seuil.
- **Le parallèle n'est plus un effet du catalogue** mais un bloc de structure : séparation, 2 à
  **4 branches**, somme. Deux modes : **copie** (chaque branche reçoit tout le signal) et **bandes**
  (crossover Linkwitz-Riley 24 dB/oct, 2 à 4 bandes, N-1 coupures croissantes, somme plate). Par
  branche : gain, polarité, muet ; un nom en mode copie. Le solo est une aide d'écoute de `browse`,
  traduite en muets, jamais enregistrée.
- **Un seul niveau** : pas de parallèle dans une branche (refusé à la validation). Plusieurs
  parallèles peuvent se suivre en série. `Chain` reste une liste ; pas de graphe arbitraire.
- **Alignement** : une branche sans NAM est alignée à la construction sur la première branche qui en
  contient un (mesure sur l'enregistrement de calibration, sinon la note synthétique), comme le
  grave clair de `--crossover` aujourd'hui ; le gain de la branche s'ajoute. Les retards entre
  branches restent mesurés et compensés. Le niveau « match » reste mesuré sur toute la chaîne.
- **`--crossover HZ` devient un raccourci** : bandes [HZ], grave = son direct, aigu = NAM › IR, au
  rendu identique. `-m`, `-i`, `--pre`, `--post` restent et se traduisent en blocs.
- **Le trim** de l'instrument est en tête de chaîne ; quand rien ne précède l'unique NAM, il reste dans
  l'étage NAM (au bit près, comme aujourd'hui).
- **`browse`** : le synoptique de la chaîne est l'éditeur (navigation 2D, vumètres par bloc et par
  branche, réduction du compresseur, gate fermé), selon la
  [spécification](../design/browse-chaine.md) et la [maquette](../design/browse-chaine.html)
  validées. Échap ne quitte jamais ; `q` demande confirmation si quelque chose n'est pas enregistré.
- **Mesures pour l'affichage** : tables préallouées à la construction de la chaîne (comme le vumètre
  du limiteur), aucune allocation ni verrou en temps réel.

**Conséquences.** Un preset décrit une liste de blocs dont certains sont des parallèles (format
proposé dans #6). Les rendus de référence ne changent pas : les chaînes d'aujourd'hui s'expriment
toutes en blocs. La ligne de commande ne décrit pas les branches (illisible) : une chaîne avec un
parallèle autre que `--crossover` passe par `browse` et les presets. Pas de modulation entre
blocs, pas de routage stéréo, pas de parallèle imbriqué.
