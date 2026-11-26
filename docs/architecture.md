# Architecture de namfoop

Page de repère pour les agents et les humains : où est quoi, qui a le droit de dépendre de qui, et
pourquoi. Les décisions déjà prises sont dans [`docs/adr/`](adr/) : ne pas les rouvrir sans
nouvelle mesure.

## Architecture

C'est l'état réel du code (vérifié par `scripts/check-arch.sh`), pas une cible.

```
namfoop  (binaire : CLI, écrans, une fonction mince par commande)
  ├─> engine   (session audio : routeur, swap, vumètres, tap d'entrée ; raw pour les mesures)  ─> audio-io, dsp, model
  ├─> library  (bibliothèque, import, échange, overrides, recommandations, profils)       ─> dsp, model, nam-sys
  ├─> midi-io  (entrée MIDI : port ALSA seq, port simulé ; ni clap, ni config, ni DSP)
  └─> model    (types sérialisables : config, routage, profils de carte)
audio-io (ALSA, carte simulée, priorité RT ; ni clap, ni config, ni DSP)
dsp      (étages mono, chaîne, swap, analyses hors ligne)  ─> nam-sys (FFI C++ NAM)
```

Les dépendances ne vont que **vers le bas**. La CLI pilote, `engine` fait sonner, `audio-io` parle
à la carte, `midi-io` écoute le pédalier, `dsp` calcule.

**Pourquoi `midi-io` ne passe pas par `engine`** alors que la carte son, elle, y passe : le MIDI
n'entre pas dans le fil audio. Il est lu par un fil à part, consommé par la **boucle d'écran**, et
remis à des machines pures — l'arbitre de tempo (`engine::tempo`) et la banque de chaînes. Mettre la
porte dans `engine` n'aurait ajouté qu'un passage obligé ; la règle « une seule porte » est tenue
par la crate elle-même et par le contrôle textuel ci-dessous.

| Crate      | Responsabilité                                                       | Interdit                                   |
|------------|----------------------------------------------------------------------|--------------------------------------------|
| `nam-sys`  | FFI vers le C++ NAM                                                  | tout le reste                              |
| `dsp`      | étages audio mono, chaîne, swap, analyses hors ligne                 | carte son, `model` (donc la config), threads, CLI |
| `audio-io` | ouvrir une carte (ALSA, simulée), négocier format et buffer, RT      | clap, `toml`/`serde` (donc la config), DSP |
| `midi-io`  | ouvrir un port MIDI (séquenceur ALSA, simulé), livrer des messages typés | clap, la config, DSP, toute crate du projet |
| `model`    | types sérialisables : config, routage, profils de carte              | toute logique, toute crate du projet       |
| `engine`   | session audio : routeur, swap, mesures, commandes, reprise           | clap, ratatui, bibliothèque de sons        |
| `library`  | modèles et IR, import, échange, overrides, recommandations, profils | carte son, clap, ratatui                   |
| `namfoop`  | CLI, écrans plein écran, une fonction mince par commande             | logique audio, accès direct à un backend   |

Règles clés : `dsp` et `audio-io` ne se connaissent pas ; seule `engine` ouvre la carte ; seule
`midi-io` ouvre un port MIDI ; `model` ne dépend d'aucune crate du projet ; aucune crate de
bibliothèque n'utilise `clap` ni `ratatui`.

**Ce que « ne connaît pas la config » veut dire exactement** : `dsp` **lit et écrit des fichiers** — ses propres formats, les presets (`.preset.toml`) et
les mélanges d'IR (`.cab.toml`). Ce qu'il ne peut pas voir, c'est la configuration de
l'utilisateur : il ne dépend pas de `model`, donc le type `Config` lui est inaccessible, et
`check-arch.sh` le vérifie. `audio-io`, lui, ne dépend ni de `toml` ni de `serde` : il ne
désérialise rien. Il lit bien `/proc/asound/...` pour sonder les cartes, mais c'est du matériel,
pas de la configuration.

## Décisions

Chacune a son ADR dans [`docs/adr/`](adr/), qui porte le contexte, les alternatives et les mesures.

| Décision | Ce qu'elle fixe |
|---|---|
| [0002](adr/0002-pas-de-plugins.md) | un moteur monolithique en Rust, pas de plugins |
| [0003](adr/0003-hat-egale-carte-alsa.md) | un HAT I2S est une carte ALSA plus un profil : rien de spécial dans le code |
| [0004](adr/0004-decoupage-en-crates.md) | le découpage en crates et le sens des dépendances |
| [0005](adr/0005-effets-et-chaines.md) | un effet = `dsp::fx::FxSpec` + `Stage`, tout mono, parallèle par un étage `Split` |
| [0006](adr/0006-chaine-en-blocs-et-parallele.md) | NAM et IR en **blocs**, parallèle jusqu'à 4 branches ; l'éditeur est spécifié dans [`docs/design/`](design/browse-chaine.md) |
| [0007](adr/0007-parametres-vivants.md) | les **paramètres vivants** : une case partagée, écrite par le contrôle, lue une fois par bloc, hors de l'identité d'une demande de chaîne |
| [0008](adr/0008-tempo-et-divisions.md) | le **tempo** : trois sources (preset, tap, horloge MIDI), un arbitre dans `engine` qui les hiérarchise, une case, et des **divisions rythmiques** dont la valeur « libre » rend la main aux millisecondes |
| [0009](adr/0009-clavier-de-browse.md) | le **clavier de `browse`** : quatre étages (la pédale, l'espace, le commun, le dernier recours), un **propriétaire par touche**, une modale qui ne retombe jamais, et un **oracle** qui mesure chaque touche dans chaque contexte |
| [0010](adr/0010-retrait-de-jack.md) | deux backends, ALSA et sim, et aucun autre (elle supersède l'ADR 0001) |
| [0011](adr/0011-midi-conventions-fractal.md) | le **MIDI** : les conventions des pédaliers **Fractal** par défaut (PC = morceau, CC 34 = son, CC 14 = tap), **Ableton** pour le transport (horloge, `Start` / `Stop`, SPP) ; les numéros restent réglables |
| [0012](adr/0012-midi-appareils-et-canaux.md) | le **MIDI** en **appareils et canaux**, sans rôles : tous les appareils écoutés (un se décoche), **une seule horloge** (le premier qui bat, ou celui qu'on choisit), les **numéros appartiennent au canal** ; amende l'ADR 0011 |
| [0013](adr/0013-echange-par-valise.md) | l'**échange de la bibliothèque** entre machines par une **valise** sur une clé USB : tout emporter, un dossier et non une archive, le **contenu** et non la date, le **dernier échange retenu**, l'un ou l'autre jamais un mélange, importer avant d'exporter ; la config reste sur chaque machine |

Deux conséquences structurelles qui ne se lisent nulle part ailleurs : les **profils de carte** sont
des données (`profiles/*.toml`, `model::card`) et `audio-io` ne reçoit qu'une liste de contrôles
mixer dans `Settings` ; les **presets** sont des fichiers de la bibliothèque, lus par `library`, et
non des types de `model`.

## Le coût d'un ajout

La bonne mesure d'une architecture n'est pas le nombre de lignes mais **ce que coûte l'ajout
suivant** : combien de fichiers il faut toucher, et si le compilateur sait les nommer.

| Ajouter… | Fichiers | Qui nomme les endroits |
|---|---|---|
| un **effet** | **6**, tous dans `dsp` | le `match` de `with_effect!` et le test `catalog_covers_every_variant` : le compilateur et un test refusent l'oubli (procédure dans `crates/dsp/CLAUDE.md`) |
| un **backend** | **6** | trois `match` **exhaustifs** (`audio_io::start_with`, `setup::devices::list`, `setup::state`) : le compilateur nomme lui-même ce qui manque |
| un **contexte clavier** | **9** | trois `match` exhaustifs (`on_key` dans `state/keys/mod.rs`, `bindings` dans `state/bindings/mod.rs`, `help_title` dans `ui/windows.rs`) ; l'oracle du clavier signale le contexte non mesuré, mais les autres endroits restent silencieux |

Le détail, parce que c'est la différence qui compte :

- **un effet** : `dsp/src/fx/<nom>.rs`, une variante de `FxSpec`, un bras de `with_effect!`, une
  ligne de `catalog()`, un bras du test de couverture, une section du guide.
- **un backend** : `audio-io/src/<nom>.rs`, le `mod` + la variante de `Kind` + le bras de
  `start_with` dans `audio-io/src/lib.rs`, une variante de `model::BackendKind`, une variante de
  `BackendArg` et sa conversion dans `namfoop/src/cli_audio.rs`, les tailles de buffer proposées
  (`setup/state.rs`), la liste des cartes (`setup/devices.rs`). Trois énumérations, mais le patron
  de l'[ADR 0004](adr/0004-decoupage-en-crates.md) : `BackendArg` est un `ValueEnum` de `clap`, dont
  les commentaires de variantes **engendrent** l'aide de la CLI et `docs/cli.md`.
- **un contexte clavier** : la variante de `Context` et son `if` (`state/context.rs`), le bras de la
  table (`state/bindings/`), le dispatch (`state/keys/mod.rs`), le fichier d'état de l'écran, le `Ctx`
  de l'oracle (`state/keyboard.rs`), la table engendrée (`state/keyboard.ref.txt`), le titre et les
  notes (`ui/windows.rs`), la barre du bas si elle a du dynamique (`ui/footer.rs`), le dessin de
  l'écran (`ui/`).

C'est donc **un écran qui coûte plus qu'un effet**, alors que le modèle d'extensibilité du projet
est celui des effets : une déclaration unique, un `match` exhaustif, un test de couverture. Ce qui
fait la différence est le nombre de consommateurs **silencieux** — qui ne cassent pas la compilation
quand on les oublie.

## Règles vérifiées

`scripts/check-arch.sh` (appelé par `scripts/check.sh`, donc en CI) lit `cargo metadata` et refuse
les arêtes interdites. Seules les dépendances normales et de build comptent (pas les
dev-dependencies). Une crate nouvelle ajoute ses règles à la table du script **et** à celle-ci.

| Crate     | Règle                                                       |
|-----------|-------------------------------------------------------------|
| `nam-sys` | seulement `thiserror` (+ `cc` en build)                     |
| `dsp`     | interdit `alsa`, `clap`, `ratatui`, `libc`, `namfoop`, `audio-io`, `midi-io`, `engine`, `model` ; aucun thread créé (contrôle textuel) |
| `audio-io` | interdit `clap`, `ratatui`, `dsp`, `nam-sys`, `namfoop`, `toml`, `engine`, `serde`, `midi-io` |
| `midi-io` | seulement `alsa`, `libc`, `log`, `rtrb`, `thiserror` (aucune crate du projet) |
| `engine`  | interdit `clap`, `ratatui`, `namfoop`, `toml`, `alsa` (la carte passe par `audio-io`), `midi-io` |
| `model`   | seulement `serde`, `toml`, `thiserror` (aucune crate du projet) |
| `library` | interdit `audio-io`, `midi-io`, `engine`, `clap`, `ratatui`, `alsa` |
| tout le code | aucun `audio_io::start` / `start_with` hors d'`engine` et `audio-io` (contrôle textuel) |
| tout le code | aucun `midi_io::open` hors de `cli_midi.rs` et `midi_cmd.rs` (contrôle textuel) : ailleurs, on reçoit un port déjà ouvert, et la règle « un port absent n'empêche jamais de démarrer » tient en un endroit |
| `namfoop` | aucune couleur nommée (`Color::…`) hors de `theme.rs` (contrôle textuel) |
| toutes     | un `CLAUDE.md` par crate, cité dans la carte racine et ici ; `.rs` > 1 000 lignes refusé (`check-docs.sh`) |
