# ADR 0005 · Effets et chaînes

**Statut : adopté** (#5, socle en #79).

**Contexte.** Les effets (gate, compresseur, EQ, crossover, delay, chorus, réverbe) doivent se régler
depuis la CLI, `browse`, les presets (#6) et les CC MIDI (#7) sans quatre descriptions différentes.
Sur scène, latence et fiabilité priment ; l'utilisateur joue de la basse en mono.

**Décision.**
- Tout reste **mono** ; la stéréo sera une évolution, s'il y en a un jour.
- Un effet = `dsp::fx::FxSpec` (données sérialisables) + un `Stage` (calcul). Chaque paramètre est
  décrit par un `ParamInfo` (nom, unité, plage, défaut, pas, aide) : CLI, `namfoop fx`, éditeur de
  `browse` et CC MIDI se génèrent depuis la même table ; les tests génériques passent tout le
  catalogue (pas d'allocation, indépendance au bloc, sortie finie, silence, `reset`).
- Le parallèle est un **étage** (`Split`, qui contient des sous-chaînes), pas un graphe : `Chain`
  reste une liste.
- Aucune latence ajoutée, sauf la compensation de retard entre les branches d'un `Split`.
- Paramètres en deux temps : pour #5, un changement reconstruit la chaîne et fond (les queues de
  delay / réverbe sont coupées) ; `set_param` en direct avec lissage viendra avec #6.
- Presets (#6) : fichiers TOML **dans la bibliothèque**, lus par `library` ; `model` garde la config
  de l'instrument.

**Conséquences.** Ajouter un effet = un fichier, une variante de `FxSpec`, une ligne de catalogue
(`crates/dsp/CLAUDE.md`). Pas de modulation entre effets, pas de graphe arbitraire, pas de maintien
des queues au changement de chaîne dans #5.
