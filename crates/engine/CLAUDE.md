# engine — session audio

**Responsabilité.** La seule crate qui démarre un flux audio : ouvre la carte via `audio-io`,
vérifie la fréquence et le routage contre les canaux de la carte, route la basse dans une chaîne
`dsp` échangeable à chaud (`dsp::swap`, fondu), et publie les vumètres. C'est ici que viendront
une seule fois mlock / cœur dédié (#3), presets (#6), MIDI (#7) et reprise sur carte perdue (#46).

**API** (`lib.rs`, rester petit) :
- `Engine::start(EngineConfig { audio, routing, max_block, fade_ms, quiet, limiter_meter }, chaîne)`
  -> `Engine` ; erreurs `thiserror` (`error.rs`), messages vus par l'utilisateur (ne pas changer) ;
- `send_chain(chain)` (rend la chaîne si une autre attend), `idle()`, `meters()` (crêtes et charge
  remises à zéro, écrêtages cumulés, réduction du limiteur), `xruns()`, `sample_rate()`, `n_in()`,
  `n_out()`, `describe()` (routage) ; la carte est rendue au drop ;
- `router` (`Routing`, `OutputRoute`, `Source` avec `FromStr`, `Router`), `meters` (`Meters`,
  `fetch_max`, `take`).

`limiter_meter` : le même `Arc<AtomicU32>` doit être donné à la chaîne (`LimiterSpec::meter`).

**Règles temps réel** (`CLAUDE.md` racine) : `Callback::process` et `Router::process` n'allouent
pas, ne verrouillent pas, ne loguent pas et ne paniquent pas ; un canal hors limites est ignoré
dans le callback ET refusé dans `start`. Tests : `callback_does_not_allocate`,
`process_does_not_allocate`.

**Interdit** (`scripts/check-arch.sh`) : `clap`, `ratatui`, `namfoop`, `toml`, et l'accès direct à
`alsa` / `jack` (tout passe par `audio-io`). Les options CLI de routage restent dans le binaire
(`cli_routing.rs`).

**Tester** : `cargo test -p engine` (chemin Engine = chaîne nue en régime établi, session `sim`
avec deux chaînes successives, routage refusé), puis
`namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`.
