# ADR 0008 · Tempo : trois sources, un arbitre, une case

**Statut : adopté** (décision de l'utilisateur, 15/11/2026, épique #188). **Applique l'[ADR 0007](0007-parametres-vivants.md)**
à une valeur qui a **trois** écrivains candidats, sans le contredire : la règle du seul écrivain
tient, c'est l'ordre de priorité entre les sources qui manquait.

**Contexte.** Les effets de temps n'ont que des temps **absolus** : `delay.time` en millisecondes,
`chorus.rate` en Hz, `reverb.predelay` en ms. Sur scène, ce qu'on veut c'est « une croche pointée
**au tempo du morceau** », pas « 344 ms ». Rien n'existait dans le dépôt : aucune notion de tempo,
de BPM, de tap ni de division rythmique — `dsp::fx::Unit` n'avait que `Db, Ms, Hz, Ratio, Percent,
Q, Seconds, DbOct`.

L'ADR 0007 avait déjà prévu le tempo (« d'autres réglages vivants viendront : tempo, marche / arrêt
d'un bloc, CC MIDI vers un paramètre ») et avait déjà tranché l'essentiel : « un seul écrivain par
case, et c'est l'état de l'écran. Un CC MIDI, un **tap tempo** ou un footswitch entrent dans l'état
**comme une touche** ». Ce qui restait ouvert n'est donc pas *qui écrit la case*, mais **qui gagne**
quand le preset, le tap et l'horloge MIDI veulent tous les trois donner le tempo.

## Décision

### Trois sources, un arbitre, un ordre de priorité

Les trois sources sont des **entrées d'un arbitre** (`engine::tempo`, machine à états pure, horloge
fournie, aucun audio) ; c'est l'arbitre, via la boucle de `browse`, qui écrit la case. Ordre de
priorité, de la plus forte à la plus faible :

| source | quand elle gagne | quand elle lâche |
|---|---|---|
| **tap** | dès la 2ᵉ frappe | changement de preset, ou `Start` / `Stop` MIDI |
| **horloge MIDI** | verrou acquis (24 tops cohérents) | 500 ms sans top → **garde le dernier tempo** et le signale |
| **preset** | au chargement, si aucune source plus forte n'est active | — |

**Un tap détrône l'horloge**, et on ne revient à l'horloge que sur un événement **net** (changement
de preset, `Start` / `Stop`), jamais par expiration silencieuse. C'est le modèle de la Strymon
TimeLine, dont le `MIDI Clock Reset` « lets delay time re-sync to an external MIDI clock tempo after
you tap in a new tempo ». Ce n'est **pas** celui de Helix ni du Quad Cortex, qui *verrouillent* le
tap dès qu'une horloge arrive (« you cannot tap in an alternate BPM … the BPM will be locked to the
external MIDI Clock signal ») : inacceptable ici, puisque l'utilisateur tape justement pour
rattraper une désynchro du groupe ou une erreur de programmation des lumières.

**Fiabilité live avant tout** : une horloge qui meurt en plein morceau ne provoque **jamais** de
saut de tempo. On garde la dernière valeur connue et l'écran dit « horloge perdue ». Un retour
silencieux à la valeur du preset serait un changement de son audible causé par une panne.

### Un tempo par preset, et le tap le modifie

Le tempo est un champ du preset (`tempo_bpm`, dans `dsp::preset::PresetSpec`, déclaré **avant**
`blocks` : en TOML une valeur ne s'écrit pas après un tableau de tables). Un tap **écrit le tempo
courant et marque le preset « non enregistré »**, exactement comme un réglage d'effet ou le volume
(#110) : `S` l'enregistre, une bascule de preset restaure celui du preset suivant, `q` demande
confirmation. **Aucune règle nouvelle à apprendre** — c'est la sémantique de tout le reste de
`browse`, et c'est le `Tap Mode = PRESET` de Strymon.

Le mode « tempo libre / global » (le tempo tapé survit aux bascules de preset, comme le *Global* de
Helix et du Quad Cortex) est **écarté pour l'instant** : un réglage de plus à comprendre et à
montrer, pour un besoin que l'usage n'a pas encore confirmé. À rouvrir si les jams le réclament.

### La queue de preset garde le tempo de **son** preset

**Précision apportée à l'[ADR 0007](0007-parametres-vivants.md)** (#225, entendu à l'essai). Celui-ci
dit qu'une case est partagée par toutes les chaînes audibles, **queue de preset comprise**, et qu'une
réverbe qui finit de sonner « suit le réglage ». C'est juste pour un **niveau** : le volume suit, et
il n'y a pas de saut pendant le fondu. C'est **faux pour un rythme** : la queue appartient au preset
qu'on vient de quitter et ses répétitions ont été posées à *son* tempo. Sans correctif, un `delay`
en `1/8` à 60 BPM voit ses répétitions passer de 500 à 250 ms en pleine décroissance quand le preset
suivant impose 120.

Une chaîne qui entre dans la queue **cesse donc de suivre le tempo** (`Stage::enter_trail`) et garde
sa tête de lecture où elle est. C'est aussi ce que fait le marché : sur une pédale à trails, les
répétitions en cours gardent leur temps.

Deux détails qui comptent :
- on gèle la **tête de lecture**, et non le BPM relu dans la case. Au moment où la chaîne sort, la
  case contient **déjà** le tempo du preset suivant (l'écran l'écrit avant que la nouvelle chaîne
  ne soit installée), donc le relire reviendrait à adopter justement ce dont on veut se protéger ;
- le gel a lieu au **début du fondu**, pas à sa fin : c'est le plus tôt que la chaîne sortante
  puisse l'apprendre. Il reste donc une fenêtre de quelques millisecondes — entre l'écriture de la
  case et l'installation de la nouvelle chaîne — pendant laquelle la chaîne sortante a pu commencer
  à déplacer sa tête. Le fondu de 50 ms de `Delay::set_time` la rend inaudible, et la supprimer
  demanderait de lier l'écriture de la case à l'installation de la chaîne, donc de retarder le
  tempo de la chaîne **entrante** : pas un bon échange.

Le **volume**, lui, ne gèle pas : le raisonnement de l'ADR 0007 tient pour lui.

### Gelé ou vivant : le patron de `Volume`

`dsp::block::ChainSpec` reçoit un `tempo: Tempo`, calqué sur `Volume` (#179, #187) :
- `Tempo::Bpm(f32)` **gelé** : `run`, `render`, `bench`, les rendus de référence — reproductible ;
- `Tempo::Live(param::Control)` : `browse` — **une** case, partagée par toutes les chaînes audibles,
  queue de preset comprise. Un tap pendant un fondu agit donc sur les deux chaînes, ce qu'on veut.

Un étage qui lit la case s'en protège (valeur non finie ignorée, bornes `30` à `300` BPM) et ne
recalcule rien quand les bits lus sont ceux du bloc précédent — la règle de l'ADR 0007.

### Les divisions rythmiques : un paramètre dont la première valeur est « libre »

Un paramètre `division` par effet synchronisable, **qui prend le pas sur `time`** :

| indice | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| | libre | 1/16 | 1/8T | 1/8 | 1/8. | **1/4** | 1/4. | 1/2 | 1/1 |
| × noire | — | 0,25 | 0,333 | 0,5 | 0,75 | **1** | 1,5 | 2 | 4 |

`division = libre` → `time` s'édite en millisecondes comme avant. `division = 1/8.` → `time` passe
en **grisé, entre parenthèses, avec sa valeur calculée** (`(375 ms)`) et n'est plus modifiable ;
revenir à `libre` **retrouve** la valeur en ms intacte (le scalaire reste stocké, il est seulement
sans effet).

C'est la forme de l'Axe-Fx, mot pour mot : « to enter a delay time in milliseconds, first set Tempo
to None », et sinon « the time will be in parentheses like (350ms) and you can't adjust it
manually ». Les deux autres formes du marché ont été examinées et écartées :

| | Comment le temps s'exprime | |
|---|---|---|
| **Fractal Axe-Fx** | `Tempo` (None, 1/4, 1/8.…) + `Time` dérivé entre parenthèses | **retenu** |
| **Helix** | **un** `Time` à deux domaines : on appuie sur le potard pour basculer ms ↔ note | écarté |
| **Quad Cortex** | un interrupteur `tempo sync` par delay + une « sync note » | écarté |
| **Strymon TimeLine** | `Tap Division` **par preset**, 5 valeurs (CC 21) : pas un réglage de l'effet, mais « ce que vaut une frappe » | écarté |

La forme Helix demanderait un paramètre dont **l'unité dépend d'un autre paramètre** — du neuf dans
`ParamInfo`, qui décrit aujourd'hui un scalaire et son unité (ADR 0005) — et deux valeurs stockées
derrière un même nom, alors que le panneau de `browse` est une liste plate de scalaires réglés à
`←` / `→`, sans geste « appuyer sur le potard ». Le `Tap Division` de Strymon est une rustine de
pédale à un seul potard (les forums montrent les utilisateurs s'y perdre) : avec un écran, une
division **par effet** est strictement meilleure, et c'est le choix de Fractal, de Line 6 et de
Neural DSP.

**Neuf valeurs, dans l'ordre des noms de notes.** Les musiciens naviguent par **nom de note**, pas
par durée, et le marché en offre peu : Strymon en vend **cinq** (1/4, 1/8., 1/8, triolet, 1/16) et
cela suffit à des milliers de gens. L'ordre retenu est celui des noms de notes, qui se trouve être
aussi strictement croissant en durée — donc `←` / `→` vont toujours du plus court au plus long, sans
surprise. `1/32`, `1/16.`, `1/4T`, `1/2T` et `1/2.` sont écartés : rares à la basse, et chaque
valeur de plus est un cran de plus à traverser sur scène. En ajouter un jour ne coûte qu'une ligne
de table.

Une division plus longue que le tampon du `delay` (2 s, `MAX_SECONDS`) est **écrêtée et le dit**
(`(2000 ms, plafond)`). Le tampon n'est **pas** agrandi : `time` resterait borné à 2000 ms, et à
90 % de réinjection `tail_seconds` dépasserait `MAX_TEST_TAIL_SECONDS` (90 s).

Le `delay` d'abord. Chaque autre effet où la synchronisation a du sens (`chorus.rate`,
`reverb.predelay`) a son issue et sa PR : toutes les vitesses ne se synchronisent pas bien, Line 6
le dit lui-même (« not all Speed parameters can be synced to note values, as they may be non-linear
and highly interactive »).

### Le tap

Deux frappes donnent déjà un tempo (réponse immédiate) ; à partir de la 3ᵉ, moyenne des **4 derniers
intervalles**. Le Helix moyenne jusqu'à dix frappes et demande « at least 10 taps to get the best
accuracy » : trop lent à réagir quand on rattrape une désynchro au milieu d'un morceau. Le Quad
Cortex, à l'inverse, se fonde sur « the interval between your last two clicks » : trop nerveux.
Un intervalle hors de ±40 % de la moyenne courante **repart** une séquence (hésitation, fausse
frappe) ; 3 s sans frappe la termine. Bornes **30 à 300 BPM**, résolution 0,1 BPM. Un tap **recale
le temps 1** du métronome : on tape sur le premier temps.

Touche dans `browse` : **`*`** (à droite de `ù` sur AZERTY FR, sans Maj). Aucune touche existante ne
bouge : les dix-neuf lettres mnémoniques sont prises, `t` reste le TOML replié. Comme toute touche
commune, elle passe **après** les couches de saisie de texte (#175).

### L'horloge MIDI (mise en œuvre confiée à #7)

Les deux, comme tout le marché : **horloge** Beat Clock (`0xF8`, 24 tops par noire) avec
`Start` / `Stop` / `Continue`, **et** un CC de tap (le Quad Cortex utilise CC#44, Helix CC#64).

Un top isolé est inexploitable : à 120 BPM un top vaut 20,833 ms, et 21 ms donne déjà 119 BPM.
Donc : moyenne glissante sur 24 tops, rejet des intervalles hors de ±25 %, **publication seulement
si l'écart dépasse 0,3 BPM** (sinon le temps du delay vibrerait en permanence), perte de verrou
après 500 ms sans top, `Start` recale le temps 1, `Stop` garde le dernier tempo. L'horloge se coupe
par la config : une horloge bruyante en jam est un risque, pas un service.

### Le métronome audible

Jamais audible par défaut, et **jamais** sur une sortie qui ne le demande pas explicitement :
`model::routing::OutputRoute` gagne un `click: bool`, et le clic est **ajouté** par
`engine::router::Router` aux sorties qui le portent. Ce n'est pas une `Source` : l'in-ear veut la
basse **et** le clic. Réglages dans `[metronome]` de la config — `level_db` (défaut -18),
`beats_per_bar` (défaut 4, accent sur le premier temps) —, pas dans le preset : c'est une aide
d'écoute, pas un son. Le Quad Cortex, qui a le métronome le plus complet du marché, sépare de la
même façon `Volume`, `Pan`, `Sound` et surtout **`Routing`** : « assigns the metronome playback to
specific device outputs ».

Son : salve de 25 ms, 1 kHz (2 kHz sur l'accent), enveloppe en cosinus surélevé — pas de clic
parasite, aucune allocation. La phase **ne se remet pas à zéro** quand le tempo change (seul
l'incrément change) : c'est ce qui rend le clic naturel à l'oreille. Elle se recale sur un tap ou un
`Start` MIDI.

### La queue de preset

Un delay synchronisé n'a plus de temps connu hors temps réel, donc la **queue de preset** (#172) ne
peut plus être devinée : elle attend aujourd'hui 500 ms de silence sous -60 dBFS, plafond 2 s, et à
60 BPM une croche pointée vaut 1125 ms — la fenêtre couperait une répétition qui n'est pas encore
arrivée. **#173 (`Stage::tail_ms`) devient un prérequis** de cette feature : la chaîne doit **dire**
la durée de sa queue au lieu qu'on la devine. Jusque-là, la limite est connue et assumée.

Un changement de tempo, lui, passe par le fondu de 50 ms déjà écrit dans `Delay::set_time` : **pas
de glissement de hauteur** — ce que Strymon appelle `MIDI Clock Sweep` éteint. Le nombre
d'échantillons n'est recalculé qu'une fois par bloc, et `set_time` n'est appelé que s'il **change**,
sinon le fondu redémarrerait sans cesse.

## Conséquences

- `dsp::fx::Unit` gagne `Division`, et `Effect::build` reçoit un contexte (`BuildCtx`) au lieu de
  deux arguments : c'est ce qui fait voyager la case de tempo jusqu'à l'étage, et ce qui rendra
  gratuit le prochain paramètre vivant (#6).
- `division` vaut **0 (libre) par défaut** : les presets existants se relisent, et les rendus de
  référence sont inchangés **au bit près**.
- Le tempo ne fait **pas** partie de l'identité d'une demande de chaîne (règle de l'ADR 0007) :
  deux presets qui ne diffèrent que par leur tempo sont la même demande, et une place de banque
  préchargée correspond quel que soit le tempo.
- `engine` accueille une machine à états **sans audio** (l'arbitre). C'est là qu'atterrira MIDI
  (#7), et `namfoop` en dépend déjà ; `dsp` ne garde que ce qui est du calcul (la table des
  divisions, la conversion BPM → millisecondes).
- Un paramètre peut désormais être **sans effet à cause d'un autre** (`time` quand `division` agit).
  L'écran doit le montrer, comme il montre déjà un `gain` annulé par l'alignement sur le DI (#178).

**Ce que cet ADR ne couvre pas** : le mode « tempo libre » (écarté ci-dessus), les signatures
rythmiques par preset, la synchronisation des modulations entre blocs et l'automation — que
l'[ADR 0006](0006-chaine-en-blocs-et-parallele.md) refuse toujours.
