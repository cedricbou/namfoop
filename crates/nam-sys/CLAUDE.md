# nam-sys — FFI vers NeuralAmpModelerCore

**Responsabilité.** Charger et faire jouer un modèle `.nam` : le C++ de NeuralAmpModelerCore
(sous-module `vendor/`, C++20, compilé par `build.rs` avec `cc`) derrière une API C minimale.

**API.** `shim/` (nam_shim.h/.cpp) : load, reset, process (f32 <-> double), loudness, slimmable.
Côté Rust (`src/lib.rs`) : `Model::load`, `reset(sample_rate, max_block)`, `process(input, output)`,
`expected_sample_rate`, `loudness_db`, `set_slimmable_size`, `slimmable_breakpoints` ; erreurs
`thiserror`.

**Interdit** (`scripts/check-arch.sh`) : toute dépendance autre que `thiserror` (+ `cc` en build).

**Règles locales.**
- Le C++ NAM est toujours compilé en -O3, même en profil debug (sinon les modèles ne tiennent pas
  le temps réel), avec `-ffast-math -fno-finite-math-only` et `-march=native` / `-mcpu=native`
  (`NAMFOOP_NATIVE=0` : binaire portable ; `-mcpu=cortex-a76` en cross pour le Raspberry Pi 5).
- `NAMFOOP_SKIP_CPP=1` : build.rs ne compile pas le C++ (clippy / check uniquement).
- `process` est appelé dans le callback audio : les règles temps réel de `crates/dsp/CLAUDE.md`
  s'appliquent.

**Faits mesurés / dette.**
- Le C++ NAM n'est pas couvert par `assert_no_alloc` (allocateur Rust seulement).

**Tester seul** : `cargo test -p nam-sys` ; les modèles de démo sont dans
`vendor/NeuralAmpModelerCore/example_models/` (`wavenet.nam`, utilisé par les tests de `dsp`).

La place de cette crate dans le graphe des dépendances : la carte du `CLAUDE.md` racine et
[`docs/architecture.md`](../../docs/architecture.md).
