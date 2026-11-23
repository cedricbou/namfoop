<img src="docs/identite/banniere.svg" alt="namfoop" width="100%">

Multi-effet basse temps réel en ligne de commande, pour Linux (PC, bientôt Raspberry Pi 5) et une
carte son USB : modèles [NAM](https://github.com/sdatkinson/NeuralAmpModelerCore) (amplis, pédales,
rigs complets) + IR de baffle, calibration du niveau de la basse, limiteur, ~5 ms de latence
aller-retour (mesurée avec une PreSonus AudioBox iTwo).

```
basse ─▶ entrée basse ─▶ trim ─▶ NAM ─▶ IR ─▶ niveau ─▶ master ─▶ limiteur ─▶ sorties « traité »
                     └─────────────────────────────────────────────────────▶ sorties « DI »
```

**Ce qui marche aujourd'hui** : chaîne NAM + IR, ALSA direct et carte
simulée, cartes multicanaux, assistant de configuration et navigateur de sons plein écran (changement sans
coupure, IR conseillées, mélanges d'IR, éditeur de chaîne), calibration d'entrée et volume
aligné sur la basse en direct, effets (gate, compresseur, EQ, crossover, chorus, delay, réverbe),
presets (enregistrer une chaîne, la rechercher, la rejouer, repartir d'une chaîne vierge, passer à
la suivante en 20 ms grâce à la banque préchargée en mémoire, la réverbe du preset quitté finissant
de sonner),
sets de concert (set list, vue scène, prompteur ChordPro),
entrée MIDI (pédalier ou Ableton écoutés, port réglable, port simulé pour essayer sans matériel),
rendu hors ligne, bench CPU, mesure de latence (à vide ou sous charge). À venir : ce que namfoop
fait des messages MIDI (preset, tap, horloge), démarrage automatique, Raspberry Pi.

## Démarrage rapide

**[Le démarrage rapide illustré](docs/demarrage-rapide.md)** montre chaque étape à l'écran, de
l'installation au premier concert. En commandes :

```sh
git clone --recurse-submodules https://github.com/cedricbou/namfoop.git && cd namfoop
cargo build --release                         # prérequis : voir « Prérequis de compilation »
./target/release/namfoop import nam/         # zips (Tone3000, packs achetés) -> library/
./target/release/namfoop models               # inventaire des modèles et IR
./target/release/namfoop setup                # une fois : carte, entrées, sorties, buffer
./target/release/namfoop calibrate            # une fois : joue fort 15 s
./target/release/namfoop browse               # choisir modèle / IR / mélange en jouant (S enregistre un preset, n repart de zéro)
./target/release/namfoop presets              # les presets enregistrés
./target/release/namfoop sets                 # les sets de concert (morceaux et leurs sons)
./target/release/namfoop midi                 # les ports MIDI, et ce qu'y envoie le pédalier
./target/release/namfoop run --set concert    # JOUER UN CONCERT : écran de scène verrouillé (u : accordeur, basse coupée)
./target/release/namfoop run --preset chill   # rejouer un preset
./target/release/namfoop run -m b7k           # jouer
./target/release/namfoop run -m "800d modern" -i "dg212 sm57" --route 2=di
```

Les sons, presets et sets vivent dans la bibliothèque : `library/` du dossier courant s'il existe,
sinon `~/.local/share/namfoop/library`. Pour lancer namfoop d'ailleurs (`cargo install --path
crates/namfoop`, console de scène, Pi), range-la là, ou indique-la par `library = "…"` dans la config
(guide, § 6).

Si le dépôt a été cloné sans `--recurse-submodules`, la compilation s'arrête sur `nam-sys` avec
« NeuralAmpModelerCore absent » : lancer `git submodule update --init --recursive` à la racine du
dépôt, puis relancer `cargo build --release`.

Autres commandes : `namfoop meta`
(corriger les métadonnées d'un modèle : type de capture, gain d'entrée, IR préférée), `namfoop render`
(traiter un WAV), `namfoop bench` (coût CPU), `namfoop latency`
(mesure aller-retour, à vide ou avec les chaînes de référence : `--chains`),
`namfoop fx` (effets à placer avant ou après le modèle avec `--pre` /
`--post`). Sans matériel : `namfoop --backend sim run -m b7k`.

## Prérequis de compilation

Il faut un compilateur C++20 (le moteur NAM est compilé avec le projet), `pkg-config`, `git`, et les
fichiers de développement d'ALSA. Rust s'installe avec [rustup](https://rustup.rs), qui
prend tout seul la version fixée par `rust-toolchain.toml`.

Arch Linux et dérivées (Manjaro, EndeavourOS, CachyOS) :

```sh
sudo pacman -S --needed base-devel git rustup alsa-lib
```

Ubuntu, Debian et dérivées (Mint, Pop!_OS, Raspberry Pi OS) :

```sh
sudo apt install build-essential pkg-config git libasound2-dev
```

Fedora :

```sh
sudo dnf install gcc-c++ pkgconf-pkg-config git alsa-lib-devel
```

Sans eux, la compilation s'arrête sur `alsa-sys` avec « Package alsa was not found in the
pkg-config search path ».

## Documentation

- **[Démarrage rapide](docs/demarrage-rapide.md)** : le parcours en images — installer, configurer
  la carte, calibrer, écouter des modèles NAM et des IR, bâtir une chaîne d'effets, jouer une set
  list.
- **[Guide utilisateur](docs/guide.md)** : câblage, réglages système, assistant de configuration,
  bibliothèque, calibration, jeu, niveaux, backends et routage, dépannage.
- [Référence des commandes](docs/cli.md) : générée depuis l'aide intégrée (`namfoop <commande> --help`).
- [Charte graphique](docs/identite/charte.md) : palette, polices, logo, icône, bannière, et ce
  qu'on a le droit d'en faire.
- [CHANGELOG](CHANGELOG.md) : évolutions par version.
- [CLAUDE.md](CLAUDE.md) : architecture, règles temps réel, harnais de développement.

## Développement

```sh
scripts/check.sh            # sans matériel : format, clippy, tests, docs, rendu de la bibliothèque
scripts/check.sh --quick    # idem sans la bibliothèque
scripts/test-backends.sh    # avec matériel : latence + run en ALSA (câble sortie 1 -> entrée 2)
                            # une autre carte se donne en variables : voir l'en-tête du script
scripts/install-hooks.sh    # hooks : pas de commit/push direct sur main, check rapide avant commit
```

Chaque évolution passe par une issue, une branche et une PR (`scripts/feature.sh start | pr |
merge`) : voir « Cycle de développement » dans [CLAUDE.md](CLAUDE.md).

## Licences

namfoop est un logiciel libre sous **GPL-3.0-or-later** ([`LICENSE`](LICENSE)) : on peut l'utiliser,
l'étudier, le modifier et le redistribuer, à condition qu'une version redistribuée, modifiée ou non,
reste sous la même licence avec son code source.

Ce que namfoop embarque garde sa propre licence, compatible avec la GPL :

| Composant | Licence |
| :--- | :--- |
| NeuralAmpModelerCore (sous-module) et ses modèles d'exemple | MIT |
| Eigen (en-têtes du sous-module, non modifiés) | MPL-2.0 |
| nlohmann/json (en-tête du sous-module) | MIT |
| Crates Rust de `Cargo.lock` | MIT, Apache-2.0 ou équivalentes permissives |
| Polices Chakra Petch, Inter, JetBrains Mono | SIL OFL 1.1 |

Les modèles et IR ne sont **pas** distribués avec le code : chaque pack garde la licence de son
auteur.

Fractal Audio, Axe-Fx, Neural DSP, Quad Cortex, Line 6, Helix, Kemper, Strymon, Ableton, Tone3000 et
les marques d'amplis citées appartiennent à leurs propriétaires respectifs. namfoop les nomme pour
décrire une compatibilité ou comparer des choix ; il n'est ni affilié à ces sociétés, ni approuvé
par elles.

Le logo, l'icône, l'onde et la bannière de [`docs/identite/`](docs/identite/charte.md) sont **tous
droits réservés** : utilisables pour parler de namfoop ou y renvoyer, pas pour un autre projet.
