# dsp — étages audio mono

**Responsabilité.** Les traitements, sans carte son ni config : étages mono (trait `Stage`),
chaîne, changement de chaîne à chaud, analyses hors ligne.

**Fichiers / API.** Le trait `Stage` (dans `lib.rs`) a **`enter_trail()`** (défaut : rien, #225) :
appelé une fois, en RT, quand la chaîne passe dans la **queue de preset**, au **début** du fondu.
L'étage cesse alors de suivre les réglages de **rythme** et garde sa tête de lecture. La case de
tempo étant unique et partagée (ADR 0007), sans ça un delay en `1/8` à 60 BPM voit ses répétitions
s'accélérer à 250 ms quand le preset suivant impose 120. On gèle la **tête**, pas le BPM relu : à cet
instant la case contient déjà le tempo du suivant. Un **niveau**, lui, suit (le volume ne gèle pas).
`reset()` lève le gel, puisque c'est la remise à zéro d'une chaîne reprise par `engine::bank`.
Le trait a aussi **`tail_ms()`** (défaut 0, #173) : combien
de temps l'étage peut encore sonner une fois son entrée silencieuse, à -60 dBFS près. `Chain` les
**somme** (une réverbe derrière un delay sonne après la dernière répétition), `Split` prend le
**maximum** de ses branches audibles, et c'est ce que la queue de preset garde en vie. Chaque étage
à queue le déclare : `Delay` depuis son état **vivant** (le temps dépend du tempo, #195) et son
`mix`, `Reverb` depuis son RT60 + predelay pondérés par son `mix`, `IrStage` depuis sa longueur,
`Chorus` depuis son retard modulé ; un gain, un EQ, un gate, un NAM n'annoncent rien.
Puis : `nam.rs`, `ir.rs` (fft-convolver), `gain.rs` (`Gain` figé, et `LiveGain` qui suit
une case `param::Control` : lecture **une fois par bloc**, rampe de `RAMP_MS` = 20 ms comptées en
**échantillons**, qui traverse les frontières de bloc — jamais une rampe étalée sur le buffer, qui
ferait dépendre le son de la taille de bloc ; case lue à la construction ET à `reset()`, sinon une
chaîne préchargée sauterait de 36 dB à l'installation), `tempo.rs` (**LE TEMPO ET LES DIVISIONS**, ADR 0008 : `DIVISIONS`, la table des **neuf**
divisions — indice **0 = `libre`**, qui rend la main au temps en millisecondes —, dans l'ordre des
noms de notes, qui est aussi croissant en durée (`←` / `→` prévisibles) ; chaque entrée porte son
`name` (`noire`) et son `beats_label` (`un temps`) parce qu'une fraction nue se lit mal — `1/4` est
une noire, donc UN temps, et non un quart de temps, ce qui a vraiment trompé l'utilisateur (#207) ;
`MAX_INDEX` est une
**const** parce que les tables `PARAMS` en sont une, et un test vérifie qu'elle suit la table ;
`at` / `label_at` / `index_of` (l'étiquette en mini-syntaxe : `delay:division=1/8.`),
`Division::ms_at`, `clamp_bpm` (30 à 300, non fini -> 120) ; et `Tempo::{Bpm, Live}`, calqué sur
`chain::Volume` : figé pour `run` / `render` / `bench` / les rendus de référence, **vivant** pour
`browse`. Qui *décide* du tempo (preset, tap, horloge MIDI) est l'affaire de l'arbitre d'`engine`),
`param.rs` (**LA CASE DE RÉGLAGE**, ADR 0007 :
miroir de `meter.rs` dans l'autre sens — le **contrôle possède la valeur**, le temps réel la lit ; un
seul écrivain par case ; elle ne décrit rien, la description reste `fx::ParamInfo`), `limiter.rs`,
`meter.rs`
(vumètres d'affichage : `Meter` (f32 dans un `AtomicU32`, maximum gardé jusqu'à la lecture qui
remet à zéro), `BlockMeters { peak, action }` (`action` = réduction du comp / fermeture du gate),
`BranchMeters { sum, blocks }`, `SplitMeters`, `ChainMeters` (préallouée, `clear()`, et
`level_db` : l'alignement sur le DI mesuré à la construction, que l'écran doit montrer, #178), et
l'étage `Probe` qui ne touche pas au signal), `block.rs`
(**la chaîne en blocs**, ADR 0006 : `ChainSpec { trim_db, blocks, level, volume, master_db,
limiter }` — `volume` est le **volume du preset** (#179), poussé **après** l'alignement de niveau
et avant le master : le seul gain d'un preset que `Level::Match` n'annule pas (#178). Il est soit
`Volume::Db` (figé : `run`, `render`, `bench`, les rendus de référence ; aucun étage posé au neutre),
soit `Volume::Live` (une case, posée par `browse` : l'étage est alors posé **même au neutre**, sinon
la case ne serait lue par personne, #187) —,
`BlockSpec::{Fx, Nam, Ir, Split}`, `SplitSpec`/`BranchSpec` (nom, gain, polarité, muet, blocs) ;
`validate()` refuse un parallèle dans une branche (un seul niveau) ; `build()` charge, aligne les
branches sans modèle sur la première qui en a un, et place le trim ; `build_with_meters()` rend en
plus la table des vumètres (`meter.rs`), seule à poser des sondes ; `describe()` pour le log
`chaîne :` — `◆ bandes 150 Hz [< 150 Hz : direct | > 150 Hz : NAM b7k › IR sm57]`), `chain.rs`
(`Chain`, `Level` raw / normalize / match mesuré sur toute la chaîne, `Volume` figé / vivant,
`LimiterSpec`, et `AmpSpec` :
les options CLI, qui **produisent** une `ChainSpec` via `to_chain_spec()` — `--crossover HZ` = bandes
[HZ], grave = son direct),
`wav.rs`, `blend.rs` (mélanges d'IR .cab.toml : alignement, gain, polarité, calcul au
chargement), `preset/mod.rs` (**presets** `.preset.toml` : la chaîne enregistrée, son `volume_db` (#179, déclaré
**avant** `blocks` : en TOML une valeur ne s'écrit pas après un tableau de tables) — blocs, parallèles,
branches ; chemins **relatifs** à la bibliothèque avec l'identifiant court en filet ; effets écrits
au complet par serde pour que le son soit figé ; `to_blocks()` donne les `BlockSpec` du temps réel), `profile.rs` (réponse d'une IR par tiers d'octave, étendue grave/aiguë à -12 dB,
`from_transfer`, distorsion harmonique, **`loudness_db`** : loudness pondérée K d'ITU-R BS.1770,
dont se sert `Level::Match`, #181), `split.rs` (étage parallèle `Split` : 2 à `MAX_BRANCHES` = 4 branches, mode copie ou bandes
(`SplitMode::Bands`, 1 à 3 coupures croissantes, crossover LR4) ; par branche `SplitBranch` : gain,
polarité, muet (muette = pas traitée) ; réglages vérifiés à la construction (`Result`), jamais en RT ;
retards mesurés par corrélation et compensés, imbricable ; `measure_latency`), `biquad.rs` (biquad RBJ en TDF-II, f64, `response_db`,
formes passe-bas / passe-haut / cloche / plateaux / **passe-tout** ; réutilisable par le crossover et les
effets de temps), `fx/` (effets réglables : `FxSpec`, `catalog()`, voir « Ajouter un effet » ;
`Effect::build` prend un **`BuildCtx`** `{ sample_rate, max_block, tempo }` et non deux arguments :
c'est ce qui fait voyager la case de tempo jusqu'à l'étage, et le prochain paramètre vivant (#6)
passera par là sans toucher aux onze effets. `Unit::Division` est un **scalaire comme les autres**
(min 0, max `tempo::MAX_INDEX`, pas 1, ADR 0005 préservé) : seul son affichage diffère, et
`Unit::format` est le **seul** endroit qui dit comment montrer la valeur d'un paramètre (`-6 dB`,
`1/8.`) — `namfoop fx`, le panneau de `browse` et la mini-syntaxe doivent dire la même chose ; le parallèle **n'est
pas** un effet : c'est `block::SplitSpec`, et `fx::STRUCTURAL_KINDS` le refuse en mini-syntaxe avec
un message qui dit où le décrire), `swap.rs` (changement de chaîne à chaud : `SwapHandle`
envoie une `Chain` construite, `SwapChain` fond en ~20 ms **sur l'entrée** des deux chaînes, et la
chaîne quittée passe dans la **queue de preset** (`trails_ms`, #172) : alimentée en silence, sa
sortie ajoutée à celle de la chaîne en écoute, elle finit de sonner puis repart vers le contrôle —
jamais libérée en RT. Depuis #173, la durée est **demandée** à la chaîne (`Stage::tail_ms`) et non
plus devinée en écoutant le silence : `QUIET_PEAK` / `QUIET_MS` ont disparu ; le contrôle la **libère** (`collect`) ou la **reprend** (`reclaim`) pour la
garder bâtie, ce que fait `engine::bank`). `tests/` : propriétés de chaîne (indépendance au bloc), absence
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

- **Le volume du preset est vivant** (#187, ADR 0007) : il ne rebâtit plus la chaîne. Mesuré en
  pseudo-terminal (x86, profil `ci`, backend sim, bibliothèque jetable, `level = "match"`, de la
  touche à la ligne `NAMFOOP_BROWSE_TRACE`) : **21 ms**, le plancher de la boucle d'affichage de
  `browse` (la même valeur qu'une bascule de preset depuis la banque), et **aucune chaîne
  installée** — contre **143 ms** pour `nam › ir` et **358 à 389 ms** pour un parallèle à deux NAM
  quand le volume faisait partie de l'identité de la demande. Dont ≈ 61 ms de DI de calibration
  repassé dans la chaîne pour un alignement qui ne pouvait pas changer. **Pi 5 non mesuré**
  (#44 / #45).

- **L'alignement sur le DI compare des loudness pondérées K** (#181), et non des RMS larges :
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
  **insuffisant** à lui seul — le problème que l'utilisateur entendait (#181) est une différence de
  **timbre** (+18 dB au-dessus de 500 Hz à RMS égal) qu'aucun nombre unique ne résume. Le réglage
  qui tranche est le **volume par preset** (#179).

  La **pondération A a été mesurée et écartée** : estimée sur les sept zones des courbes en cache,
  l'écart fuzz − ampli vaut +15,4 dB en RMS large, **+17,9 dB** pondéré K, mais **+12,0 dB** pondéré
  A. A ignore tellement le grave qu'elle baisse surtout l'ampli (riche en grave) et alignerait donc
  le fuzz **plus haut** que le RMS large : le mauvais sens. K reste aussi le bon choix physique à
  niveau de scène, où les courbes isosoniques sont plus plates qu'à bas niveau.

  Conséquence sur les rendus de référence : `golden/full.toml` (le seul en `Level::Match`) a été
  régénéré, RMS 0,2099 -> 0,1923.

- **L'alignement sur le DI annule tout gain de la chaîne** (#178, #25). En `Level::Match`,
  `build_inner` fait passer l'enregistrement de calibration dans la chaîne **entière déjà
  construite** et pousse le gain inverse **après** tous les blocs : il compense donc exactement ce
  qu'on y ajoute. Mesuré (`render -m mrhr --level match --post gain:db=$g`, sinus 110 Hz) : bloc à
  -40 dB -> alignement **+40,9 dB** ; à 0 -> +0,9 ; à +24 -> **-23,1** ; sortie identique aux trois
  (crête -11,6 dBFS, RMS -16,4). La compensation est bornée à ±60 dB. Un gain **avant** un modèle
  n'est pas annulé en tant que **drive** (la saturation change), seulement en niveau.
  Conséquences : seul `master_db` (après l'alignement) change le volume ; `ChainMeters::level_db`
  publie la valeur pour que `browse` la montre ; un niveau par preset reste à faire (#179).

- **Queue de preset** (#172, `cargo test --release -p dsp --test trails -- --nocapture`, modèle
  d'exemple WaveNet, bloc 64) : la chaîne quittée finit de sonner, donc **deux chaînes tournent**
  pendant ce temps — mesuré **×2,1 par bloc**, soit, avec les chiffres du tableau plus bas,
  45 -> 90 µs pour b7k (3,4 -> 6,8 % du budget) et 104 -> 208 µs pour le modèle le plus lourd.
  Durée réellement observée de la queue (plafond 2 s), **avant et après #173** (la durée est
  désormais demandée à la chaîne, non devinée en écoutant le silence) :

  | chaîne quittée | avant (silence mesuré) | après (durée annoncée) |
  |---|---|---|
  | `nam` seul (rien qui sonne) | 521 ms (500 de silence exigés + 20 de fondu) | **23 ms** |
  | `nam › reverb` (défauts) | 1 000 ms (coupée) | **1 120 ms** (ce qu'elle annonce) |
  | `nam › delay 400 ms, feedback 70 %` | 2 000 ms | 2 001 ms (le plafond) |
  | n'importe laquelle, `--no-trails` | 23 ms | 23 ms (le fondu de sortie seul) |

  Le fondu porte désormais sur l'**entrée** des deux chaînes (c'est ce qui permet à l'ancienne de
  continuer à sonner) : mesuré sur un sinus, le son direct n'en souffre pas plus qu'avant. Le pire
  cas n'est pas la non-linéarité du modèle mais la **corrélation** — deux chaînes qui sortent le
  même signal s'additionnent, et un fondu à puissance constante les gonfle de **+2,97 dB** avec
  l'ancien schéma, **+3,54 dB** avec le nouveau (+0,6 dB de plus, sur 20 ms). Aucun creux, et un
  `gate` dans la chaîne quittée ne se referme pas pendant le fondu.

  Le seuil de silence est **-60 dBFS** et non -80 : mesuré (`render`, b7k + IR, silence en entrée),
  un modèle NAM alimenté en silence sort un plancher autour de **-62 dBFS** (bruit du réseau,
  remonté par la normalisation), si bien qu'un seuil plus bas ne se déclencherait jamais. La fenêtre
  de silence est de 500 ms parce qu'un **delay** ne sort rien entre deux répétitions : plus court,
  on lui couperait la suivante (exact seulement si `Stage` disait la durée de sa queue, #173).
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
- Plusieurs modèles en parallèle (#111, x86, release, 48 kHz,
  `cargo test --release -p dsp --test chain -- --nocapture cost_of_several_models`, avec
  `NAMFOOP_BENCH_MODEL=<fichier.nam>` pour un modèle de la bibliothèque) : le coût est **linéaire**
  en nombre de modèles (le parallèle lui-même ne coûte que 0,06 à 3 µs, voir plus bas).
  Par bloc de 64 (budget 1333 µs), moyenne / pire cas, chaîne `◆ copie [nam › ir | …]` :

  | modèles | b7k (45 µs/bloc) | le plus lourd de la bibliothèque (104 µs/bloc) |
  |---|---|---|
  | 1 | 45 µs (3,4 %) / 138 µs (10 %) | 104 µs (7,8 %) / 248 µs (19 %) |
  | 2 | 92 µs (6,9 %) / 160 µs (12 %) | 209 µs (16 %) / 360 µs (27 %) |
  | 3 | 139 µs (10 %) / 202 µs (15 %) | 314 µs (24 %) / 601 µs (45 %) |
  | 4 | 184 µs (14 %) / 256 µs (19 %) | 417 µs (31 %) / 509 µs (38 %) |

  Bloc 48 (budget 1000 µs) : même coût absolu, donc une part un peu plus grande (4 lourds : 32 %
  de moyenne, 44 % de pire cas). `namfoop bench` sur les 139 modèles de la bibliothèque, bloc 64 :
  41 µs le plus léger, 42 µs de médiane, 104 µs le plus lourd (3,1 à 7,8 % du budget), max 246 µs.
  Seuil retenu avec l'utilisateur : `browse` **alerte dès 2 modèles actifs** et ne refuse rien
  (le coût est linéaire, et 2 modèles restent confortables sur x86). **Le Pi 5 reste à mesurer
  (#44 / #45)** : lui seul dira s'il faut une limite dure.
- Vumètres par bloc (#107) : une chaîne de 4 blocs avec un parallèle à 3 branches (7 sondes,
  2 mesures d'action, 3 mesures de branche) coûte 4,63 µs par bloc de 64 sans mesures et 4,87 µs
  avec (+0,24 µs, +0,02 % du budget ; x86, release,
  `cargo test --release -p dsp --test meters -- --nocapture cost`). Lire un vumètre juste après le
  démarrage donne le transitoire (gate parti fermé, dépassement d'un filtre) : `clear()` puis
  relire, c'est ce que fait l'affichage à 25 images/s.
- Crossover à plus de deux bandes (#105) : la somme reste plate parce que chaque bande traverse
  aussi le **passe-tout** (`Shape::AllPass`, Q = 1/√2) de chaque coupure au-dessus d'elle —
  `LR4 passe-bas + LR4 passe-haut` à la même fréquence **est** ce passe-tout (vérifié dans
  `biquad.rs`, écart < 1e-5 sur du bruit). Écart mesuré sur la somme, branches vides, 20 Hz–20 kHz
  plus une grille fine autour de chaque coupure : **0,000 dB** à 2, 3 et 4 bandes. Coût par bloc de
  64 (x86, release, `cargo test --release -p dsp --lib split::tests::cost -- --nocapture`) :
  copie 4 branches 0,06 µs, 2 bandes 0,67 µs (identique à #83 : même code), 3 bandes 1,74 µs
  (0,13 % du budget), 4 bandes 3,23 µs (0,24 %) — **à mesurer sur Pi 5 (#44)**. `--crossover 150`
  (b7k + IR) rend des WAV identiques au bit près avant / après (blocs 48 / 64 / 1000).
- Crossover (#83) : somme LR4 plate à ± 0,1 dB de 20 Hz à 20 kHz ; une branche retardée de 2 ms
  creuse de plus de 3 dB autour de la coupure sans compensation, moins de 1 dB avec. Retard mesuré
  b7k + IR dg212 : 43–44 échantillons (0,9 ms, l'attaque de l'IR). Alignement du grave clair sur la
  calibration de l'utilisateur : -14,7 dB, niveau final inchangé (RMS -24,4 dBFS) ; coût `split`
  vide (4 biquads) 0,67 µs par bloc de 64 (0,05 %).
- **La queue annoncée couvre toujours la décroissance réelle** (#173). Vérifié par la mesure
  (`render`, impulsion puis 12 s de silence, dernier échantillon au-dessus de **-60 dBFS**) :

  | réglage | annoncé | réel |
  |---|---|---|
  | `reverb` (défauts : decay 1,5 s, mix 15 %) | 1 098 ms | 352 ms |
  | `reverb:decay=0.5` | 373 | 191 |
  | `reverb:decay=3,predelay=50` | 2 226 | 554 |
  | `reverb:mix=100` | 1 510 | 740 |
  | `reverb:mix=100,decay=5` | 5 010 | 1 832 |
  | `delay` (défauts : 400 ms, fb 35 %, mix 30 %) | 2 593 | 1 600 |
  | `delay:feedback=70,mix=100` | 8 168 | 5 601 |
  | `delay:feedback=90,mix=100` | 26 295 | 11 602 |
  | `delay:time=125,mix=100` | 968 | 625 |
  | `delay:feedback=0,mix=100,lowcut=20` | 420 | 410 |
  | `chorus:depth=10,delay=30` | 40 | 33 |

  Toujours majorant, d'un facteur 1 à 3 : c'est voulu, couper une queue s'entend, la garder trop
  longtemps ne coûte que du calcul. Deux corrections tirées de ces mesures :
  **le `mix` compte** (une réverbe à 15 % part 16,5 dB plus bas, donc atteint le plancher bien plus
  tôt : sans ça elle annonçait 1 510 ms pour 352 réels), et **les filtres étalent une transitoire**
  — une répétition unique de 400 ms ressort à 410 ms à `lowcut = 20 Hz`, d'où une marge mesurée de
  20 ms (`FILTER_SPREAD_MS`) sur le delay et le chorus, sans laquelle le delay à réinjection nulle
  était 4 à 10 ms trop court.
- **Delay synchronisé au tempo** (#195, ADR 0008) : `division` vaut `libre` par défaut, donc les
  rendus de référence sont **inchangés au bit près**. L'étage lit le tempo **une fois par bloc**
  (`follow_tempo`) et n'appelle `set_time` que si le nombre d'échantillons **change** — sinon le
  fondu de 50 ms redémarrerait sans cesse ; en `libre`, le tempo n'est même pas lu. Le tempo est
  relu **aussi à `reset()`** et sans fondu : une chaîne préchargée depuis dix minutes doit repartir
  au tempo du moment (même raison que `gain::LiveGain`). Revenir à `libre` **retrouve** `time` tel
  qu'il était réglé (`time_ms` est gardé). Mesuré (`render`, impulsion, `--no-limiter`) : à 120 BPM
  `1/4` -> échos à **500,0 / 1000,0 / 1500,0 ms**, `1/8.` -> **375,0 / 750,0 / 1125,0**, `1/16` ->
  **125,0** ; à 92 BPM `1/4` -> **652,2** ; à 90 BPM `1/8T` -> **222,2 / 444,5 / 666,7**. Une
  division plus longue que le tampon (2 s) est **écrêtée** : `1/1` à 60 BPM (4 s voulues) donne
  2000,0 ms. Le tampon n'est **pas** agrandi : `time` resterait borné à 2000 ms et, à 90 % de
  réinjection, `tail_seconds` dépasserait `MAX_TEST_TAIL_SECONDS`. `tail_seconds` part donc du
  **plafond** dès qu'une division agit (le tempo est inconnu hors temps réel) : 32,7 s aux défauts.
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

- `ChainSpec` n'a pas de format TOML **et n'en aura pas** : c'est `preset/mod.rs` qui porte la forme
  enregistrée (`PresetSpec`, § 9 de `docs/design/browse-chaine.md`, fixé par #157), avec des
  chemins **relatifs** à la bibliothèque et l'identifiant court en filet ; `ChainSpec` reste la
  forme du temps réel (chemins absolus, réglages résolus). `PresetSpec::to_blocks` fait le pont.
- `meter.rs` refait les deux fonctions atomiques d'`engine::meters` (`fetch_max` / `take`) :
  `dsp` ne peut pas dépendre d'`engine`. À unifier dans le sens `engine` -> `dsp` un jour.
- Les vumètres de l'entrée et de la sortie de la chaîne ne sont pas dans `ChainMeters` :
  `engine::meters()` les publie déjà.
- `Level::Normalize` normalise **chaque** bloc NAM à la cible (règle uniforme, identique au bit près
  au cas d'un seul modèle) : à revoir à l'écoute si plusieurs NAM en série deviennent courants.
- `AmpSpec` reste l'entrée des options CLI : à retirer quand les presets décriront les blocs (#6).
- `comp` n'a pas de détecteur RMS (crête seulement) : à ajouter si le besoin se confirme à l'écoute.

- Limiteur sans lookahead (attaque instantanée) : colore les transitoires qui dépassent -1 dBFS ;
  un lookahead de ~1 ms serait plus propre mais ajoute de la latence.
- `Delay::set_time` est enfin appelé en vrai, mais seulement par la **synchronisation au tempo**
  (#195) : régler `time` à la main reconstruit encore la chaîne, en attendant `set_param` (#6).
- `Unit::format` refait le formatage de nombre de `namfoop::browse::state::label::num` (deux
  lignes) : `dsp` ne peut pas dépendre du binaire. À unifier dans le sens `namfoop` -> `dsp`.
- Deux paramètres sont **vivants** (le volume du preset et le tempo, ADR 0007 et 0008) : les paramètres d'effet, eux,
  reconstruisent encore la chaîne à chaque appui — c'est la promesse non tenue de l'ADR 0005
  (`set_param` avec lissage, #6) et la cause de #177 (« on croit que le réglage ne fait rien »).

## Carte détaillée (reprise du `CLAUDE.md` racine)

```
crates/dsp/       étages audio mono (trait Stage) : nam.rs, ir.rs (fft-convolver), gain.rs, limiter.rs,
                  tempo.rs (tempo et divisions rythmiques : DIVISIONS, Tempo figé / vivant),
                  chain.rs (AmpSpec : trim -> NAM -> IR -> niveau -> master -> limiteur), wav.rs,
                  blend.rs (mélanges d'IR .cab.toml : alignement, gain, polarité, calcul au chargement),
                  preset/mod.rs (presets .preset.toml : la chaîne enregistrée, chemins relatifs + identifiant
                  de secours, tous les paramètres d'effet écrits -> to_blocks pour le temps réel),
                  profile.rs (réponse d'une IR par tiers d'octave, étendue grave/aiguë à -12 dB),
                  swap.rs (changement de chaîne à chaud : SwapHandle envoie une Chain construite,
                  SwapChain fond en ~20 ms sur l'ENTRÉE des deux chaînes, la chaîne quittée finit de
                  sonner dans la QUEUE DE PRESET (trails_ms, #172) puis repart vers le contrôle —
                  jamais libérée en RT)
  tests/          propriétés de chaîne (indépendance au bloc), absence d'allocation (assert_no_alloc),
                  rendus de référence golden.rs + golden/*.toml (RMS, crête, enveloppe 50 ms)
```
