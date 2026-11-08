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
                  chain.rs (AmpSpec : trim -> NAM -> IR -> niveau -> master -> limiteur), wav.rs
  tests/          propriétés de chaîne (indépendance au bloc) et absence d'allocation (assert_no_alloc)
crates/namfoop/   binaire CLI
  backend/        mod.rs (trait Processor 2in/2out, Config), jack.rs, alsa.rs (direct, streams liés), rt.rs
  live.rs         `run` : chaîne en temps réel, vumètre, charge DSP
  offline.rs      `render` (WAV -> chaîne -> WAV) et `bench`
  latency.rs      mesure de latence par impulsion (câble de bouclage)
  library.rs      recherche par mots-clés dans library/, métadonnées .nam
  amp.rs          options CLI de la chaîne -> dsp::chain::AmpSpec (priorité CLI > config > défaut)
  config.rs       ~/.config/namfoop/config.toml : trim_db, level, target_db, limiter, calibration_wav
  calibrate.rs    `calibrate` : enregistre le DI, calcule le trim, active le niveau « match »
scripts/check.sh          vérification sans matériel (À FAIRE PASSER AVANT TOUT COMMIT)
scripts/check-docs.sh     cohérence code <-> docs (appelé par check.sh)
scripts/gen-cli-doc.sh    régénère docs/cli.md
scripts/install-hooks.sh  hook pre-commit -> check.sh --quick
docs/                     guide.md (utilisateur), cli.md (généré)
.github/workflows/        CI : check.sh --quick (sans bibliothèque de sons)
scripts/test-backends.sh  tests matériels sur les deux backends
library/          modèles .nam / IR .wav de l'utilisateur (non versionné, ne pas modifier ni supprimer)
nam/              zips Tone3000 d'origine (non versionnés, ne pas toucher)
```

## Commandes

- Rust est dans `~/.cargo/bin` (pas toujours dans le PATH) : `export PATH="$HOME/.cargo/bin:$PATH"`.
- `scripts/check.sh` : fmt --check, clippy -D warnings, `cargo test` (debug), build release, rendu de
  chaque modèle de `library/` (échec si NaN ou silence ; signale les crêtes > +6 dBFS). `--quick` pour
  sauter la bibliothèque.
- `cargo test` **en debug** : `assert_no_alloc` n'est actif qu'en debug. Le C++ NAM est toujours
  compilé en -O3 (build.rs), donc les tests restent rapides.
- `render`, `run` et `calibrate` lisent la config de l'utilisateur : pour des résultats
  reproductibles (tests, comparaisons), toujours passer `--no-config`.
- Sans carte son, valider un changement audio avec `namfoop render` (même code que `run`) et
  `namfoop bench`. Écouter un rendu n'est pas possible pour un agent : s'appuyer sur les tests, les
  niveaux (crête/RMS) et des comparaisons numériques.

## Règles temps réel (non négociables)

Dans tout `Stage::process`, `Processor::process` et les callbacks backend :
- aucune allocation (Vec, String, Box, format!, log!), aucun lock, aucune I/O, aucun appel système
  bloquant ; tout est préparé à la construction de l'étage ;
- les buffers peuvent avoir **n'importe quelle longueur** (JACK 64, ALSA 48, render 1000) : découper
  en interne si besoin ; le son ne doit pas dépendre de la taille de bloc (test `chain.rs`) ;
- pas de `panic!` possible en régime établi (`panic = "abort"` en release coupe le son en live).

Tout nouvel étage doit être ajouté au test `crates/dsp/tests/realtime.rs` (absence d'allocation) et,
si pertinent, à `chain.rs` (indépendance au bloc).

## Les deux backends

Tout changement qui touche l'audio doit marcher sur **JACK et ALSA**. Le DSP est commun (trait
`backend::Processor`, 2 entrées / 2 sorties) ; seuls l'accès à la carte et le threading diffèrent.
- JACK : `pw-jack namfoop --device iTwo ...` ; le buffer demandé force le quantum PipeWire (puissances
  de 2 seulement).
- ALSA : `namfoop --backend alsa --device hw:iTwo --buffer 48 --periods 3 ...` ; si PipeWire tient la
  carte, son profil passe à « off » pendant la session puis est restauré (y compris en cas de panic).
  Un SIGKILL laisse le profil à « off » : `pactl set-card-profile <carte> pro-audio` pour réparer.
- Les tests matériels (`scripts/test-backends.sh`) demandent un câble sortie 1 -> entrée 2 et le
  bouton Mixer de l'iTwo côté Playback. **Un agent ne peut pas brancher de câble : demander à
  l'utilisateur**, et prévenir avant de lancer des tests qui émettent des clics ou prennent la carte.
- Ne pas lancer `run` avec un gain audible sans l'accord de l'utilisateur (utiliser
  `--master-db -120` pour les tests de fumée).

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
- Les IR sont normalisées sur leur gain moyen 80 Hz–5 kHz (normaliser l'énergie totale saturait de
  +15 dB dans le grave).

## Pistes ouvertes / dette connue

- Pics de charge DSP en `run` (15–50 % max contre 3 % de moyenne au bench) : tester `mlockall`,
  gouverneur CPU `performance`, préchauffage des modèles.
- Limiteur sans lookahead (attaque instantanée) : colore les transitoires qui dépassent -1 dBFS ;
  un lookahead de ~1 ms serait plus propre mais ajoute de la latence.
- Crépitements signalés en JACK avec un modèle saturé : origine non confirmée (xruns ou grain).
- Pas encore de lissage des paramètres ni de changement de chaîne à chaud (prévu avec les presets).
- Le C++ NAM n'est pas couvert par `assert_no_alloc` (allocateur Rust seulement).

## Feuille de route

1. ✅ Base : backends JACK + ALSA, `ports`, `latency`.
2. ✅ NAM + IR : nam-sys, dsp, `run`, `render`, `bench`, `models` ; calibration d'entrée, niveau
   « match », limiteur, compteurs d'écrêtage (prototype actuel).
3. Effets : gate, compresseur, EQ paramétrique, crossover LR4 (blend grave clair), chorus, delay, reverb.
4. Presets TOML + banque préchargée + changement de chaîne sans glitch (swap via `rtrb`, crossfade).
5. MIDI (JACK MIDI / ALSA seq) : Program Change -> preset, CC -> paramètres ; entrée micro sur l'entrée 2.
6. Live / Pi 5 : service systemd, démarrage auto, cross-compilation aarch64, bench sur Pi.
