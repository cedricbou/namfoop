# CLAUDE.md — namfoop

Multi-effet basse temps réel en CLI (Rust) : modèles NAM + IR + effets, sorties sondier / in-ears,
presets MIDI, PC Linux et Raspberry Pi 5. **La fiabilité en live et la latence priment sur tout.**
Doc utilisateur : `README.md`. Architecture : [`docs/architecture.md`](docs/architecture.md) et
décisions [`docs/adr/`](docs/adr/) (ne pas les rouvrir sans nouvelle mesure).

## Carte (chaque crate a son `CLAUDE.md` : le lire avant d'y travailler)

```
crates/nam-sys/   FFI C++ NeuralAmpModelerCore            interdit : tout sauf thiserror (+cc)
crates/dsp/       étages mono, chaîne, swap, analyses     interdit : carte son, clap, ratatui, config
                  RÈGLES TEMPS RÉEL (non négociables) et rendus de référence : crates/dsp/CLAUDE.md
crates/audio-io/  ALSA, JACK, sim, priorité RT             interdit : clap, config, dsp, engine
crates/model/     types sérialisables (config, routage)   interdit : toute crate du projet, logique
crates/engine/    session audio : seule à ouvrir la carte  interdit : clap, ratatui, alsa/jack directs
crates/library/   bibliothèque, import, profils            interdit : audio-io, engine, clap, ratatui
crates/namfoop/   binaire : CLI, écrans, une fonction par commande ; anyhow
scripts/          check.sh (avant tout commit), check-docs.sh, check-arch.sh, gen-cli-doc.sh,
                  install-hooks.sh, feature.sh, test-backends.sh (matériel)
docs/             guide.md, cli.md (généré), architecture.md, adr/
library/, nam/    sons et zips de l'utilisateur : non versionnés, ne pas modifier (tests : copie jetable)
```

Où aller pour quoi : son / étage / chaîne -> `dsp` ; carte, format, latence -> `audio-io` ;
routage, vumètres, changement de chaîne en live -> `engine` ; config -> `model` ; import, IR
conseillées, profils -> `library` ; option, écran, commande -> `namfoop`.

## Commandes

- Rust est dans `~/.cargo/bin` : `export PATH="$HOME/.cargo/bin:$PATH"`.
- `scripts/check.sh` : fmt, clippy -D warnings, tests (debug), build, docs, architecture, rendu de
  chaque modèle de `library/` ; `--quick` sans la bibliothèque. Profil `ci`
  (`NAMFOOP_PROFILE=ci`) : ce que fait la CI (jobs `lint` et `check`, sccache).
- Toujours `--no-config` pour des résultats reproductibles (n'écrit rien non plus).
- Audio : tout changement doit marcher sur **JACK et ALSA** (matériel) et **sim** (agents). Un
  agent ne branche pas de câble : demander à l'utilisateur, prévenir avant de prendre la carte ;
  jamais de `run` audible sans accord (`--master-db -120`). Détails : `crates/audio-io/CLAUDE.md`.

## Cycle de développement (obligatoire)

**issue -> branche -> PR -> CI verte -> squash merge -> main à jour**, imposé par
`scripts/feature.sh`, les hooks (`scripts/install-hooks.sh`) et la CI `pr-rules`.

```sh
scripts/feature.sh start 12        # ou start "Titre" bug ; branche feat/12-...
scripts/feature.sh pr              # check.sh complet, push, PR « Closes #12 » (--refs : reste ouverte)
scripts/feature.sh merge           # attend la CI, squash merge, main à jour
```

- jamais de commit ni de push sur `main` (`NAMFOOP_ALLOW_MAIN=1` : urgence demandée par l'utilisateur) ;
- une branche = une issue (`feat|fix|docs/<n°>-...`) ; la PR met à jour `CHANGELOG.md` si elle
  touche au code ; `Refs #n` tant que des tests matériels restent (label `test matériel`, à
  demander à l'utilisateur) ;
- une PR de déplacement ne change aucun comportement ; une issue touche au plus deux crates ;
- les limites découvertes deviennent des issues (label `dette technique`).

## Documentation (vérifiée par `scripts/check-docs.sh`)

En français : `README.md` (chaque sous-commande), `docs/guide.md` (chaque option, le pourquoi, un
exemple ```sh), `docs/cli.md` (généré par `scripts/gen-cli-doc.sh`), `CHANGELOG.md` (« Non
publié »). Pour une fonctionnalité visible : aide clap, cli.md, guide (+ README), changelog, dans le
même commit. check-docs vérifie aussi les exemples, l'absence de sons versionnés, un `CLAUDE.md`
par crate cité ici et dans `docs/architecture.md`, et la taille des fichiers (> 600 lignes :
avertissement, > 1 000 : échec).

## Conventions

- Messages, logs, docs, commentaires en **français** ; identifiants en anglais.
- Erreurs : `anyhow` dans le binaire, types d'erreur propres (`thiserror`) dans les bibliothèques.
- `rustfmt.toml` largeur 110, clippy sans warning, toolchain épinglée.
- Ne jamais versionner `library/`, `nam/` ni de fichiers audio (droits Tone3000 non établis).
- Tenir à jour le `CLAUDE.md` de la crate touchée (fichiers, règles, faits mesurés, dette).

## Feuille de route (`gh issue list --label roadmap`)

1. ✅ Base : backends JACK + ALSA (+ sim), `setup`, `ports`, `latency`.
2. ✅ NAM + IR, calibration, niveau « match », limiteur.
3. Effets : gate, compresseur, EQ, crossover LR4, chorus, delay, reverb.
4. Presets TOML + banque préchargée + changement de chaîne sans glitch.
5. MIDI : Program Change -> preset, CC -> paramètres ; entrée micro.
6. Live / Pi 5 : service systemd, démarrage auto, cross-compilation aarch64.
