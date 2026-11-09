# CLAUDE.md — guide pour les agents (et les humains) qui travaillent sur namfoop

namfoop est un multi-effet basse temps réel en CLI (Rust) : modèles NAM + IR + effets, sorties vers
le sondier et les in-ears, presets pilotés par MIDI, cible PC Linux et Raspberry Pi 5. Le
propriétaire joue de la basse sur scène : **la fiabilité en live et la latence priment sur tout**.
Doc utilisateur : `README.md`. Plan initial : étapes 1 à 6 ci-dessous.

## Disposition

```
crates/nam-sys/   FFI vers NeuralAmpModelerCore (sous-module vendor/, C++20, compilé par build.rs avec cc)
  shim/           API C (nam_shim.h/.cpp) : load, reset, process (f32 <-> double), loudness, slimmable
crates/dsp/       étages audio mono (trait Stage) : nam.rs, ir.rs (fft-convolver), gain.rs, limiter.rs,
                  chain.rs (AmpSpec : trim -> NAM -> IR -> niveau -> master -> limiteur), wav.rs,
                  blend.rs (mélanges d'IR .cab.toml : alignement, gain, polarité, calcul au chargement),
                  swap.rs (changement de chaîne à chaud : SwapHandle envoie une Chain construite,
                  SwapChain fond en ~20 ms, l'ancienne repart vers le contrôle — jamais libérée en RT)
  tests/          propriétés de chaîne (indépendance au bloc) et absence d'allocation (assert_no_alloc)
crates/namfoop/   binaire CLI
  backend/        mod.rs (trait Io : canaux dynamiques ; Processor ; AudioArgs -> Settings), jack.rs,
                  alsa.rs (direct, streams liés, probe), sim.rs (carte simulée), rt.rs
  router.rs       routage temps réel : entrée basse -> chaîne -> sorties (processed/di/mic/mute)
  tui.rs          commun aux écrans plein écran : Key, poll_key, Session (logs coupés, stderr
                  redirigé, terminal restauré), meter, screen_text (tests)
  browse/         `browse` : state.rs (logique pure + horloge fournie, testée), ui.rs (TestBackend),
                  mod.rs (index, audio routeur + SwapChain, thread de construction des chaînes)
  setup/          `setup` plein écran : state.rs (logique pure, testée), ui.rs (ratatui, TestBackend),
                  audio.rs (vumètres, bip, test des buffers), devices.rs (cartes par backend)
  live.rs         `run` : chaîne en temps réel, vumètre, charge DSP
  offline.rs      `render` (WAV -> chaîne -> WAV) et `bench`
  latency.rs      mesure de latence par impulsion (câble de bouclage)
  library.rs      index de library/ (identifiants m…/i…/c…), recherche (id > chemin > mots entiers >
                  version de base), métadonnées .nam
  recommend.rs    besoin d'IR (gear_type) et IR conseillées (IR préférée du modèle, marque, baffle
                  basse, mélanges)
  import.rs       `import` : zips / dossiers -> library/{nam,ir,docs}/<pack>/, doublons par contenu,
                  validation (modèle joué, IR lue), type de capture (nom > dossiers > métadonnées),
                  IR fournie avec le pack
  overrides.rs    library/overrides.toml (corrections par empreinte du contenu, jamais dans le .nam)
                  et `meta` (afficher / corriger) ; appliqué par Library::meta partout
  amp.rs          options CLI de la chaîne -> dsp::chain::AmpSpec (priorité CLI > config > défaut)
  config.rs       ~/.config/namfoop/config.toml : trim_db, level, target_db, limiter, calibration_wav,
                  [audio] (backend, device, buffer...), [routing] (bass_in, mic_in, outputs)
  calibrate.rs    `calibrate` : enregistre le DI, calcule le trim, active le niveau « match »
scripts/check.sh          vérification sans matériel (À FAIRE PASSER AVANT TOUT COMMIT)
scripts/check-docs.sh     cohérence code <-> docs (appelé par check.sh)
scripts/gen-cli-doc.sh    régénère docs/cli.md
scripts/install-hooks.sh  hooks : pre-commit (refus sur main + check.sh --quick), commit-msg (Refs #n),
                          pre-push (refus vers main)
scripts/feature.sh        cycle issue -> branche -> PR -> merge (start | pr | merge | status)
docs/                     guide.md (utilisateur), cli.md (généré)
.github/workflows/        CI : check.yml (check.sh --quick, sans bibliothèque de sons),
                          pr-rules.yml (issue référencée, nom de branche, CHANGELOG)
.github/ISSUE_TEMPLATE/   modèles d'issue (fonctionnalité, bug) ; pull_request_template.md
scripts/test-backends.sh  tests matériels sur les deux backends
library/          modèles .nam / IR .wav de l'utilisateur, docs/ des packs, cabs/, overrides.toml
                  (non versionné, ne pas modifier ni supprimer ; tests : bibliothèque jetable)
nam/              zips d'origine : Tone3000, komposition101 (achetés) — non versionnés, ne pas toucher
```

## Commandes

- Rust est dans `~/.cargo/bin` (pas toujours dans le PATH) : `export PATH="$HOME/.cargo/bin:$PATH"`.
- `scripts/check.sh` : fmt --check, clippy -D warnings, `cargo test` (debug), build release, rendu de
  chaque modèle de `library/` (échec si NaN ou silence ; signale les crêtes > +6 dBFS). `--quick` pour
  sauter la bibliothèque.
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
  `library/` de l'utilisateur. La logique se teste dans `setup::state` / `browse::state`.
- Sans carte son, valider un changement audio avec `namfoop render` (même code que `run`) et
  `namfoop bench`. Écouter un rendu n'est pas possible pour un agent : s'appuyer sur les tests, les
  niveaux (crête/RMS) et des comparaisons numériques.

## Règles temps réel (non négociables)

Dans tout `Stage::process`, `Processor::process`, `Router::process` et les callbacks backend :
- aucune allocation (Vec, String, Box, format!, log!), aucun lock, aucune I/O, aucun appel système
  bloquant ; tout est préparé à la construction de l'étage ;
- les buffers peuvent avoir **n'importe quelle longueur** (JACK 64, ALSA 48, render 1000) : découper
  en interne si besoin ; le son ne doit pas dépendre de la taille de bloc (test `chain.rs`) ;
- pas de `panic!` possible en régime établi (`panic = "abort"` en release coupe le son en live) :
  un index de canal venu de la config est vérifié hors temps réel (`Router::validate`) ET protégé
  dans le callback.

Tout nouvel étage doit être ajouté au test `crates/dsp/tests/realtime.rs` (absence d'allocation) et,
si pertinent, à `chain.rs` (indépendance au bloc).

## Les backends

Tout changement qui touche l'audio doit marcher sur **JACK et ALSA** (testé sur le matériel) et sur
**sim** (sans matériel, utilisable par les agents). Le DSP est commun (trait `backend::Io`, nombre de
canaux de la carte) ; seuls l'accès à la carte et le threading diffèrent.
- `pw-jack` n'est jamais nécessaire (relance automatique avec la libjack PipeWire si besoin,
  `jack::open_client`) : ne pas l'utiliser dans les docs ni les scripts (vérifié par check-docs.sh).
- JACK : `namfoop --backend jack --device iTwo ...` ; le buffer demandé force le quantum PipeWire
  (puissances de 2 seulement). `--device` = nom exact du périphérique (écrit par setup) ou regex.
- sim : `namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 -m b7k` exerce toute la
  chaîne temps réel sans carte ; `--backend sim setup` ouvre l'assistant.
- ALSA : `namfoop --backend alsa --device hw:iTwo --buffer 48 --periods 3 ...` ; si PipeWire tient la
  carte, son profil passe à « off » pendant la session puis est restauré (y compris en cas de panic).
  Un SIGKILL laisse le profil à « off » : `pactl set-card-profile <carte> pro-audio` pour réparer.
- Les tests matériels (`scripts/test-backends.sh`) demandent un câble sortie 1 -> entrée 2 et le
  bouton Mixer de l'iTwo côté Playback. **Un agent ne peut pas brancher de câble : demander à
  l'utilisateur**, et prévenir avant de lancer des tests qui émettent des clics ou prennent la carte.
- Ne pas lancer `run` avec un gain audible sans l'accord de l'utilisateur (utiliser
  `--master-db -120` pour les tests de fumée).

## Cycle de développement (obligatoire)

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

## Documentation (vérifiée par le harnais)

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

## Conventions

- Messages utilisateur, logs, docs et commentaires en **français**. Identifiants en anglais.
- Erreurs : `anyhow` dans le binaire, `thiserror` dans les crates bibliothèques.
- `rustfmt.toml` : largeur 110. Clippy sans warning.
- Ne jamais versionner `library/`, `nam/` ni de fichiers audio : les droits sur les modèles et IR
  Tone3000 ne sont pas établis (vérifié par check-docs.sh).
- Toolchain épinglée (`rust-toolchain.toml`) pour que clippy donne le même verdict en local et en CI.
- Mettre à jour ce fichier (architecture, règles, faits mesurés) avec le code.

## Faits mesurés (à garder à jour)

- Latence aller-retour iTwo (PC) : ALSA 48×3 = 5,2 ms ; JACK/PipeWire 64 = 6,5–7 ms. Plancher matériel
  ~1–2 ms (USB + convertisseurs). 32 et 16 frames : xruns massifs avec l'iTwo.
- Bench x86 (bloc 64, 48 kHz) : WaveNet 3 canaux ≈ 21 µs (1,6 % du budget), 8 canaux ≈ 42 µs (3,2 %).
- Bibliothèque : 55 modèles, tous au format NAM 0.7.0 `SlimmableContainer` (WaveNet 3ch + 8ch,
  `--size 0` = 3ch), loudness entre -14 et -40 dB (d'où la normalisation à -18 dB). 144 IR, surtout
  48 kHz mono, quelques-unes en 44,1 / 96 kHz (rééchantillonnées au chargement), jusqu'à 6 s (tronquées
  à 200 ms).
- Niveau d'entrée : les captures Tone3000 n'ont pas d'`input_level_dbu`. Avec la basse de
  l'utilisateur dans l'iTwo (crête ~ -12 dBFS), il a fallu -18 dB d'entrée pour retrouver le son des
  démos (Bassman 800 « presque clair », THD < 5 % jusqu'à -6 dBFS de sinus) -> cible de calibration :
  crête robuste (99,9e centile) à -30 dBFS à l'entrée des modèles.
- Métadonnées des packs achetés (komposition101, 84 modèles) : `gear_type = amp` partout, FullRig
  compris ; marque / modèle `T3K-Null`, `tz-make` ; pas d'`input_level_dbu` ; loudness -9 à -13 dB.
  Seul le nom (`_DI`, `_FullRig`) ou le dossier (`Full-Rig Profiles/`) est fiable. Les mêmes IR sont
  livrées dans plusieurs packs (25 doublons sur 266). Formats NAM 0.7.0 A2 SlimmableContainer et
  0.5.4 WaveNet, IR float 32 bits mono/stéréo : tout se charge.
- Deviner « baffle inclus » par le son NE MARCHE PAS en basse : pente aigus (6–10 kHz) / médium
  (300–1500 Hz) sur bruit blanc, de -30 à -1 dB pour les FullRig comme pour les DI, et même au sein
  d'une paire DI/FullRig du même son l'écart va de -7 à +10 dB (baffles basse avec tweeter). Abandonné.
- Les IR sont normalisées sur leur gain moyen 80 Hz–5 kHz (normaliser l'énergie totale saturait de
  +15 dB dans le grave).

## Pistes ouvertes / dette connue

Suivies dans les issues GitHub (label `dette technique`) ; résumé :

- Pics de charge DSP en `run` (15–50 % max contre 3 % de moyenne au bench) : tester `mlockall`,
  gouverneur CPU `performance`, préchauffage des modèles.
- Limiteur sans lookahead (attaque instantanée) : colore les transitoires qui dépassent -1 dBFS ;
  un lookahead de ~1 ms serait plus propre mais ajoute de la latence.
- Crépitements signalés en JACK avec un modèle saturé : origine non confirmée (xruns ou grain).
- Pas encore de lissage des paramètres ni de changement de chaîne à chaud (prévu avec les presets).
- Le C++ NAM n'est pas couvert par `assert_no_alloc` (allocateur Rust seulement).

## Feuille de route

Chaque étape a son issue (label `roadmap`) : `gh issue list --label roadmap`.

1. ✅ Base : backends JACK + ALSA (+ sim), canaux dynamiques, `setup`, `ports`, `latency`.
2. ✅ NAM + IR : nam-sys, dsp, `run`, `render`, `bench`, `models` ; calibration d'entrée, niveau
   « match », limiteur, compteurs d'écrêtage (prototype actuel).
3. Effets : gate, compresseur, EQ paramétrique, crossover LR4 (blend grave clair), chorus, delay, reverb.
4. Presets TOML + banque préchargée + changement de chaîne sans glitch (swap via `rtrb`, crossfade).
5. MIDI (JACK MIDI / ALSA seq) : Program Change -> preset, CC -> paramètres ; entrée micro sur l'entrée 2.
6. Live / Pi 5 : service systemd, démarrage auto, cross-compilation aarch64, bench sur Pi.
