# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/). Chaque fonctionnalité ou
correction visible par l'utilisateur ajoute une ligne dans « Non publié » ; à la publication, la
section prend le numéro de version.

## Non publié

### Ajouté
- `namfoop setup` : assistant plein écran (backend, carte, vumètres par entrée, bip par sortie,
  test automatique des buffers), qui enregistre `[audio]` et `[routing]` dans la config.
- Cartes multicanaux : namfoop ouvre tous les canaux de la carte (ex. Soundcraft MTK 14/12).
- Routage : `--bass-in`, `--mic-in`, `--route N=processed|di|mic|mute` (remplace `--out2`).
- Backend `sim` : carte simulée (basse synthétique) pour tester sans matériel.
- Options globales `--config` / `--no-config` (communes à toutes les commandes).
- Harnais : cycle issue -> branche -> PR -> merge (`scripts/feature.sh`), hooks git qui refusent
  commits et pushs directs sur `main`, règles de PR vérifiées en CI, modèles d'issue et de PR.

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
