# engine — session audio

**Responsabilité.** La seule crate qui démarre un flux audio : ouvre la carte via `audio-io`,
vérifie le routage contre les canaux de la carte (la fréquence réelle est vérifiée par
l'appelant : une chaîne vide de calibration s'accommode de celle de la carte), route la basse dans une chaîne
`dsp` échangeable à chaud (`dsp::swap`, fondu), et publie les vumètres. La **banque de chaînes
préchargées** (`bank.rs`, #6) vit ici ; c'est elle que MIDI (#7) pilotera. Y viendront aussi, une
seule fois, mlock / cœur dédié (#3) et la reprise sur carte perdue (#46).

**API** (`lib.rs`, rester petit) :
- `Engine::start(EngineConfig { audio, routing, max_block, fade_ms, trails_ms, quiet, limiter_meter,
  input_tap }, chaîne)`
  -> `Engine` ; erreurs `thiserror` (`error.rs`), messages vus par l'utilisateur (ne pas changer) ;
- `send_chain(chain)` (rend la chaîne si une autre attend), `take_chain_sender()` (envoi depuis un
  autre thread : `ChainSender::send / send_warm` (chaîne déjà préchauffée) ` / collect / reclaim /
  idle / rate / block`), `idle()`, `take_input_tap()` (file `rtrb`
  de l'entrée basse brute, bloc entier ou bloc perdu compté par `tap_lost()`), `meters()` (crêtes et charge
  remises à zéro, écrêtages cumulés, réduction du limiteur), `xruns()`, `sample_rate()`, `n_in()`,
  `n_out()`, `describe()` (routage) ; la carte est rendue au drop ;
- `raw::start(settings, callback, quiet)` : **réservé aux outils de mesure et de configuration**
  (`setup`, `latency`) ; tout le reste passe par `Engine`. `scripts/check-arch.sh` refuse tout
  `audio_io::start` hors d'`engine` / `audio-io` ;
- `rt::flush_denormals()` : flush-to-zero du thread courant (FTZ + DAZ x86_64, FZ aarch64 — ce
  bloc d'assembleur est compilé par `scripts/check-aarch64.sh`, rien d'autre ne le regarde), appelé
  au premier callback d'`Engine` et de `raw` (#78) ; `render` et `bench` l'appellent aussi ;
- `rt` : réglages temps réel du processus, une fois par `start` / `raw::start` (#3) : `mlockall`
  seulement si `memlock` est illimité (sinon `MCL_FUTURE` ferait échouer des allocations),
  avertissement si le gouverneur n'est pas `performance`, `prewarm` (0,25 s de bruit puis 0,5 s
  de silence au bloc réel) pour la chaîne initiale et chaque `send_chain` / `ChainSender::send` ;
  `meters::LoadHistogram` (charge de chaque callback, pas de 1 %) -> `load_summary()` ;
- `bank` (**banque de chaînes préchargées**, #6) : `Bank::new(sender, max_block)` possède l'envoi
  de chaînes de la session. Elle est **passive** (aucun thread à elle) : l'appelant appelle
  `next_waiting()` + `preload(i, &ChainSpec)` quand il a du temps, puis `switch(i)` -> `Switch`
  (`Installed(ChainMeters)`, `AlreadyPlaying`, `NotReady` — à l'appelant de bâtir comme avant —,
  `Failed(raison)`, `Busy`), `install(chain)` pour une chaîne bâtie ailleurs (l'éditeur de
  `browse`) — ou, quand l'appelant veut vérifier entre les deux qu'une demande plus récente n'est
  pas arrivée (#191), `prewarm(&mut chain)` **puis** `install_warm(chain)` : le préchauffage est le
  gros du coût (26 à 54 ms), et l'installer à l'aveugle fait entendre une chaîne qu'on ne veut
  plus —, `collect()` qui **reprend** la chaîne sortante, la **remet à zéro** et la range dans sa
  place, `invalidate(i)` / `resize(n)` quand la banque change, `state(i)`, `progress()`, `playing()`.
  Une place en échec ne bloque pas la banque. Avec une **queue de preset** (`trails_ms`, #172), la
  chaîne sortante ne revient qu'une fois éteinte : sa place reste `Out`, et y revenir pendant ce
  temps donne `NotReady` (l'appelant la rebâtit, ~80 ms : un retard, pas une panne) ;
- `tempo` (**L'ARBITRE DE TEMPO**, ADR 0008, #196) : `Arbiter`, machine à états **pure** — aucun
  audio, aucun thread, aucune poignée `dsp`, horloge **fournie** par l'appelant (même patron que
  `namfoop::latency::Detector` et `browse::state`). Entrées `preset(bpm)`, `tap(now)`,
  `clock_tick(now)`, `clock_start()`, `clock_stop()`, `poll(now)` ; sorties `bpm()`,
  `source() -> TempoSource::{Preset, Tap, MidiClock, ClockLost}` (avec `label()` pour l'en-tête) et
  `beat_epoch()` (compteur de recalage du **temps 1**, qu'un tap et un `Start` incrémentent : le
  métronome n'a qu'à le suivre). Priorité **tap > horloge > preset** ; un tap détrône l'horloge et
  on n'y revient que sur un événement net. Une horloge qui meurt **garde le dernier tempo** et
  passe en `ClockLost` : jamais de saut de son causé par une panne. C'est elle que MIDI (#7)
  alimentera ; `browse` écrit ensuite la case (#200), l'ADR 0007 n'autorisant **qu'un** écrivain.
- `metronome` (**LE CLIC**, ADR 0008, #197) : `Metronome` + `MetronomeConfig { gain, tempo,
  downbeat, beats_per_bar, sound }`. **Trois sons** (#230), parce que ce qui perce un retour de
  basse ne se choisit qu'à l'oreille — et ce qui les distingue est **l'attaque**, mesurée par
  `the_percussive_sounds_attack_fast_and_die_quickly` : `bois` 0,62 ms / 25 ms (deux partiels
  inharmoniques ×2,6, attaque linéaire de 0,3 ms puis décroissance exponentielle de 7 ms ; le
  défaut, il tient sa hauteur donc l'accent s'entend), `tic` 0,29 ms / 7,3 ms (bruit déterministe
  passé dans un passe-haut à 2 kHz, 3,5 à l'accent : pas de hauteur, seul le filtre change),
  `bip` **12,62 ms** / 24 ms (l'ancien : sinus à enveloppe en cosinus surélevé — c'est son attaque
  lente qui le noyait). Toutes les enveloppes **commencent à zéro**, donc le premier échantillon
  non nul arrive un cran après le temps (piège de test). Les trois cases sont **vivantes** : `gain` linéaire (0 : éteint,
  et c'est l'appelant qui convertit les dB de la config), `tempo` est **la même case** que les
  effets de temps, `downbeat` est un **compteur** dont tout changement recale le temps 1 — c'est ce
  que fournit `tempo::Arbiter::beat_epoch`. Éteint, la **grille des temps continue d'avancer** :
  rallumer tombe juste. Un changement de tempo ne remet **pas** la phase à zéro (seul l'écart entre
  deux temps change).
- `router` (`Routing`, `OutputRoute`, `Source` avec `FromStr`, `Router`) : `with_metronome` **ajoute**
  le clic aux seules sorties qui portent `click = true` — jamais substitué à leur source (les
  in-ears veulent la basse **et** le clic), et si **aucune** sortie ne le demande le générateur
  n'est même pas créé. C'est la garantie qui compte : un clic parti dans le sondier en concert est
  une catastrophe, et un test le vérifie. **Le clic est ajouté après le limiteur** (il n'est pas
  dans la chaîne) et n'entre pas dans `RouteStats` : sur une sortie déjà au plafond, la somme peut
  donc écrêter sans que `[CLIP SORTIE]` le dise. Dit dans le guide ; à reprendre si ça gêne à
  l'usage. `meters` (`Meters`, `fetch_max`, `take`).

`limiter_meter` : le même `Arc<AtomicU32>` doit être donné à la chaîne (`LimiterSpec::meter`).

**Règles temps réel** (`CLAUDE.md` racine) : `Callback::process` et `Router::process` n'allouent
pas, ne verrouillent pas, ne loguent pas et ne paniquent pas ; un canal hors limites est ignoré
dans le callback ET refusé dans `start`. Tests : `callback_does_not_allocate` (tap compris),
`process_does_not_allocate`.

**Interdit** (`scripts/check-arch.sh`) : `clap`, `ratatui`, `namfoop`, `toml`, et l'accès direct à
`alsa` (tout passe par `audio-io`). Les options CLI de routage restent dans le binaire
(`cli_routing.rs`).

**Tester** : `cargo test -p engine` (chemin Engine = chaîne nue en régime établi, session `sim`
avec deux chaînes successives, routage refusé), puis
`namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`.

## Faits mesurés

- **Banque préchargée** (#165, x86, release, `cargo test --release -p engine --test bank --
  --nocapture --test-threads=1`, modèle B7K + IR Mesa Powerhouse du bassiste, bloc 64) :

  | | `nam › ir` | parallèle à deux `nam › ir` |
  |---|---|---|
  | construction (`ChainSpec::build`) | 19 ms | 107 ms |
  | préchauffage (`rt::prewarm`) | 26 ms | 54 ms |
  | **bascule d'une chaîne préchargée** | **1 µs** | **1 µs** |
  | remise à zéro au retour (`Chain::reset`) | < 0,1 ms | < 0,1 ms |
  | mémoire résidente par chaîne gardée | **534 kio** | **1 219 kio** |

  La **queue de preset** (#172) retarde le retour de la chaîne sortante du temps qu'elle sonne
  (mesures dans `crates/dsp/CLAUDE.md`) : c'est voulu, et c'est ce qui rend `NotReady` possible sur
  une place qu'on vient juste de quitter.

  Conséquences retenues : **on précharge toute la banque, sans plafond** (20 presets ≈ 10 à 25 Mio
  de mémoire résidente) ; le préchauffage se paie **une fois**, au préchargement
  (`send_warm`) ; la chaîne sortante est reprise et remise à zéro (gratuit) au lieu d'être
  reconstruite. Un parallèle coûte **plus** que la somme de ses branches à la construction
  (107 ms contre 2 × 19) parce que `dsp::split` mesure le retard de **chaque** branche par
  corrélation, en y faisant passer une sonde — modèles compris.
- Dénormalisés (#78) : `audio-io` active le flush-to-zero dans les threads ALSA et sim
  (`rt::disable_denormals`) ; `engine` le refait au premier callback (tout backend futur couvert).
  Mesuré hors ligne (`render`, b7k + IR dg212, bloc 48) : impulsion puis 10 s de silence en
  0,36 s CPU (×28 temps réel) avant comme après, IR seule de 2 s en 0,026 s avant comme après ;
  sorties identiques octet pour octet. NAM et la convolution ne produisent donc pas de
  dénormalisés ici : le correctif sert aux effets IIR, delay, réverbe (#5).

## Faits mesurés sur les tests

- **`sim_session_swaps_chains` était instable : 3 échecs sur 12 en isolation** (x86 16 cœurs au
  repos), corrigé par #130. Deux causes, et la seconde est la plus instructive :
  1. **les deux fenêtres n'étaient pas comparables** — la référence durait 400 ms sans rien faire,
     la phase d'échange 800 ms **plus** un `rt::prewarm` par `send_chain` (0,25 s de bruit + 0,5 s
     de silence chacun, sur le thread de contrôle). Une machine qui lâche une période par seconde
     montrait donc 0 au repos et 1 pendant les échanges, et le test l'imputait au code. La
     référence dure maintenant exactement aussi longtemps que **toute** la phase d'échange ;
  2. **référence propre, on tolère quand même 1 xrun** : la phase d'échange charge vraiment le
     processeur, et c'est elle qui pousse un flux limite au-delà du bord. Un vrai défaut
     décrocherait à **chaque** échange, donc au moins 2 fois (le test en fait 2).

  Ce qu'on ne fait **pas** : comparer les deux comptes quand la référence décroche déjà. La phase
  d'échange travaille strictement plus, donc sous charge elle décroche proportionnellement plus, et
  aucun seuil ne sépare « échanger coûte du processeur » de « échanger a un défaut ». Une première
  version comparait `swapping <= idle + 1` : mesuré, 48 boucles de charge sur 16 cœurs la font
  échouer. On retombe donc sur la règle de #114 : on le dit et on ne conclut pas.

  Vérifié dans les trois sens : **15 / 15** au repos (contre 9 / 12 avant) ; sous 48 processus de
  charge, le test passe en annonçant « 3 xrun au repos, 2 pendant les échanges — ne conclut pas » ;
  et deux décrochages simulés avec une référence propre le font bien échouer, un seul est toléré.
- `sim_session_swaps_chains` n'exige pas 0 xrun dans l'absolu : la carte simulée en compte un dès
  que son thread a une période de retard, ce qui mesure la machine, pas le code (#114, une PR cassée
  par un runner partagé). Le test prend une **référence** sans échange : si le flux décroche déjà
  là, il ne conclut pas (et le dit) ; s'il est propre, un échange de chaîne ne doit ajouter aucun
  décrochage. Vérifié dans les deux sens : 48 boucles de charge sur 16 cœurs -> le test passe en
  signalant 1 à 3 xruns de référence ; référence forcée à 0 sous la même charge -> il échoue bien.
  Le 0 xrun strict reste sur le matériel (`scripts/test-backends.sh`), où il a un sens.

## Dette connue

- Pics de charge DSP en `run` (#3) : réglés par la priorité FIFO 80 (groupe `realtime`), voir les
  faits mesurés de `crates/audio-io/CLAUDE.md`. Reste à valider 30 min sous charge et sur le Pi.