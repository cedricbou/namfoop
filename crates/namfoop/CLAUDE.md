# namfoop — binaire CLI

**Responsabilité.** La ligne de commande (clap), les écrans plein écran (ratatui) et une fonction
mince par commande, qui appelle `engine` (audio), `library` (sons), `model` (config) et `dsp`.

**Fichiers.**
```
crates/namfoop/   binaire CLI
cli_audio.rs    options audio clap (AudioArgs, BackendArg) -> audio_io::Settings (CLI > config > défaut),
                to_audio (model::BackendKind -> audio_io::Kind)
cli_routing.rs  options de routage clap (RoutingArgs, --route N=SOURCE) -> engine::Routing
tui.rs          commun aux écrans plein écran : Key, poll_key, Session (logs coupés, stderr
                redirigé, terminal restauré), meter, screen_text (tests)
browse/         `browse` : state/ (logique pure + horloge fournie, testée : mod.rs état et
                navigation, compare.rs A/B b / r et référence c via effective_ir / effective_model,
                editor.rs éditeur `e` avec écoute du gain et grain remesuré, blend.rs mélanges,
                tests.rs), ui/ (ratatui, TestBackend : mod.rs listes et barre d'état, spectrum.rs
                panneau ≥ 130 col., editor.rs, tests.rs), mod.rs (index, Engine + ChainSender dans
                le thread de construction, overrides.toml) ; `import --review` l'ouvre sur les
                modèles importés (browse::Start)
setup/          `setup` plein écran : state.rs (logique pure, testée), ui.rs (ratatui, TestBackend),
                audio.rs (vumètres, bip, test des buffers ; engine::raw), devices.rs (cartes par backend)
live.rs         `run` : construit la chaîne, Engine::start, affichage (vumètre, charge DSP)
offline.rs      `render` (WAV -> chaîne -> WAV) et `bench`
latency.rs      mesure de latence par impulsion (câble de bouclage ; engine::raw)
import_cmd.rs   `import` : options clap, bilan affiché (profils calculés à l'affichage)
meta_cmd.rs     `meta` : options clap, affichage d'un modèle
models_cmd.rs   `models` : inventaire affiché
amp.rs          options CLI de la chaîne -> dsp::chain::AmpSpec (priorité CLI > config > défaut)
config.rs       glue de la config (types dans crates/model) : CalibrationExt (repère des vumètres)
calibrate.rs    `calibrate` : enregistre le DI (tap d'entrée d'Engine), calcule le trim, active le niveau « match »
```

**Interdit.** Ouvrir la carte autrement que par `engine` (`scripts/check-arch.sh` refuse tout
`audio_io::start`) ; mettre de la logique audio ou de bibliothèque ici (elle va dans la crate
concernée). Erreurs : `anyhow`.

**Documentation** (vérifiée par `scripts/check-docs.sh`) : textes d'aide clap complets (le test
`every_argument_has_help` échoue sinon), `scripts/gen-cli-doc.sh`, guide, README, CHANGELOG — voir
« Documentation » dans le `CLAUDE.md` racine.

## Tester

- `render`, `run`, `calibrate`, `latency` et `setup` lisent la config de l'utilisateur (carte,
  routage, trim...) : pour des résultats reproductibles (tests, comparaisons), toujours passer
  l'option globale `--no-config` (elle empêche aussi toute écriture).
- `setup` et `browse` sont plein écran : pour les tester de bout en bout, les piloter dans un
  pseudo-terminal (Python `pty`) avec `--no-config --backend sim` ; pour `browse`, une bibliothèque
  JETABLE (`--library <tmp>`) : il écrit les mélanges dans `<library>/cabs/`, ne jamais polluer
  `library/` de l'utilisateur. `NAMFOOP_BROWSE_TRACE=<fichier>` y écrit chaque chaîne réellement
  installée dans le thread audio (modèle, IR, entrée) et chaque grain mesuré par l'éditeur : c'est
  la vérification de ce qu'on entend. La logique se teste dans `setup::state` / `browse::state`.
- Test de fumée : `namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`.

## Faits mesurés

- Niveau d'entrée : les captures Tone3000 n'ont pas d'`input_level_dbu`. Avec la basse de
  l'utilisateur dans l'iTwo (crête ~ -12 dBFS), il a fallu -18 dB d'entrée pour retrouver le son des
  démos (Bassman 800 « presque clair », THD < 5 % jusqu'à -6 dBFS de sinus) -> cible de calibration :
  crête robuste (99,9e centile) à -30 dBFS à l'entrée des modèles.

## Annexe : détail du harnais (reprise de l'ancien `CLAUDE.md` racine)

Le `CLAUDE.md` racine en garde l'essentiel ; voici le texte complet.

### Contexte

namfoop est un multi-effet basse temps réel en CLI (Rust) : modèles NAM + IR + effets, sorties vers
le sondier et les in-ears, presets pilotés par MIDI, cible PC Linux et Raspberry Pi 5. Le
propriétaire joue de la basse sur scène : **la fiabilité en live et la latence priment sur tout**.
Doc utilisateur : `README.md`. Plan initial : étapes 1 à 6 ci-dessous.

**Architecture : lire d'abord [`docs/architecture.md`](docs/architecture.md)** (crates, sens des
dépendances, règles vérifiées par `scripts/check-arch.sh`) et les décisions prises dans
[`docs/adr/`](docs/adr/) (ne pas les rouvrir sans nouvelle mesure). Découpage en cours : #47.

### Dépôt

```
scripts/check.sh          vérification sans matériel (À FAIRE PASSER AVANT TOUT COMMIT)
scripts/check-docs.sh     cohérence code <-> docs (appelé par check.sh)
scripts/check-arch.sh     sens des dépendances entre crates, table de règles en tête (appelé par check.sh)
scripts/gen-cli-doc.sh    régénère docs/cli.md
scripts/install-hooks.sh  hooks : pre-commit (refus sur main + check.sh --quick), commit-msg (Refs #n),
                          pre-push (refus vers main)
scripts/feature.sh        cycle issue -> branche -> PR -> merge (start | pr | merge | status)
docs/                     guide.md (utilisateur), cli.md (généré), architecture.md, adr/ (décisions)
.github/workflows/        CI : check.yml (check.sh --quick, sans bibliothèque de sons),
                          pr-rules.yml (issue référencée, nom de branche, CHANGELOG)
.github/ISSUE_TEMPLATE/   modèles d'issue (fonctionnalité, bug) ; pull_request_template.md
scripts/test-backends.sh  tests matériels sur les deux backends
library/          modèles .nam / IR .wav de l'utilisateur, docs/ des packs, cabs/, overrides.toml
                  (non versionné, ne pas modifier ni supprimer ; tests : bibliothèque jetable)
nam/              zips d'origine : Tone3000, komposition101 (achetés) — non versionnés, ne pas toucher
```

### Commandes

- Rust est dans `~/.cargo/bin` (pas toujours dans le PATH) : `export PATH="$HOME/.cargo/bin:$PATH"`.
- `scripts/check.sh` : fmt --check, clippy -D warnings, `cargo test` (debug), build release, rendu de
  chaque modèle de `library/` (échec si NaN ou silence ; signale les crêtes > +6 dBFS). `--quick` pour
  sauter la bibliothèque.
- Rendus de référence (`crates/dsp/tests/golden.rs`, lancé par `cargo test`, donc par
  `check.sh --quick` et la CI) : signal synthétique dans NAM seul, NAM + IR et la chaîne complète,
  blocs 16 / 48 / 64 / 1000, comparés à `crates/dsp/tests/golden/*.toml` (tolérance relative 1e-3,
  enveloppe ±0,05 dB ; un gain de 0,5 dB le fait échouer). Un déplacement de code ne doit JAMAIS
  les changer. Régénérer (`NAMFOOP_UPDATE_GOLDEN=1 cargo test -p dsp --test golden`) seulement
  pour un changement de son voulu, en le disant dans la PR.
- `cargo test` **en debug** : `assert_no_alloc` n'est actif qu'en debug (ou dans le profil `ci`,
  qui garde les assertions). Le C++ NAM est toujours compilé en -O3 (build.rs).
- Profil `ci` (`NAMFOOP_PROFILE=ci scripts/check.sh`) : tests et binaire compilés une seule fois,
  sans LTO — c'est ce que fait la CI. Le binaire de scène reste `release` (LTO). Les scripts
  utilisent `target/$NAMFOOP_PROFILE/namfoop` (défaut `release`).
- `NAMFOOP_SKIP_CPP=1` : build.rs ne compile pas le C++ (clippy / check uniquement).
- CI (`.github/workflows/check.yml`) : jobs `lint` et `check` en parallèle, sccache (Rust + C++),
  `push` seulement sur main. Mesurer une étape : `gh run view <id> --log` (horodatages des `== `).
- `render`, `run`, `calibrate`, `latency` et `setup` lisent la config de l'utilisateur (carte,
  routage, trim...) : pour des résultats reproductibles (tests, comparaisons), toujours passer
  l'option globale `--no-config` (elle empêche aussi toute écriture).
- `setup` et `browse` sont plein écran : pour les tester de bout en bout, les piloter dans un
  pseudo-terminal (Python `pty`) avec `--no-config --backend sim` ; pour `browse`, une bibliothèque
  JETABLE (`--library <tmp>`) : il écrit les mélanges dans `<library>/cabs/`, ne jamais polluer
  `library/` de l'utilisateur. `NAMFOOP_BROWSE_TRACE=<fichier>` y écrit chaque chaîne réellement
  installée dans le thread audio (modèle, IR, entrée) et chaque grain mesuré par l'éditeur : c'est
  la vérification de ce qu'on entend. La logique se teste dans `setup::state` / `browse::state`.
- Sans carte son, valider un changement audio avec `namfoop render` (même code que `run`) et
  `namfoop bench`. Écouter un rendu n'est pas possible pour un agent : s'appuyer sur les tests, les
  niveaux (crête/RMS) et des comparaisons numériques.

### Cycle de développement (obligatoire)

Toute évolution suit : **issue GitHub -> branche -> PR -> CI verte -> squash merge -> main à jour**.
La protection de branche GitHub n'est pas disponible (dépôt privé, offre gratuite) : le cycle est
imposé par `scripts/feature.sh`, les hooks git (`scripts/install-hooks.sh`) et le workflow
`pr-rules` de la CI.

```sh
scripts/install-hooks.sh                  # une fois par clone
scripts/feature.sh start 12               # reprend l'issue #12 -> branche feat/12-<titre>
scripts/feature.sh start "Titre" bug      # ou crée l'issue (labels : enhancement, bug, documentation,
                                          #   roadmap, dette technique, test matériel)
# ... commits (pre-commit : check.sh --quick ; « Refs #12 » ajouté au message) ...
scripts/feature.sh pr                     # check.sh complet, push, PR « Closes #12 »
scripts/feature.sh pr --refs              # idem mais l'issue reste ouverte (tests matériels à faire)
scripts/feature.sh merge                  # attend la CI, squash merge, supprime la branche, main à jour
scripts/feature.sh status
```

Règles :
- jamais de commit ni de push direct sur `main` (les hooks refusent ; `NAMFOOP_ALLOW_MAIN=1` est
  réservé aux urgences explicitement demandées par l'utilisateur) ;
- une branche = une issue ; le nom `feat|fix|docs/<n°>-...` porte le numéro (vérifié en CI) ;
- la PR référence l'issue (`Closes #n`, ou `Refs #n` si des tests matériels restent à faire) et met
  à jour `CHANGELOG.md` si elle touche au code (vérifié en CI et par `feature.sh pr`) ;
- fusion seulement CI verte (`feature.sh merge` attend et refuse sinon) ; squash uniquement ;
- les tests matériels restants se cochent dans l'issue (label `test matériel`) ; un agent les
  demande à l'utilisateur ;
- les limites découvertes en route deviennent des issues (label `dette technique`) plutôt que
  des notes perdues.

### Documentation (vérifiée par le harnais)

Trois niveaux, tous en français :
- `README.md` : présentation et démarrage rapide ; cite chaque sous-commande ;
- `docs/guide.md` : guide utilisateur par tâche ; explique **chaque** option et sous-commande ;
- `docs/cli.md` : référence **générée** par `scripts/gen-cli-doc.sh` depuis les textes clap.
Plus `CHANGELOG.md` (section « Non publié »).

Pour toute fonctionnalité visible par l'utilisateur, dans le même commit :
1. textes d'aide clap complets (le test `every_argument_has_help` échoue sinon) ;
2. `scripts/gen-cli-doc.sh` ;
3. section du guide (le pourquoi, un exemple dans un bloc ```sh, les pièges) ; README si nouvelle
   sous-commande ;
4. une ligne dans `CHANGELOG.md` > « Non publié ».

`scripts/check-docs.sh` (lancé par `check.sh`) échoue si `docs/cli.md` est périmé, si une option ou
sous-commande manque au guide / README, si un exemple ```sh des docs ne passe pas l'analyse de la CLI
(mode `NAMFOOP_CHECK_ARGS=1`, qui vérifie aussi que les mots-clés -m / -i sont non ambigus quand
`library/` existe), ou si un `.nam` / `.wav` / `.zip` est versionné. Il avertit si du code a changé
sans entrée de changelog.

### Conventions

- Messages utilisateur, logs, docs et commentaires en **français**. Identifiants en anglais.
- Erreurs : `anyhow` dans le binaire, `thiserror` dans les crates bibliothèques.
- `rustfmt.toml` : largeur 110. Clippy sans warning.
- Ne jamais versionner `library/`, `nam/` ni de fichiers audio : les droits sur les modèles et IR
  Tone3000 ne sont pas établis (vérifié par check-docs.sh).
- Toolchain épinglée (`rust-toolchain.toml`) pour que clippy donne le même verdict en local et en CI.
- Mettre à jour ce fichier (architecture, règles, faits mesurés) avec le code.

### Feuille de route

Chaque étape a son issue (label `roadmap`) : `gh issue list --label roadmap`.

1. ✅ Base : backends JACK + ALSA (+ sim), canaux dynamiques, `setup`, `ports`, `latency`.
2. ✅ NAM + IR : nam-sys, dsp, `run`, `render`, `bench`, `models` ; calibration d'entrée, niveau
   « match », limiteur, compteurs d'écrêtage (prototype actuel).
3. Effets : gate, compresseur, EQ paramétrique, crossover LR4 (blend grave clair), chorus, delay, reverb.
4. Presets TOML + banque préchargée + changement de chaîne sans glitch (swap via `rtrb`, crossfade).
5. MIDI (JACK MIDI / ALSA seq) : Program Change -> preset, CC -> paramètres ; entrée micro sur l'entrée 2.
6. Live / Pi 5 : service systemd, démarrage auto, cross-compilation aarch64, bench sur Pi.

### Pistes ouvertes / dette connue

Suivies dans les issues GitHub (label `dette technique`) ; résumé :

- Pics de charge DSP en `run` (15–50 % max contre 3 % de moyenne au bench) : tester `mlockall`,
  gouverneur CPU `performance`, préchauffage des modèles.
- Limiteur sans lookahead (attaque instantanée) : colore les transitoires qui dépassent -1 dBFS ;
  un lookahead de ~1 ms serait plus propre mais ajoute de la latence.
- Crépitements signalés en JACK avec un modèle saturé : origine non confirmée (xruns ou grain).
- Pas encore de lissage des paramètres ni de changement de chaîne à chaud (prévu avec les presets).
- Le C++ NAM n'est pas couvert par `assert_no_alloc` (allocateur Rust seulement).
