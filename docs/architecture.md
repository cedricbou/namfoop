# Architecture de namfoop

Page de repère pour les agents et les humains : où est quoi, qui a le droit de dépendre de qui, et
pourquoi. Les décisions déjà prises sont dans [`docs/adr/`](adr/) : ne pas les rouvrir sans
nouvelle mesure. Le découpage a été fait dans l'issue #47 (PR #59 à #68).

## Architecture

C'est l'état réel du code (vérifié par `scripts/check-arch.sh`), pas une cible.

```
namfoop  (binaire : CLI, écrans, une fonction mince par commande)
  ├─> engine   (session audio : routeur, swap, vumètres, tap d'entrée ; raw pour les mesures)  ─> audio-io, dsp, model
  ├─> library  (bibliothèque, import, overrides, recommandations, profils)               ─> dsp, model
  └─> model    (types sérialisables : config, routage ; presets et profils de carte à venir)
audio-io (ALSA, JACK, sim, priorité RT ; ni clap, ni config, ni DSP)
dsp      (étages mono, chaîne, swap, analyses hors ligne)  ─> nam-sys (FFI C++ NAM)
```

Les dépendances ne vont que **vers le bas**. La CLI pilote, `engine` fait sonner, `audio-io` parle
à la carte, `dsp` calcule.

| Crate      | Responsabilité                                                       | Interdit                                   |
|------------|----------------------------------------------------------------------|--------------------------------------------|
| `nam-sys`  | FFI vers le C++ NAM                                                  | tout le reste                              |
| `dsp`      | étages audio mono, chaîne, swap, analyses hors ligne                 | carte son, config, threads, CLI            |
| `audio-io` | ouvrir une carte (ALSA, JACK, sim), négocier format et buffer, RT    | clap, config utilisateur, DSP              |
| `model`    | types sérialisables : config, routage, presets, profils de carte     | toute logique, toute crate du projet       |
| `engine`   | session audio : routeur, swap, mesures, commandes, reprise           | clap, ratatui, bibliothèque de sons        |
| `library`  | modèles et IR, import, overrides, recommandations, profils de sons   | carte son, clap, ratatui                   |
| `namfoop`  | CLI, écrans plein écran, une fonction mince par commande             | logique audio, accès direct à un backend   |

Règles clés : `dsp` et `audio-io` ne se connaissent pas ; seule `engine` ouvre la carte ; `model`
ne dépend d'aucune crate du projet ; aucune crate de bibliothèque n'utilise `clap` ni `ratatui`.

## Où en est-on

Les sept crates existent ; chacune a son `CLAUDE.md` (responsabilité, API, interdits, règles,
faits mesurés, tests), cité par la carte du `CLAUDE.md` racine. Toutes les commandes passent par
`engine` (`Engine`, ou `engine::raw` pour `setup` et `latency`). Aucun fichier `.rs` hors tests ne
dépasse 1 000 lignes (avertissement au-delà de 600, `scripts/check-docs.sh`). Le son est protégé
par les rendus de référence (`crates/dsp/tests/golden.rs`). Les profils de carte (#58) sont des
données (`profiles/*.toml`, `model::card`) ; `audio-io` ne reçoit qu'une liste de contrôles mixer
dans `Settings`. À venir sur cette base : #3, presets (#6), MIDI (#7), service de scène (#46).

## Règles vérifiées

`scripts/check-arch.sh` (appelé par `scripts/check.sh`, donc en CI) lit `cargo metadata` et refuse
les arêtes interdites. Seules les dépendances normales et de build comptent (pas les
dev-dependencies). Chaque issue qui crée une crate ajoute ses règles à la table du script et ici.

| Crate     | Règle                                                       | Depuis |
|-----------|-------------------------------------------------------------|--------|
| `nam-sys` | seulement `thiserror` (+ `cc` en build)                     | #49    |
| `dsp`     | interdit `alsa`, `jack`, `clap`, `ratatui`, `libc`, `namfoop`, `audio-io`, `engine` | #49, #50, #52 |
| `audio-io` | interdit `clap`, `ratatui`, `dsp`, `nam-sys`, `namfoop`, `toml`, `engine` | #50, #52 |
| `engine`  | interdit `clap`, `ratatui`, `namfoop`, `toml`, `alsa`, `jack` (la carte passe par `audio-io`) | #52 |
| `model`   | seulement `serde`, `toml`, `thiserror` (aucune crate du projet) | #54 |
| `library` | interdit `audio-io`, `engine`, `clap`, `ratatui`, `alsa`, `jack` | #55 |
| tout le code | aucun `audio_io::start` / `start_with` hors d'`engine` et `audio-io` (contrôle textuel) | #53 |
| toutes     | un `CLAUDE.md` par crate, cité dans la carte racine et ici ; `.rs` > 1 000 lignes refusé (`check-docs.sh`) | #57 |
