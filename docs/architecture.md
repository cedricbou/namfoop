# Architecture de namfoop

Page de repère pour les agents et les humains : où est quoi, qui a le droit de dépendre de qui, et
pourquoi. Les décisions déjà prises sont dans [`docs/adr/`](adr/) : ne pas les rouvrir sans
nouvelle mesure. Le découpage est suivi dans l'issue #47.

## Architecture cible

```
namfoop  (binaire : CLI, écrans, une fonction mince par commande)
  ├─> engine   (session audio : routeur, swap, vumètres, tap, presets / MIDI plus tard)  ─> audio-io, dsp, model
  ├─> library  (bibliothèque, import, overrides, recommandations, profils)               ─> dsp, model
  └─> model    (types sérialisables : config, routage, presets, profils de carte)
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

## État actuel

Trois crates : `nam-sys`, `dsp`, et le binaire `namfoop` qui porte encore les backends, le
routeur, la bibliothèque et les écrans (voir « Disposition » dans `CLAUDE.md`). Les crates
`audio-io`, `engine`, `model` et `library` seront extraites une par une (#50 à #57), à comportement
constant, sous le filet des rendus de référence (`crates/dsp/tests/golden.rs`).

## Règles vérifiées

`scripts/check-arch.sh` (appelé par `scripts/check.sh`, donc en CI) lit `cargo metadata` et refuse
les arêtes interdites. Seules les dépendances normales et de build comptent (pas les
dev-dependencies). Chaque issue qui crée une crate ajoute ses règles à la table du script et ici.

| Crate     | Règle                                                       | Depuis |
|-----------|-------------------------------------------------------------|--------|
| `nam-sys` | seulement `thiserror` (+ `cc` en build)                     | #49    |
| `dsp`     | interdit `alsa`, `jack`, `clap`, `ratatui`, `libc`, `namfoop` | #49    |
