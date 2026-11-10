# engine — session audio

**Responsabilité.** La seule crate qui démarre un flux audio : ouvre la carte via `audio-io`,
vérifie le routage contre les canaux de la carte (la fréquence réelle est vérifiée par
l'appelant : une chaîne vide de calibration s'accommode de celle du serveur JACK), route la basse dans une chaîne
`dsp` échangeable à chaud (`dsp::swap`, fondu), et publie les vumètres. C'est ici que viendront
une seule fois mlock / cœur dédié (#3), presets (#6), MIDI (#7) et reprise sur carte perdue (#46).

**API** (`lib.rs`, rester petit) :
- `Engine::start(EngineConfig { audio, routing, max_block, fade_ms, quiet, limiter_meter, input_tap }, chaîne)`
  -> `Engine` ; erreurs `thiserror` (`error.rs`), messages vus par l'utilisateur (ne pas changer) ;
- `send_chain(chain)` (rend la chaîne si une autre attend), `take_chain_sender()` (envoi depuis un
  autre thread : `ChainSender::send / collect / idle`), `idle()`, `take_input_tap()` (file `rtrb`
  de l'entrée basse brute, bloc entier ou bloc perdu compté par `tap_lost()`), `meters()` (crêtes et charge
  remises à zéro, écrêtages cumulés, réduction du limiteur), `xruns()`, `sample_rate()`, `n_in()`,
  `n_out()`, `describe()` (routage) ; la carte est rendue au drop ;
- `raw::start(settings, callback, quiet)` : **réservé aux outils de mesure et de configuration**
  (`setup`, `latency`) ; tout le reste passe par `Engine`. `scripts/check-arch.sh` refuse tout
  `audio_io::start` hors d'`engine` / `audio-io` ;
- `router` (`Routing`, `OutputRoute`, `Source` avec `FromStr`, `Router`), `meters` (`Meters`,
  `fetch_max`, `take`).

`limiter_meter` : le même `Arc<AtomicU32>` doit être donné à la chaîne (`LimiterSpec::meter`).

**Règles temps réel** (`CLAUDE.md` racine) : `Callback::process` et `Router::process` n'allouent
pas, ne verrouillent pas, ne loguent pas et ne paniquent pas ; un canal hors limites est ignoré
dans le callback ET refusé dans `start`. Tests : `callback_does_not_allocate` (tap compris),
`process_does_not_allocate`.

**Interdit** (`scripts/check-arch.sh`) : `clap`, `ratatui`, `namfoop`, `toml`, et l'accès direct à
`alsa` / `jack` (tout passe par `audio-io`). Les options CLI de routage restent dans le binaire
(`cli_routing.rs`).

**Tester** : `cargo test -p engine` (chemin Engine = chaîne nue en régime établi, session `sim`
avec deux chaînes successives, routage refusé), puis
`namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`.
