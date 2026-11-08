# namfoop

Multi-effet basse temps réel en ligne de commande, pour Linux (PC, bientôt Raspberry Pi 5) et une
carte son USB : modèles [NAM](https://github.com/sdatkinson/NeuralAmpModelerCore) (amplis, pédales,
rigs complets) + IR de baffle, calibration du niveau de la basse, limiteur, ~5 ms de latence
aller-retour (mesurée avec une PreSonus AudioBox iTwo).

```
basse ─▶ entrée 1 ─▶ trim ─▶ NAM ─▶ IR ─▶ niveau ─▶ master ─▶ limiteur ─▶ sortie 1 (sondier)
                                                                      └─▶ sortie 2 (même / DI / muet)
```

**État : prototype (v0.1).** Disponible : chaîne NAM + IR, backends JACK (PipeWire) et ALSA direct,
calibration d'entrée et volume aligné sur la basse en direct, rendu hors ligne, bench CPU, mesure
de latence. À venir : effets, presets, MIDI, démarrage automatique, Raspberry Pi.

## Démarrage rapide

```sh
git clone --recurse-submodules https://github.com/cedricbou/namfoop.git && cd namfoop
cargo build --release                         # prérequis : rustup, gcc, alsa-lib, pipewire-jack
scripts/import-tone3000.sh nam library        # zips Tone3000 de nam/ -> library/
./target/release/namfoop models               # inventaire des modèles et IR
pw-jack ./target/release/namfoop --device iTwo calibrate          # une fois : joue fort 15 s
pw-jack ./target/release/namfoop --device iTwo run -m b7k         # jouer
./target/release/namfoop --backend alsa --device hw:iTwo --buffer 48 run -m "800d modern" -i "dg212 sm57"
```

Autres commandes : `namfoop render` (traiter un WAV), `namfoop bench` (coût CPU),
`namfoop latency` (mesure aller-retour), `namfoop ports` (ports JACK).

## Documentation

- **[Guide utilisateur](docs/guide.md)** : câblage, réglages système, bibliothèque, calibration,
  jeu, niveaux, backends, dépannage.
- [Référence des commandes](docs/cli.md) : générée depuis l'aide intégrée (`namfoop <commande> --help`).
- [CHANGELOG](CHANGELOG.md) : évolutions par version.
- [CLAUDE.md](CLAUDE.md) : architecture, règles temps réel, harnais de développement.

## Développement

```sh
scripts/check.sh            # sans matériel : format, clippy, tests, docs, rendu de la bibliothèque
scripts/check.sh --quick    # idem sans la bibliothèque
scripts/test-backends.sh    # avec matériel : latence + run sur JACK ET ALSA (câble sortie 1 -> entrée 2)
scripts/install-hooks.sh    # vérification rapide automatique avant chaque commit
```

## Licences

namfoop : MIT. NeuralAmpModelerCore (sous-module) : MIT. Les modèles et IR ne sont **pas** distribués
avec le code : chaque pack garde la licence de son auteur.
