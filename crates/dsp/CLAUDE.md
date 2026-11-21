# dsp — étages audio mono

**Responsabilité.** Les traitements, sans carte son ni config : étages mono (trait `Stage`),
chaîne, changement de chaîne à chaud, analyses hors ligne.

**Interdit** (`scripts/check-arch.sh`) : `alsa`, `clap`, `ratatui`, `libc`, `namfoop`, `audio-io`,
`engine`, **`model`** — c'est ce dernier qui rend vraie la phrase « `dsp` ne connaît pas la
config » : le type `Config` est inaccessible d'ici. En revanche `dsp` **lit et écrit** ses propres
formats (`preset/mod.rs`, `blend.rs`), et un `grep` de `check-arch.sh` vérifie qu'il ne crée
**aucun thread** : c'est à l'appelant de décider où le DSP tourne.

Le rustdoc de cette crate porte les contrats détaillés (unités, valeurs par défaut, cas `None`,
garanties temps réel) ; ce fichier donne la carte, les règles et les mesures.

## Fichiers

Un fichier par ligne : le rôle, et ce qu'il ne faut pas casser. Le reste — unités, valeurs par
défaut, cas limites — est dans le rustdoc.

- `lib.rs` : le trait **`Stage`**, et ses deux méthodes à défaut. `tail_ms()` dit combien l'étage
  peut encore sonner une fois son entrée silencieuse, à **-60 dBFS** près : `Chain` les **somme**,
  `Split` prend le **maximum** de ses branches audibles. `enter_trail()` est appelée une fois, en
  RT, quand la chaîne passe dans la queue de preset : l'étage fige alors son **rythme** (la case de
  tempo est unique et partagée, donc sans ça un delay en `1/8` à 60 BPM s'accélérerait au tempo du
  preset suivant), mais pas ses **niveaux** ; `reset()` lève le gel.
- `nam.rs` le modèle (FFI `nam-sys`), `ir.rs` la convolution (fft-convolver), `limiter.rs` le
  limiteur de sortie, `wav.rs` la lecture et l'écriture WAV.
- `gain.rs` : `Gain` figé, et `LiveGain` qui suit une case `param::Control`. Lecture **une fois par
  bloc**, rampe de `RAMP_MS` = 20 ms comptées en **échantillons**, qui traverse les frontières de
  bloc — jamais une rampe étalée sur le buffer, qui ferait dépendre le son de la taille de bloc. La
  case est lue à la construction **et** à `reset()`, sinon une chaîne préchargée sauterait de 36 dB
  à l'installation.
- `tempo.rs` : **le tempo et les divisions** (ADR 0008). `DIVISIONS`, la table des divisions —
  indice **0 = `libre`**, qui rend la main au temps en millisecondes —, dans l'ordre des noms de
  notes, qui est aussi croissant en durée (`←` / `→` prévisibles) ; chaque entrée porte son `name`
  (`noire`) et son `beats_label` (`un temps`), parce qu'une fraction nue se lit mal : `1/4` est une
  noire, donc **un** temps, et non un quart de temps. `Tempo::{Bpm, Live}` est calqué sur
  `chain::Volume` : figé pour `run` / `render` / `bench` et les rendus de référence, **vivant** pour
  `browse`. Qui *décide* du tempo (preset, tap, horloge MIDI) est l'affaire de l'arbitre d'`engine`.
- `param.rs` : **la case de réglage** (ADR 0007), miroir de `meter.rs` dans l'autre sens — le
  contrôle **possède** la valeur, le temps réel la lit, un seul écrivain par case. Elle ne décrit
  rien : la description reste `fx::ParamInfo`.
- `meter.rs` : les vumètres d'affichage (`Meter`, `BlockMeters { peak, action }`, `BranchMeters`,
  `SplitMeters`, `ChainMeters` préallouée) et l'étage `Probe`, qui ne touche pas au signal.
  `ChainMeters::level_db` publie l'alignement sur le DI mesuré à la construction, que l'écran doit
  montrer.
- `block.rs` : **la chaîne en blocs** (ADR 0006) — `ChainSpec`, `BlockSpec::{Fx, Nam, Ir, Split}`,
  `SplitSpec` / `BranchSpec`. `volume` est le **volume du preset**, poussé **après** l'alignement de
  niveau et avant le master : le seul gain qu'un `Level::Match` n'annule pas. En `Volume::Live`
  l'étage est posé **même au neutre**, sinon personne ne lirait la case. `validate()` refuse un
  parallèle dans une branche (un seul niveau) ; `build()` aligne les branches sans modèle sur la
  première qui en a un ; `build_with_meters()` est seul à poser des sondes.
- `chain.rs` : `Chain`, `Level` (raw / normalize / **match** mesuré sur toute la chaîne), `Volume`,
  `LimiterSpec`, et `AmpSpec` — les options CLI, qui **produisent** une `ChainSpec`.
- `split.rs` : l'étage parallèle, 2 à `MAX_BRANCHES` = 4 branches, mode copie ou bandes (1 à
  3 coupures croissantes, crossover LR4) ; par branche gain, polarité, muet (muette = pas traitée).
  Réglages vérifiés à la construction (`Result`), **jamais** en RT ; retards mesurés par corrélation
  et compensés ; imbricable.
- `biquad.rs` : biquad RBJ en TDF-II, f64, formes passe-bas / passe-haut / cloche / plateaux /
  **passe-tout**, réutilisable par le crossover et les effets de temps.
- `swap.rs` : **le changement de chaîne à chaud**. `SwapChain` fond en ~20 ms **sur l'entrée** des
  deux chaînes, et la chaîne quittée passe dans la **queue de preset**, alimentée en silence, sa
  sortie ajoutée à celle de la chaîne en écoute, pour la durée que la chaîne **annonce**
  (`Stage::tail_ms`). Elle n'est **jamais libérée en RT** : le contrôle la libère (`collect`) ou la
  reprend (`reclaim`) pour la garder bâtie, ce que fait `engine::bank`.
- `fx/` : les effets réglables (`FxSpec`, `catalog()`, trait `Effect`, mini-syntaxe). `Effect::build`
  prend un **`BuildCtx`** `{ sample_rate, max_block, tempo }` et non deux arguments : c'est ce qui
  fait voyager la case de tempo jusqu'à l'étage, et le paramètre vivant suivant passera par là sans
  toucher aux onze effets. `Unit::Division` est un **scalaire comme les autres** (ADR 0005
  préservé) : seul son affichage diffère, et `Unit::format` est le **seul** endroit qui dit comment
  montrer la valeur d'un paramètre, parce que `namfoop fx`, le panneau de `browse` et la
  mini-syntaxe doivent dire la même chose. Le parallèle **n'est pas** un effet : c'est
  `block::SplitSpec`, et `fx::STRUCTURAL_KINDS` le refuse en mini-syntaxe avec un message qui dit où
  le décrire.
- `preset/mod.rs` : les **presets** `.preset.toml`. Chemins **relatifs** à la bibliothèque avec
  l'identifiant court en filet, effets écrits au complet par serde pour que le son soit figé,
  `volume_db` déclaré **avant** `blocks` (en TOML une valeur ne s'écrit pas après un tableau de
  tables) ; `to_blocks()` donne les `BlockSpec` du temps réel.
- `blend.rs` : les mélanges d'IR `.cab.toml` (alignement, gain, polarité), calculés au chargement.
- `bench.rs` : les **chaînes de référence**, du modèle nu au pire cas connu (quatre `nam › ir` en
  parallèle), avec le modèle d'exemple du moteur NAM — embarqué par `include_bytes!`, donc mesurable
  sur une machine où seul le binaire est installé — et une IR de synthèse à graine fixe. Un seul
  endroit les décrit, parce que deux mesures s'en servent et doivent parler des mêmes chaînes : le
  repère de coût par bloc (`tests/budget.rs`, hors ligne) et `namfoop latency --chains`, sur la
  carte. `Level::Raw`, aucun limiteur : rien ne doit dépendre des réglages de l'utilisateur.
- `profile.rs` : la réponse d'une IR par tiers d'octave, étendue grave / aiguë à -12 dB,
  `from_transfer`, distorsion harmonique, et **`loudness_db`** (ITU-R BS.1770), dont se sert
  `Level::Match`.
- `tests/` : propriétés de chaîne (indépendance au bloc), absence d'allocation (`assert_no_alloc`),
  rendus de référence (`golden.rs` + `golden/*.toml`), budget de coût par bloc (`budget.rs` +
  `budget.toml`).

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
- les buffers peuvent avoir **n'importe quelle longueur** (ALSA 48, render 1000, 16 en test) : découper
  en interne si besoin ; le son ne doit pas dépendre de la taille de bloc (test `chain.rs`) ;
- pas de `panic!` possible en régime établi (`panic = "abort"` en release coupe le son en live) :
  un index de canal venu de la config est vérifié hors temps réel (`Router::validate`) ET protégé
  dans le callback.

Tout nouvel étage doit être ajouté au test `crates/dsp/tests/realtime.rs` (absence d'allocation) et,
si pertinent, à `chain.rs` (indépendance au bloc).

## Tester

- `cargo test` **en debug** : `assert_no_alloc` n'est actif qu'en debug (ou dans le profil `ci`,
  qui garde les assertions). Le C++ NAM est toujours compilé en -O3 (build.rs).
- **Rendus de référence** (`tests/golden.rs`, lancé par `cargo test`, donc par `check.sh --quick` et
  la CI) : signal synthétique dans NAM seul, NAM + IR et la chaîne complète, blocs 16 / 48 / 64 /
  1000, comparés à `tests/golden/*.toml` (tolérance relative 1e-3, enveloppe ±0,05 dB ; un gain de
  0,5 dB le fait échouer). Un déplacement de code ne doit **jamais** les changer. Régénérer
  (`NAMFOOP_UPDATE_GOLDEN=1 cargo test -p dsp --test golden`) seulement pour un changement de son
  voulu, en le disant dans la PR.
- **Budget de coût par bloc** (`tests/budget.rs`, valeurs dans `tests/budget.toml`) : le même filet,
  pour la **performance**, sur quatre chaînes aux blocs 48 et 64. Le seuil est un **rapport à un
  repère mesuré dans la même exécution** et non des µs : sur cinq exécutions, le rapport varie de
  6 % au pire quand les µs varient de 40 % avec la fréquence du processeur. Marge 1,4, donc une
  régression de 40 % est refusée. Le test **ne conclut pas** sans optimisation ni sur une machine
  dont le repère disperse de plus de 50 % : `check.sh` le lance donc explicitement en release. Ses
  chiffres portent sur le **modèle d'exemple** du sous-module NAM, seul disponible en CI : ils
  traquent les régressions du code, pas le coût de la bibliothèque de l'utilisateur, que
  `namfoop bench` mesure.
- Sans carte son, valider un changement audio avec `namfoop render` (même code que `run`) et
  `namfoop bench`. Écouter un rendu n'est pas possible pour un agent : s'appuyer sur les tests, les
  niveaux (crête/RMS) et des comparaisons numériques.

## Faits mesurés

### Coût par bloc

x86, release, bloc de 64 à 48 kHz, donc un budget de **1333 µs** par bloc (1000 µs au bloc 48, où
les coûts absolus sont les mêmes). Modèles mesurés par `namfoop bench`, effets et structures par
`bench --fx` et les tests `--release ... -- --nocapture` nommés.

| Élément | µs / bloc | part du budget |
|---|---|---|
| NAM WaveNet 3 canaux (modèle d'exemple) | 21 | 1,6 % |
| NAM WaveNet 8 canaux | 42 | 3,2 % |
| NAM b7k | 45 | 3,4 % |
| le modèle le plus lourd de la bibliothèque | 104 | 7,8 % |
| `gain` | 0,02 | — |
| `gate` | 0,15 | 0,01 % |
| `comp` | 0,85 | 0,06 % |
| un filtre, par bande | ~0,2 | 0,01–0,02 % |
| `delay` | 0,27 | 0,02 % |
| `chorus` | 1,6 | 0,12 % |
| `reverb` | 0,89 | 0,07 % |
| parallèle, copie à 4 branches | 0,06 | — |
| parallèle, 2 / 3 / 4 bandes | 0,67 / 1,74 / 3,23 | jusqu'à 0,24 % |
| table de vumètres (7 sondes) | +0,24 | +0,02 % |

`namfoop bench` sur les 139 modèles de la bibliothèque, bloc 64 : 41 µs le plus léger, 42 µs de
médiane, 104 µs le plus lourd (3,1 à 7,8 %), 246 µs au pire cas. **Le Pi 5 reste à mesurer** : lui
seul dira s'il faut une limite dure.

**Plusieurs modèles en parallèle** (`cargo test --release -p dsp --test chain -- --nocapture
cost_of_several_models`, `NAMFOOP_BENCH_MODEL=<fichier.nam>` pour un modèle de la bibliothèque) :
le coût est **linéaire** en nombre de modèles, le parallèle lui-même ne coûtant que 0,06 à 3 µs.
Moyenne / pire cas par bloc de 64, chaîne `◆ copie [nam › ir | …]` :

| modèles | b7k | le plus lourd de la bibliothèque |
|---|---|---|
| 1 | 45 µs (3,4 %) / 138 µs (10 %) | 104 µs (7,8 %) / 248 µs (19 %) |
| 2 | 92 µs (6,9 %) / 160 µs (12 %) | 209 µs (16 %) / 360 µs (27 %) |
| 3 | 139 µs (10 %) / 202 µs (15 %) | 314 µs (24 %) / 601 µs (45 %) |
| 4 | 184 µs (14 %) / 256 µs (19 %) | 417 µs (31 %) / 509 µs (38 %) |

Seuil retenu avec l'utilisateur : `browse` **alerte dès 2 modèles actifs** et ne refuse rien (le
coût est linéaire, et 2 modèles restent confortables sur x86). Lire un vumètre juste après le
démarrage donne le transitoire (gate parti fermé, dépassement d'un filtre) : `clear()` puis relire,
c'est ce que fait l'affichage à 25 images/s.

### Niveaux et alignement

- **Le volume du preset est vivant** (ADR 0007) : il ne rebâtit aucune chaîne. Mesuré en
  pseudo-terminal (x86, profil `ci`, backend sim, bibliothèque jetable, `level = "match"`, de la
  touche à la ligne `NAMFOOP_BROWSE_TRACE`) : **21 ms**, le plancher de la boucle d'affichage de
  `browse`, et **aucune chaîne installée**. **Pi 5 non mesuré.**
- **L'alignement sur le DI compare des loudness pondérées K**, et non des RMS larges :
  `profile::loudness_db` (ITU-R BS.1770 sans porte ; les deux biquads de la norme reconstruits par
  les formules RBJ aux fréquences et Q de ses pôles, donc valables à **toute** fréquence
  d'échantillonnage — vérifié à 44,1 / 48 / 96 kHz). Mesuré sur la bibliothèque de l'utilisateur
  (`render -m <id> -i auto` sur son DI de calibration, le log donne les deux) :

  | modèle | pondérée K | RMS large | écart |
  |---|---|---|---|
  | Big Fuzz (pedal, très saturé 71 %) | +4,4 dB | +5,6 dB | **-1,2** |
  | Agressive Scooped Mids (pedal) | +1,8 | +2,4 | -0,6 |
  | Clean Comp Preamp (pedal) | +1,1 | +1,4 | -0,3 |
  | Ampeg SVT (amp_cab) | +1,0 | +0,9 | **+0,1** |
  | MESA 800D FAT CLASSIC ROCK | +6,3 | +6,8 | -0,5 |
  | Fender Bassman 800 | +17,9 | +18,4 | -0,5 |

  Donc **1,3 dB** de correction relative entre le fuzz et l'ampli : réel, dans le bon sens, et
  **insuffisant** à lui seul — ce que l'utilisateur entend est une différence de **timbre**
  (+18 dB au-dessus de 500 Hz à RMS égal) qu'aucun nombre unique ne résume. Le réglage qui tranche
  est le **volume par preset**.
- **L'alignement sur le DI annule tout gain de la chaîne.** En `Level::Match`, `build_inner` fait
  passer l'enregistrement de calibration dans la chaîne **entière déjà construite** et pousse le
  gain inverse **après** tous les blocs : il compense exactement ce qu'on y ajoute. Mesuré
  (`render -m mrhr --level match --post gain:db=$g`, sinus 110 Hz) : bloc à -40 dB -> alignement
  **+40,9 dB** ; à 0 -> +0,9 ; à +24 -> **-23,1** ; sortie identique aux trois (crête -11,6 dBFS,
  RMS -16,4). La compensation est bornée à ±60 dB. Un gain **avant** un modèle n'est pas annulé en
  tant que **drive** (la saturation change), seulement en niveau. Conséquence : seul `master_db`
  (après l'alignement) change le volume, et `ChainMeters::level_db` publie la valeur pour que
  `browse` la montre.
- Les IR sont normalisées sur leur gain moyen 80 Hz–5 kHz (normaliser l'énergie totale saturait de
  +15 dB dans le grave).

### Queue de preset

Deux chaînes tournent pendant la queue, donc **×2,1 par bloc** mesuré (`cargo test --release -p dsp
--test trails -- --nocapture`, modèle d'exemple WaveNet, bloc 64) : 45 -> 90 µs pour b7k, 104 ->
208 µs pour le modèle le plus lourd. Durée observée (plafond 2 s) :

| chaîne quittée | queue |
|---|---|
| `nam` seul (rien qui sonne) | **23 ms** (le fondu seul) |
| `nam › reverb` (défauts) | 1 120 ms, ce qu'elle annonce |
| `nam › delay 400 ms, feedback 70 %` | 2 001 ms, le plafond |
| n'importe laquelle, `--no-trails` | 23 ms |

Le fondu porte sur l'**entrée** des deux chaînes : c'est ce qui permet à la chaîne quittée de
continuer à sonner. Le pire cas n'est pas la non-linéarité du modèle mais la **corrélation** — deux
chaînes qui sortent le même signal s'additionnent, et un fondu à puissance constante les gonfle de
**+3,54 dB** sur 20 ms. Aucun creux, et un `gate` dans la chaîne quittée ne se referme pas pendant
le fondu.

Le seuil d'audibilité est **-60 dBFS** et non -80 : mesuré (`render`, b7k + IR, silence en entrée),
un modèle NAM alimenté en silence sort un plancher autour de **-62 dBFS** (bruit du réseau, remonté
par la normalisation), si bien qu'un seuil plus bas ne se déclencherait jamais.

**La queue annoncée couvre toujours la décroissance réelle.** Vérifié par la mesure (`render`,
impulsion puis 12 s de silence, dernier échantillon au-dessus de -60 dBFS) :

| réglage | annoncé | réel |
|---|---|---|
| `reverb` (défauts : decay 1,5 s, mix 15 %) | 1 098 ms | 352 ms |
| `reverb:mix=100,decay=5` | 5 010 | 1 832 |
| `delay` (défauts : 400 ms, fb 35 %, mix 30 %) | 2 593 | 1 600 |
| `delay:feedback=90,mix=100` | 26 295 | 11 602 |
| `chorus:depth=10,delay=30` | 40 | 33 |

Toujours majorant, d'un facteur 1 à 3 : c'est voulu, couper une queue s'entend, la garder trop
longtemps ne coûte que du calcul. Deux règles en sortent : **le `mix` compte** (une réverbe à 15 %
part 16,5 dB plus bas, donc atteint le plancher bien plus tôt), et **les filtres étalent une
transitoire** — une répétition unique de 400 ms ressort à 410 ms à `lowcut = 20 Hz`, d'où la marge
de 20 ms (`FILTER_SPREAD_MS`) sur le delay et le chorus, sans laquelle le delay à réinjection nulle
est 4 à 10 ms trop court.

### Effets et structures

- **Crossover** : la somme LR4 reste plate parce que chaque bande traverse aussi le **passe-tout**
  (`Shape::AllPass`, Q = 1/√2) de chaque coupure au-dessus d'elle — `LR4 passe-bas + LR4 passe-haut`
  à la même fréquence **est** ce passe-tout (vérifié dans `biquad.rs`, écart < 1e-5 sur du bruit).
  Écart mesuré sur la somme, branches vides, 20 Hz–20 kHz plus une grille fine autour de chaque
  coupure : **0,000 dB** à 2, 3 et 4 bandes ; ±0,1 dB avec des branches réelles. Une branche
  retardée de 2 ms creuse de plus de 3 dB autour de la coupure sans compensation, moins de 1 dB
  avec ; retard mesuré b7k + IR dg212 : 43–44 échantillons (0,9 ms, l'attaque de l'IR).
  `--crossover 150` (b7k + IR) rend des WAV identiques au bit près (blocs 48 / 64 / 1000).
- **Delay synchronisé au tempo** (ADR 0008) : `division` vaut `libre` par défaut, donc les rendus de
  référence sont inchangés au bit près. L'étage lit le tempo **une fois par bloc** (`follow_tempo`)
  et n'appelle `set_time` que si le nombre d'échantillons **change**, sinon le fondu de 50 ms
  redémarrerait sans cesse ; en `libre`, le tempo n'est même pas lu. Le tempo est relu **aussi à
  `reset()`** et sans fondu : une chaîne préchargée depuis dix minutes doit repartir au tempo du
  moment (même raison que `gain::LiveGain`). Revenir à `libre` **retrouve** `time` tel qu'il était
  réglé. Mesuré (`render`, impulsion, `--no-limiter`) : les échos tombent à ±0,1 ms de la division
  (à 120 BPM, `1/4` -> 500,0 / 1000,0 / 1500,0 ms ; `1/8.` -> 375,0 ; à 90 BPM, `1/8T` -> 222,2).
  Une division plus longue que le tampon (2 s) est **écrêtée** — `1/1` à 60 BPM donne 2000,0 ms —,
  et le tampon n'est **pas** agrandi : `time` resterait borné à 2000 ms et, à 90 % de réinjection,
  `tail_seconds` dépasserait `MAX_TEST_TAIL_SECONDS`. `tail_seconds` part donc du **plafond** dès
  qu'une division agit, le tempo étant inconnu hors temps réel : 32,7 s aux défauts.
- **Delay** : tampon de 2 s alloué à la construction (2^17 floats, 512 Kio), réinjection ≤ 90 %
  (filtres de boucle Butterworth de gain ≤ 1, donc gain de boucle < 1 : stable) ; queue à -140 dB à
  90 % / 400 ms : 62 s. **Chorus** : sous `lowcut` / 4, écart < 0,2 dB (passe-haut LR4 sur la voix
  modulée et sur le son direct qu'elle remplace).
- **Réverbe** : FDN 8 lignes (30–61 ms, premiers), Householder, passe-bas à un pôle par ligne (gain
  1 au continu : le RT60 du grave ne bouge pas), passe-haut LR4 en sortie. RT60 (Schroeder, -5 à
  -35 dB) à ±15 % de `decay` de 0,2 à 10 s ; une octave sous `lowcut`, < -12 dB par rapport au
  médium. Avec flush-to-zero (`render`), la queue qui s'éteint coûte autant que du bruit (31 s,
  0,023 s CPU dans les deux cas). Niveau : ≈ celui de l'entrée à 0,3 s, plus fort (< +10 dB) pour
  une queue de 8 s. **À mesurer sur Pi 5.**
- **Comp** : détecteur de crête à relâche fixe de 10 ms, puis réduction lissée en dB (attaque /
  relâche) ; simulé à -6 dBFS, ratio 4, seuil -20 : 10,4 dB de réduction à 1 kHz pour 10,5 visés,
  9,8 dB à 41 Hz (l'enveloppe redescend entre deux crêtes d'un mi grave).
- **Mesurer un filtre** au sinus se fait avec le rapport des RMS, pas la crête : à 16 kHz, avec
  3 échantillons par période, la crête échantillonnée sous-estime l'amplitude de ~0,8 dB.

## Dette connue

- `ChainSpec` n'a pas de format TOML **et n'en aura pas** : c'est `preset/mod.rs` qui porte la forme
  enregistrée (`PresetSpec`, § 9 de `docs/design/browse-chaine.md`), avec des chemins **relatifs** à
  la bibliothèque et l'identifiant court en filet ; `ChainSpec` reste la forme du temps réel
  (chemins absolus, réglages résolus). `PresetSpec::to_blocks` fait le pont.
- `meter.rs` refait les deux fonctions atomiques d'`engine::meters` (`fetch_max` / `take`) :
  `dsp` ne peut pas dépendre d'`engine`. À unifier dans le sens `engine` -> `dsp` un jour.
- Les vumètres de l'entrée et de la sortie de la chaîne ne sont pas dans `ChainMeters` :
  `engine::meters()` les publie déjà.
- `Level::Normalize` normalise **chaque** bloc NAM à la cible (règle uniforme, identique au bit près
  au cas d'un seul modèle) : à revoir à l'écoute si plusieurs NAM en série deviennent courants.
- `AmpSpec` reste l'entrée des options CLI : à retirer quand les presets décriront les blocs.
- `comp` n'a pas de détecteur RMS (crête seulement) : à ajouter si le besoin se confirme à l'écoute.
- Limiteur sans lookahead (attaque instantanée) : colore les transitoires qui dépassent -1 dBFS ;
  un lookahead de ~1 ms serait plus propre mais ajoute de la latence.
- `Unit::format` refait le formatage de nombre de `namfoop::browse::state::label::num` (deux
  lignes) : `dsp` ne peut pas dépendre du binaire. À unifier dans le sens `namfoop` -> `dsp`.
- Deux paramètres sont **vivants** (le volume du preset et le tempo, ADR 0007 et 0008) ; les
  paramètres d'effet, eux, reconstruisent la chaîne à chaque appui — la promesse non tenue de
  l'ADR 0005 (`set_param` avec lissage), et la raison pour laquelle on croit qu'un réglage ne fait
  rien. `Delay::set_time` n'est appelé que par la synchronisation au tempo.
