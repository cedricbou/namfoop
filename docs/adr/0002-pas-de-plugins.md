# ADR 0002 · Moteur monolithique Rust, pas de plugins

**Contexte.** Un hôte LV2 / VST (ou mod-host) offrirait un catalogue d'effets, mais ajoute un
serveur, des formats, des threads et des allocations qu'on ne contrôle pas. La fiabilité en live
et la latence priment sur tout ; un agent doit pouvoir tester toute la chaîne sans matériel.

**Décision.** Tous les traitements sont des étages Rust (`dsp::Stage`) appelés directement dans
le callback, plus le C++ NAM via `nam-sys`. Pas d'hôte de plugins, pas de mod-host, pas de JACK
obligatoire pour relier des étages.

**Conséquences.** Chaque effet est écrit (ou porté) dans `dsp`, avec ses tests d'absence
d'allocation et d'indépendance à la taille de bloc. Le catalogue grandit lentement, mais tout le
chemin temps réel est connu et vérifiable.
