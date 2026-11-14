# namfoop

Multi-effet basse temps réel en ligne de commande, pour Linux (PC, bientôt Raspberry Pi 5) et une
carte son USB : modèles [NAM](https://github.com/sdatkinson/NeuralAmpModelerCore) (amplis, pédales,
rigs complets) + IR de baffle, calibration du niveau de la basse, limiteur, ~5 ms de latence
aller-retour (mesurée avec une PreSonus AudioBox iTwo).

```
basse ─▶ entrée basse ─▶ trim ─▶ NAM ─▶ IR ─▶ niveau ─▶ master ─▶ limiteur ─▶ sorties « traité »
                     └─────────────────────────────────────────────────────▶ sorties « DI »
```

**État : prototype.** Disponible : chaîne NAM + IR, backends JACK (PipeWire), ALSA direct et carte
simulée, cartes multicanaux, assistant de configuration et navigateur de sons plein écran (changement sans
coupure, IR conseillées, mélanges d'IR, éditeur de chaîne), calibration d'entrée et volume
aligné sur la basse en direct, effets (gate, compresseur, EQ, crossover, chorus, delay, réverbe),
presets (enregistrer une chaîne, la rechercher, la rejouer, passer à la suivante en 20 ms grâce à
la banque préchargée en mémoire),
rendu hors ligne, bench CPU, mesure de latence. À venir : MIDI, démarrage
automatique, Raspberry Pi.

## Démarrage rapide

```sh
git clone --recurse-submodules https://github.com/cedricbou/namfoop.git && cd namfoop
cargo build --release                         # prérequis : voir « Prérequis de compilation »
./target/release/namfoop import nam/         # zips (Tone3000, packs achetés) -> library/
./target/release/namfoop models               # inventaire des modèles et IR
./target/release/namfoop setup                # une fois : carte, entrées, sorties, buffer
./target/release/namfoop calibrate            # une fois : joue fort 15 s
./target/release/namfoop browse               # choisir modèle / IR / mélange en jouant (S enregistre un preset)
./target/release/namfoop presets              # les presets enregistrés
./target/release/namfoop run --preset chill   # rejouer un preset
./target/release/namfoop run -m b7k           # jouer
./target/release/namfoop run -m "800d modern" -i "dg212 sm57" --route 2=di
```

Si le dépôt a été cloné sans `--recurse-submodules`, la compilation s'arrête sur `nam-sys` avec
« NeuralAmpModelerCore absent » : lancer `git submodule update --init --recursive` à la racine du
dépôt, puis relancer `cargo build --release`.

Pas besoin de `pw-jack` : namfoop trouve PipeWire tout seul. Autres commandes : `namfoop meta`
(corriger les métadonnées d'un modèle : type de capture, gain d'entrée, IR préférée), `namfoop render`
(traiter un WAV), `namfoop bench` (coût CPU), `namfoop latency` (mesure aller-retour),
`namfoop ports` (ports JACK), `namfoop fx` (effets à placer avant ou après le modèle avec `--pre` /
`--post`). Sans matériel : `namfoop --backend sim run -m b7k`.

## Prérequis de compilation

Il faut un compilateur C++20 (le moteur NAM est compilé avec le projet), `pkg-config`, `git`, et les
fichiers de développement d'ALSA et de JACK. Rust s'installe avec [rustup](https://rustup.rs), qui
prend tout seul la version fixée par `rust-toolchain.toml`.

Arch Linux et dérivées (Manjaro, EndeavourOS, CachyOS) :

```sh
sudo pacman -S --needed base-devel git rustup alsa-lib pipewire-jack
```

Ubuntu, Debian et dérivées (Mint, Pop!_OS, Raspberry Pi OS) :

```sh
sudo apt install build-essential pkg-config git libasound2-dev libjack-jackd2-dev pipewire-jack
```

Fedora :

```sh
sudo dnf install gcc-c++ pkgconf-pkg-config git alsa-lib-devel pipewire-jack-audio-connection-kit-devel
```

Sans ces paquets, la compilation s'arrête sur `jack-sys` ou `alsa-sys` avec « Package jack (ou alsa)
was not found in the pkg-config search path ». Sur Arch, `pipewire-jack` (ou `jack2`) fournit à la
fois les fichiers de développement et la bibliothèque ; sur Debian / Ubuntu, `libjack-jackd2-dev`
sert à compiler et `pipewire-jack` à parler à PipeWire à l'exécution.

## Documentation

- **[Guide utilisateur](docs/guide.md)** : câblage, réglages système, assistant de configuration,
  bibliothèque, calibration, jeu, niveaux, backends et routage, dépannage.
- [Référence des commandes](docs/cli.md) : générée depuis l'aide intégrée (`namfoop <commande> --help`).
- [CHANGELOG](CHANGELOG.md) : évolutions par version.
- [CLAUDE.md](CLAUDE.md) : architecture, règles temps réel, harnais de développement.

## Développement

```sh
scripts/check.sh            # sans matériel : format, clippy, tests, docs, rendu de la bibliothèque
scripts/check.sh --quick    # idem sans la bibliothèque
scripts/test-backends.sh    # avec matériel : latence + run sur JACK ET ALSA (câble sortie 1 -> entrée 2)
scripts/install-hooks.sh    # hooks : pas de commit/push direct sur main, check rapide avant commit
```

Chaque évolution passe par une issue, une branche et une PR (`scripts/feature.sh start | pr |
merge`) : voir « Cycle de développement » dans [CLAUDE.md](CLAUDE.md).

## Licences

namfoop : MIT. NeuralAmpModelerCore (sous-module) : MIT. Les modèles et IR ne sont **pas** distribués
avec le code : chaque pack garde la licence de son auteur.
