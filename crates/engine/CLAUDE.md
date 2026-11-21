# engine — session audio

**Responsabilité.** La seule crate qui démarre un flux audio : ouvre la carte via `audio-io`,
vérifie le routage contre les canaux de la carte (la fréquence réelle est vérifiée par l'appelant :
une chaîne vide de calibration s'accommode de celle de la carte), route la basse dans une chaîne
`dsp` échangeable à chaud (`dsp::swap`, fondu), et publie les vumètres. La **banque de chaînes
préchargées** vit ici ; c'est elle que le MIDI pilotera, comme la reprise sur carte perdue.

**Interdit** (`scripts/check-arch.sh`) : `clap`, `ratatui`, `namfoop`, `toml`, et l'accès direct à
`alsa` (tout passe par `audio-io`). Les options CLI de routage restent dans le binaire
(`cli_routing.rs`).

## Carte

`lib.rs` doit **rester petit**. Chaque module porte son contrat dans son en-tête `//!` (unités,
valeurs par défaut, cas `None`, garanties temps réel) : c'est là qu'on lit l'API, pas ici.

- `lib.rs` : `Engine::start(EngineConfig, chaîne)` et la session — `send_chain`,
  `take_chain_sender()` (envoi depuis un autre thread), `take_input_tap()` (file `rtrb` de l'entrée
  basse brute), `meters()`, `xruns()`, `describe()` ; la carte est rendue au drop. `limiter_meter`
  n'a de sens que si le **même** `Arc<AtomicU32>` est donné à la chaîne (`LimiterSpec::meter`).
- `raw.rs` : `raw::start(settings, callback, quiet)`, **réservé aux outils de mesure et de
  configuration** (`setup`, `latency`) ; tout le reste passe par `Engine`, et `check-arch.sh`
  refuse tout `audio_io::start` hors d'`engine` / `audio-io`.
- `rt.rs` : les réglages temps réel du processus, une fois par `start` — `mlockall` seulement si
  `memlock` est illimité (sinon `MCL_FUTURE` ferait échouer des allocations), avertissement si le
  gouverneur n'est pas `performance`, `prewarm` (0,25 s de bruit puis 0,5 s de silence au bloc
  réel), `flush_denormals()` (FTZ + DAZ x86_64, FZ aarch64 — ce bloc d'assembleur n'est compilé que
  par `scripts/check-aarch64.sh`), et `meters::LoadHistogram` -> `load_summary()`.
- `bank.rs` : **la banque de chaînes préchargées**, passive (aucun thread à elle) : l'appelant
  précharge quand il a du temps, puis `switch(i)`. Quand il veut vérifier entre les deux qu'une
  demande plus récente n'est pas arrivée, `prewarm(&mut chain)` **puis** `install_warm(chain)` : le
  préchauffage est le gros du coût, et installer à l'aveugle fait entendre une chaîne qu'on ne veut
  plus. Une place en échec ne bloque pas la banque ; une chaîne en **queue de preset** ne revient
  qu'une fois éteinte, et y revenir entre-temps donne `NotReady` (l'appelant la rebâtit, ~80 ms :
  un retard, pas une panne).
- `tempo.rs` : **l'arbitre de tempo** (ADR 0008), machine à états pure, horloge fournie par
  l'appelant. Priorité **tap > horloge > preset** ; une horloge qui meurt garde le dernier tempo et
  passe en `ClockLost` — jamais de saut de son causé par une panne. L'ADR 0007 n'autorisant qu'un
  **seul** écrivain par case, c'est `browse` qui écrit le tempo arbitré. Le rejet de gigue à ±25 %
  a une **porte de sortie** (8 rejets de suite = la source a changé de tempo, la fenêtre repart) :
  sans elle, un saut de 120 à 92 BPM serait rejeté indéfiniment. `clock_start` recale le temps 1,
  `clock_continue` fait tout le reste sans le recaler. Il tient **deux** choses distinctes : la
  période (lissée, publiée à 0,3 BPM près) et la **phase** (`clock_beat_at_us`, où tombe le temps),
  qui ne se déduit pas d'elle — une période juste à 0,2 % près dérive d'une demi-seconde en quatre
  minutes. L'**ancre** de phase est le **premier top d'un flux**, parce qu'une horloge nue ne porte
  pas la position du temps (24 octets identiques par noire) et que beaucoup de sources n'envoient
  aucun transport ; les frontières suivantes se comptent sans recaler le temps 1, sinon le clic
  suivrait la granularité de la boucle d'écran. `realign_downbeat` est la moitié « phase » d'un tap,
  pour qui veut recaler sans rien dire du tempo.
- `metronome.rs` : **le clic** (ADR 0008), trois sons dont ce qui les distingue est l'**attaque**
  (`bois` 0,62 ms, `tic` 0,29 ms, `bip` 12,62 ms — c'est son attaque lente qui le noyait dans un
  retour de basse). Toutes les enveloppes commencent à zéro, donc le premier échantillon non nul
  arrive un cran après le temps : piège de test. Éteint, la **grille des temps continue d'avancer**,
  si bien que rallumer tombe juste.
- `router.rs` : `Routing`, `OutputRoute`, `Source`, `Router`. `with_metronome` **ajoute** le clic
  aux seules sorties qui portent `click = true`, jamais substitué à leur source, et sans générateur
  du tout si aucune ne le demande : un clic parti dans le sondier en concert est une catastrophe, et
  un test le vérifie. Le clic est ajouté **après** le limiteur et n'entre pas dans `RouteStats` :
  sur une sortie déjà au plafond, la somme peut donc écrêter sans que `[CLIP SORTIE]` le dise (dit
  dans le guide ; à reprendre si ça gêne à l'usage).
- `meters.rs` (`Meters`, `fetch_max`, `take`), `error.rs` (erreurs `thiserror` dont les messages
  sont vus par l'utilisateur : ne pas les changer à la légère).

**Règles temps réel** (`CLAUDE.md` racine) : `Callback::process` et `Router::process` n'allouent
pas, ne verrouillent pas, ne loguent pas et ne paniquent pas ; un canal hors limites est ignoré
dans le callback ET refusé dans `start`. Tests : `callback_does_not_allocate` (tap compris),
`process_does_not_allocate`.

## Tester

`cargo test -p engine` (chemin Engine = chaîne nue en régime établi, session `sim` avec deux
chaînes successives, routage refusé), puis
`namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`.

**Un test de flux ne conclut pas sur une machine qui décroche déjà.** La carte simulée compte un
xrun dès que son thread a une période de retard, ce qui mesure la machine et non le code. La règle,
valable pour tout test qui mesure une grandeur dépendante de la machine : prendre une **référence**
dans les mêmes conditions et pour la **même durée** que la mesure ; si la référence est sale, le
dire et **ne pas conclure** ; si elle est propre, refuser une **dégradation** plutôt qu'un
décrochage isolé. Le 0 xrun strict reste sur le matériel (`scripts/test-backends.sh`), où il a un
sens. La formulation complète est le rustdoc de `sim_session_swaps_chains` ; `dsp`
(`tests/budget.rs`) applique la même règle à son repère de coût.

## Faits mesurés

- **Banque préchargée** (x86, release, `cargo test --release -p engine --test bank --
  --nocapture --test-threads=1`, modèle B7K + IR Mesa Powerhouse du bassiste, bloc 64) :

  | | `nam › ir` | parallèle à deux `nam › ir` |
  |---|---|---|
  | construction (`ChainSpec::build`) | 19 ms | 107 ms |
  | préchauffage (`rt::prewarm`) | 26 ms | 54 ms |
  | **bascule d'une chaîne préchargée** | **1 µs** | **1 µs** |
  | remise à zéro au retour (`Chain::reset`) | < 0,1 ms | < 0,1 ms |
  | mémoire résidente par chaîne gardée | **534 kio** | **1 219 kio** |

  La **queue de preset** retarde le retour de la chaîne sortante du temps qu'elle sonne (mesures
  dans `crates/dsp/CLAUDE.md`) : c'est voulu, et c'est ce qui rend `NotReady` possible sur une place
  qu'on vient juste de quitter.

  Conséquences retenues : **on précharge toute la banque, sans plafond** (20 presets ≈ 10 à 25 Mio
  de mémoire résidente) ; le préchauffage se paie **une fois**, au préchargement (`send_warm`) ; la
  chaîne sortante est reprise et remise à zéro (gratuit) au lieu d'être reconstruite. Un parallèle
  coûte **plus** que la somme de ses branches à la construction (107 ms contre 2 × 19) parce que
  `dsp::split` mesure le retard de **chaque** branche par corrélation, en y faisant passer une
  sonde — modèles compris.
- **Dénormalisés** : `audio-io` active le flush-to-zero dans les threads ALSA et sim
  (`rt::disable_denormals`) ; `engine` le refait au premier callback, ce qui couvre tout backend
  futur. Mesuré hors ligne (`render`, b7k + IR dg212, bloc 48) : impulsion puis 10 s de silence en
  0,36 s CPU (×28 temps réel), IR seule de 2 s en 0,026 s, sorties identiques octet pour octet avec
  et sans. NAM et la convolution ne produisent donc pas de dénormalisés ici : le correctif sert aux
  effets IIR, delay et réverbe.

## Dette connue

- Pics de charge DSP en `run` : réglés par la priorité FIFO 80 (groupe `realtime`), voir les faits
  mesurés de `crates/audio-io/CLAUDE.md`. Reste à valider 30 min sous charge et sur le Pi.
