# dsp — étages audio mono

**Responsabilité.** Les traitements, sans carte son ni config : étages mono (trait `Stage`),
chaîne, changement de chaîne à chaud, analyses hors ligne.

**Fichiers / API.** `nam.rs`, `ir.rs` (fft-convolver), `gain.rs`, `limiter.rs`, `chain.rs`
(`AmpSpec` : trim -> `pre` -> NAM -> IR -> `post` -> niveau -> master -> limiteur ; `Level` raw /
normalize / match, mesuré avec les effets ; sans `pre`, le trim reste dans l'étage NAM, au bit près ;
`describe()` pour le log `chaîne :`),
`wav.rs`, `blend.rs` (mélanges d'IR .cab.toml : alignement, gain, polarité, calcul au
chargement), `profile.rs` (réponse d'une IR par tiers d'octave, étendue grave/aiguë à -12 dB,
`from_transfer`, distorsion harmonique), `split.rs` (étage parallèle `Split` : copie ou crossover LR4, retards mesurés par corrélation
et compensés, imbricable ; `measure_latency`), `biquad.rs` (biquad RBJ en TDF-II, f64, `response_db` ; réutilisable par le crossover et les
effets de temps), `fx/` (effets : `FxSpec`, catalogue, voir « Ajouter un effet »), `swap.rs` (changement de chaîne à chaud : `SwapHandle`
envoie une `Chain` construite, `SwapChain` fond en ~20 ms, l'ancienne repart vers le contrôle —
jamais libérée en RT). `tests/` : propriétés de chaîne (indépendance au bloc), absence
d'allocation (assert_no_alloc), rendus de référence `golden.rs` + `golden/*.toml` (RMS, crête,
enveloppe 50 ms).

**Interdit** (`scripts/check-arch.sh`) : `alsa`, `jack`, `clap`, `ratatui`, `libc`, `namfoop`,
`audio-io`, `engine`.

## Ajouter un effet

Socle : `src/fx/mod.rs` (`FxSpec`, `ParamInfo`, `Unit`, trait `Effect`, `catalog()`, mini-syntaxe
`nom:param=valeur,...`, TOML avec `type = "<nom>"`). Décisions : `docs/adr/0005-effets-et-chaines.md`.
1. `src/fx/<nom>.rs` : la structure de paramètres (serde, `#[serde(default)]`), `impl Effect`
   (`KIND`, `HELP`, table `PARAMS` en français, `get`, `put`, `build` ; `tail_seconds` si l'effet
   sonne plus de 10 s après la fin du signal, ≤ 90 s aux réglages extrêmes) et l'étage (`Stage`) ;
   tout ce qui alloue se fait dans `build`.
2. Une variante dans `FxSpec`, une branche dans la macro `with_effect!`, une ligne dans
   `catalog()`, une branche dans le test `catalog_covers_every_variant` (le `match` l'impose).
3. Rien d'autre côté tests : `tests/realtime.rs` (pas d'allocation), `tests/chain.rs` (indépendance
   au bloc) et `tests/fx.rs` (sortie finie, silence après la queue, `reset`) passent tout le
   catalogue, aux défauts et aux bornes de chaque paramètre (`FxSpec::extremes`). Un rendu de
   référence propre à l'effet peut s'ajouter (`golden/fx-<nom>.toml`) ; ne jamais régénérer les
   existants.
4. Le guide (section des effets) : à quoi sert l'effet, chaque paramètre, un exemple ```sh.

## Règles temps réel (non négociables)

Dans tout `Stage::process`, `Processor::process`, `Router::process`, le callback d'`engine` et les
callbacks backend :
- aucune allocation (Vec, String, Box, format!, log!), aucun lock, aucune I/O, aucun appel système
  bloquant ; tout est préparé à la construction de l'étage ;
- les buffers peuvent avoir **n'importe quelle longueur** (JACK 64, ALSA 48, render 1000) : découper
  en interne si besoin ; le son ne doit pas dépendre de la taille de bloc (test `chain.rs`) ;
- pas de `panic!` possible en régime établi (`panic = "abort"` en release coupe le son en live) :
  un index de canal venu de la config est vérifié hors temps réel (`Router::validate`) ET protégé
  dans le callback.

Tout nouvel étage doit être ajouté au test `crates/dsp/tests/realtime.rs` (absence d'allocation) et,
si pertinent, à `chain.rs` (indépendance au bloc).

## Tester

- `cargo test` **en debug** : `assert_no_alloc` n'est actif qu'en debug (ou dans le profil `ci`,
  qui garde les assertions). Le C++ NAM est toujours compilé en -O3 (build.rs).
- Rendus de référence (`crates/dsp/tests/golden.rs`, lancé par `cargo test`, donc par
  `check.sh --quick` et la CI) : signal synthétique dans NAM seul, NAM + IR et la chaîne complète,
  blocs 16 / 48 / 64 / 1000, comparés à `crates/dsp/tests/golden/*.toml` (tolérance relative 1e-3,
  enveloppe ±0,05 dB ; un gain de 0,5 dB le fait échouer). Un déplacement de code ne doit JAMAIS
  les changer. Régénérer (`NAMFOOP_UPDATE_GOLDEN=1 cargo test -p dsp --test golden`) seulement
  pour un changement de son voulu, en le disant dans la PR.
- Sans carte son, valider un changement audio avec `namfoop render` (même code que `run`) et
  `namfoop bench`. Écouter un rendu n'est pas possible pour un agent : s'appuyer sur les tests, les
  niveaux (crête/RMS) et des comparaisons numériques.

## Faits mesurés

- Bench x86 (bloc 64, 48 kHz) : WaveNet 3 canaux ≈ 21 µs (1,6 % du budget), 8 canaux ≈ 42 µs (3,2 %).
- Les IR sont normalisées sur leur gain moyen 80 Hz–5 kHz (normaliser l'énergie totale saturait de
  +15 dB dans le grave).

- Effets (#81, `bench --fx`, bloc 64, x86) : `gain` 0,02 µs, `gate` 0,15 µs (0,01 % du budget),
  `comp` 0,85 µs (0,06 %). Comp : détecteur de crête à relâche fixe de 10 ms, puis réduction lissée
  en dB (attaque / relâche) ; simulé à -6 dBFS, ratio 4, seuil -20 : 10,4 dB de réduction à 1 kHz
  pour 10,5 visés, 9,8 dB à 41 Hz (l'enveloppe redescend entre deux crêtes d'un mi grave).
- Filtres (#82, `bench --fx`, bloc 64) : ~0,2 µs par bande (0,01–0,02 % du budget). Mesurer un
  filtre au sinus avec le rapport des RMS, pas la crête (à 16 kHz, 3 échantillons par période :
  la crête échantillonnée sous-estime l'amplitude de ~0,8 dB).
- Crossover (#83) : somme LR4 plate à ± 0,1 dB de 20 Hz à 20 kHz ; une branche retardée de 2 ms
  creuse de plus de 3 dB autour de la coupure sans compensation, moins de 1 dB avec. Retard mesuré
  b7k + IR dg212 : 43–44 échantillons (0,9 ms, l'attaque de l'IR). Alignement du grave clair sur la
  calibration de l'utilisateur : -14,7 dB, niveau final inchangé (RMS -24,4 dBFS) ; coût `split`
  vide (4 biquads) 0,67 µs par bloc de 64 (0,05 %).
- Delay et chorus (#84, `bench --fx`, bloc 64) : `delay` 0,27 µs (0,02 %), `chorus` 1,6 µs (0,12 %).
  Delay : tampon de 2 s alloué à la construction (2^17 floats, 512 Kio), réinjection ≤ 90 %
  (filtres de boucle Butterworth de gain ≤ 1 : gain de boucle < 1, stable) ; queue à -140 dB à
  90 % / 400 ms : 62 s. Chorus : sous `lowcut` / 4, écart < 0,2 dB (passe-haut LR4 sur la voix
  modulée et sur le son direct qu'elle remplace).
- Réverbe (#85) : FDN 8 lignes (30–61 ms, premiers), Householder, passe-bas à un pôle par ligne
  (gain 1 au continu : le RT60 du grave ne bouge pas), passe-haut LR4 en sortie. `bench --fx`,
  bloc 64, x86 : 0,89 µs (0,07 % du budget) ; **à mesurer sur Pi 5 (#44)**. RT60 (Schroeder,
  -5 à -35 dB) à ± 15 % de `decay` de 0,2 à 10 s ; une octave sous `lowcut`, < -12 dB par rapport
  au médium. Avec flush-to-zero (`render`), la queue qui s'éteint coûte autant que du bruit
  (31 s, 0,023 s CPU dans les deux cas). Niveau : ≈ celui de l'entrée à 0,3 s, plus fort (< +10 dB)
  pour une queue de 8 s.

## Dette connue

- `comp` n'a pas de détecteur RMS (crête seulement) : à ajouter si le besoin se confirme à l'écoute.

- Limiteur sans lookahead (attaque instantanée) : colore les transitoires qui dépassent -1 dBFS ;
  un lookahead de ~1 ms serait plus propre mais ajoute de la latence.
- `Delay::set_time` (fondu entre deux têtes) n'est appelé que par les tests : à brancher sur
  `set_param` (#6).
- Pas encore de lissage des paramètres ni de changement de chaîne à chaud (prévu avec les presets).

## Carte détaillée (reprise du `CLAUDE.md` racine)

```
crates/dsp/       étages audio mono (trait Stage) : nam.rs, ir.rs (fft-convolver), gain.rs, limiter.rs,
                  chain.rs (AmpSpec : trim -> NAM -> IR -> niveau -> master -> limiteur), wav.rs,
                  blend.rs (mélanges d'IR .cab.toml : alignement, gain, polarité, calcul au chargement),
                  profile.rs (réponse d'une IR par tiers d'octave, étendue grave/aiguë à -12 dB),
                  swap.rs (changement de chaîne à chaud : SwapHandle envoie une Chain construite,
                  SwapChain fond en ~20 ms, l'ancienne repart vers le contrôle — jamais libérée en RT)
  tests/          propriétés de chaîne (indépendance au bloc), absence d'allocation (assert_no_alloc),
                  rendus de référence golden.rs + golden/*.toml (RMS, crête, enveloppe 50 ms)
```
