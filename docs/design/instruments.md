# Spécification · mes instruments et mes cartes

**Ce que couvre ce fichier** : jouer avec n'importe lequel de ses instruments sur n'importe laquelle
de ses cartes son, calibré — la liste des instruments et celle des cartes, ce qui se passe au
lancement, la question posée quand une calibration manque, la calibration elle-même, ce que l'écran
affiche, les gestes au pied (changer d'instrument, changer de morceau), la commande
`namfoop instruments` et les fichiers. Les conventions MIDI sont dans
l'[ADR 0011](../adr/0011-midi-conventions-fractal.md), l'écran de réglage du MIDI dans
[`midi.md`](midi.md), la vue scène dans [`set.md`](set.md) § 6 et § 9, l'accordeur dans
[`accordeur.md`](accordeur.md), les couleurs dans la [charte](../identite/charte.md).

**État** : spécification d'un parcours **à construire**. Rien ici n'est vérifié par un test : c'est
la cible que suivent les stories de l'épique « Changer d'instrument ou de carte son et jouer
calibré, sans rien re-régler ». À mesure qu'elles sont livrées, le code fait foi, et ce paragraphe
se retire. Deux choix écrits ici se confirment avec leur story : le nom de l'option de reprise
(§ 11) et la fenêtre de 5 secondes (§ 8).

La maquette [`instruments.html`](instruments.html) est une **illustration actionnable** : à ouvrir
dans un navigateur, elle déroule le parcours de l'arrivée sur le lieu au changement d'instrument en
plein set, avec l'écran de scène, un pédalier à quatre boutons, la grille des calibrations et ce
que la salle entend. Le set, les sons, les niveaux et les textes qu'elle montre sont fictifs, les
durées sont raccourcies, et son JavaScript n'est pas à reprendre.

**Couleurs** : la préparation, la question et la calibration ne sont pas un espace de `browse` ;
elles sont **neutres**, comme `setup` et `run` ([charte](../identite/charte.md), « Dans le
terminal »). Le **jaune ambre** dit ce qui demande attention (une calibration qui manque, une
confirmation attendue), le **vert signal** ce qui est en règle (« calibrée »), le rouge clip un
refus (saturation pendant la mesure).

## 1. Le besoin et les règles

Plusieurs instruments (des basses passives, une active, une guitare), plusieurs cartes (celle de
scène, celle qui reste à la maison, une carte de secours), plusieurs ordinateurs qui reçoivent la
même bibliothèque. Le niveau envoyé aux modèles d'ampli doit rester juste dans tous les cas : c'est
ce qui garde l'esprit d'une capture.

1. **Une calibration vaut pour un instrument sur une carte.** Le trim compense à la fois le niveau
   de sortie de l'instrument et le gain d'entrée de la carte.
2. **Une calibration se fait une fois** par instrument et par carte, en 15 secondes. Aucune n'est
   déduite d'une autre.
3. **Le gain d'entrée d'une carte a un repère fixe**, réglé une fois avec l'instrument le plus
   fort, puis laissé en place : c'est ce qui fait tenir les calibrations.
4. **Jouer calibré, ou le savoir.** Quand une calibration manque, namfoop pose la question avant de
   jouer, même sur scène. Jouer sans est permis, retenu, et rappelé à l'écran.
5. **Un lancement prêt ne demande rien.** Si l'instrument est calibré sur la carte branchée, aucun
   écran ne s'interpose : le Pi de scène démarre seul.
6. **Un geste raté ne coupe jamais le son.** Rien de ce qui se déclenche au pied n'agit sur le son
   sans confirmation, sauf un changement de morceau.
7. **Changer d'instrument relance la session.** Le son se coupe, l'écran l'annonce comme un
   chargement, puis le même morceau et le même son reviennent. Il n'y a pas de changement à chaud.
8. **namfoop ne joue jamais sur une carte que l'utilisateur n'a pas réglée.**

## 2. Deux listes, et où elles se rangent

| | Mes instruments | Mes cartes |
|---|---|---|
| Ce que c'est | un nom, un numéro, et une calibration par carte où il a été mesuré | le device, le buffer, les entrées, le rôle de chaque sortie |
| Qui l'écrit | `namfoop calibrate --instrument`, la question du lancement, le geste au pied | `namfoop setup`, une fois par carte |
| Où | **dans la bibliothèque** : `library/instruments/` | **sur l'ordinateur** : `profiles/`, à côté de la config |
| Pourquoi là | la bibliothèque se copie d'un ordinateur à l'autre, et une calibration reste vraie tant que l'instrument et la carte sont les mêmes | le buffer qui tient sans xrun et le cœur réservé dépendent de la machine |

- **L'instrument par défaut** est celui que namfoop prend à chaque lancement sans qu'on le nomme, y
  compris quand il redémarre après une coupure de courant. Il se range avec les instruments.
- **La carte préférée** est celle que namfoop prend quand plusieurs cartes réglées sont branchées.
  Elle se range dans la config de l'ordinateur.
- **Le numéro** d'un instrument fixe sa ligne dans la grille (§ 10) et son geste au pied (§ 8).
- **La guitare** est un instrument comme les autres : même entrée de la carte, mêmes sons.
- **Deux cartes du même modèle** portent le même nom pour le système : elles partagent leur profil
  et leurs calibrations.

## 3. Le parcours

| Moment | Ce que je fais | Ce que fait namfoop |
|---|---|---|
| Une fois par carte | `namfoop setup`, puis je règle le gain d'entrée avec mon instrument le plus fort et je marque le potard | range les réglages de cette carte sans toucher à ceux des autres |
| Une fois par instrument et par carte | je joue fort 15 s | mesure, range la calibration avec l'instrument |
| Avant un concert | `namfoop instruments` | montre la grille : ce qui est calibré, sur quelle carte |
| À chaque lancement | rien | prend la carte branchée, l'instrument par défaut, vérifie la calibration, joue |
| En balance | je joue fort quelques notes | le vumètre d'entrée montre ma crête et le repère de la calibration |
| Je prends un autre instrument | au clavier : je relance avec `--instrument` ; au pied : le geste de l'instrument, deux fois | relance la session avec sa calibration |
| Sur un autre ordinateur | `namfoop import` de la valise de ma clé, puis `namfoop setup` pour la carte | retrouve mes instruments et leurs calibrations |

## 4. Au lancement : la préparation

Toute commande qui joue (`browse`, `run`, `run --set`) passe d'abord par la même préparation, dans
cet ordre, avant d'ouvrir le moindre écran :

1. **La carte** (§ 12) : `--card`, sinon la préférée si elle est branchée, sinon l'unique autre
   carte réglée qui est branchée.
2. **L'instrument** : `--instrument`, sinon l'instrument par défaut. `--instrument` accepte le
   numéro, le nom, ou quelques lettres, comme `-m` et `-i` désignent un son. Un nom qui ne désigne
   aucun instrument, ou qui en désigne plusieurs, est une erreur qui les liste.
3. **La calibration** de cet instrument sur cette carte : présente, jamais faite, ou « jouée
   sans ».
4. **La question** (§ 5) si elle n'a jamais été faite, sinon l'écran s'ouvre.

Ce que namfoop écrit en démarrant, une ligne par décision :

```
carte       PreSonus AudioBox iTwo
instrument  1 BB 735A · calibrée sur iTwo (trim -17,5 dB)
```

```
carte       Behringer UMC404HD — l'iTwo n'est pas branchée
instrument  2 Fretless · jouée sans calibration sur UMC404HD
```

```
carte       PreSonus AudioBox iTwo — aussi branchée : MTK (--card mtk)
```

- **Le niveau** : un instrument dont la calibration est mesurée joue en volume « match », sur
  tout ordinateur, quel que soit le `level` de la config ; `--level` passe avant. La ligne de
  l'instrument le dit (`… · niveau match`).
- **`browse` comme `run --set`** : la préparation est la même. Le set choisi avec `L` dans `browse`
  reste le set du moment, celui que `run --set` rouvre ([`set.md`](set.md) § 9).
- **En carte simulée**, la préparation ne pose aucune question.
- **Hors carte** (`render`, `models`, la carte simulée) : la calibration qui vaut est celle de
  l'instrument sur la carte préférée, sinon sa plus récente. Avec `--no-config`, aucune.
- **Sans aucun instrument dans la bibliothèque**, la calibration de la config vaut seule, et la
  préparation ne pose aucune question.

## 5. La question

Elle occupe tout l'écran, en couleurs neutres, et **rien ne sort** tant qu'elle attend.

```
 namfoop · avant de jouer

   2 Fretless n'est pas calibrée sur iTwo.

   Entrée, ou le geste de cet instrument au pied    calibrer maintenant : je joue fort 15 s
   Échap, ou un bouton de son au pied               jouer sans calibrer

   Rien ne sort tant que je n'ai pas répondu.
```

| Réponse | Au clavier | Au pied | Suite |
|---|---|---|---|
| calibrer | `Entrée` | le CC de l'instrument, avec son numéro | la calibration (§ 6), puis l'écran |
| jouer sans | `Échap` | le CC du son | l'écran, avec « non calibrée » dans l'en-tête |

- **« Jouer sans » est retenu** pour cet instrument sur cette carte : les lancements suivants ne
  reposent pas la question, et un redémarrage après une coupure de courant rejoue aussitôt. La
  première calibration efface ce choix.
- **`q` renonce au lancement** : rien n'est retenu, la question reviendra.
- **Jouer une note ne répond pas.** Une note jouée pour vérifier le son lancerait une mesure
  fausse : seul un geste répond.
- **Le geste d'un autre instrument ne répond pas** : `Réponds d'abord pour 2 Fretless`.
- **Sans clavier ni pédalier**, personne ne peut répondre : namfoop joue sans calibration et l'écrit
  dans ses lignes de démarrage, au lieu d'attendre.
- **Dans `run --set`**, la question est posée aussi : la scène verrouillée interdit d'éditer, pas de
  calibrer.
- **Si la réponse ne peut pas s'enregistrer** (bibliothèque protégée en écriture), namfoop le dit
  et la garde pour la session.

## 6. Calibrer

La mesure est celle de `namfoop calibrate` : l'instrument joué fort, sa crête robuste, le trim qui
l'amène à la cible à l'entrée des modèles, et l'enregistrement qui sert de référence au niveau
« match » et au grain des modèles.

```
 namfoop · calibration · 2 Fretless sur iTwo

   Joue FORT, comme sur tes passages les plus appuyés.

   reste 11 s    ██████████████████░░░░░░░░░░░░    crête -13,4 dB

   Tu t'entends en direct.
```

```
 namfoop · calibration · 2 Fretless sur iTwo

   Calibrée.

   crête robuste -14,0 dBFS    trim -16,0 dB
   enregistrée avec mes instruments, dans la bibliothèque
```

- **Pendant les 15 secondes**, l'instrument sort **en direct, sans effet**, sur les sorties qui
  portent d'habitude le son traité. Sur scène, la façade l'entend : la calibration y est un geste
  d'installation.
- **Une mesure refusée** — entrée saturée, signal trop faible — dit quoi corriger (baisser le gain
  de la carte, jouer) et repose la question.
- **`namfoop calibrate --instrument <nom>`** fait la même mesure hors de tout lancement : c'est la
  façon de créer un instrument, et de refaire une calibration. Sans `--instrument`, il calibre
  l'instrument par défaut. `--card` choisit la carte comme ailleurs.
- **Le premier lancement avec un instrument** mesure le grain des modèles avec son enregistrement,
  une fois ; les mesures des autres instruments restent en cache.

## 7. À l'écran

L'en-tête de `browse` et de la scène porte, à droite, l'instrument joué, la carte et l'état :

```
 namfoop run  🔒 SET  Groupe 1 · Live Mairie                    ♩ 124 4/4    1 BB 735A · iTwo · calibrée
```

| État | Texte | Couleur |
|---|---|---|
| calibré sur cette carte | `1 BB 735A · iTwo · calibrée` | aplat vert, texte noir d'encre |
| joué sans calibration | `2 Fretless · iTwo · non calibrée` | aplat jaune, texte noir d'encre |
| carte simulée | `1 BB 735A · carte simulée` | gris |

- **Le vumètre d'entrée** garde son repère : la crête attendue quand on joue fort, d'après la
  calibration de l'instrument joué. En balance, une crête qui touche le repère dit que le gain de
  la carte est à sa place. Sans calibration, il n'y a pas de repère, et la ligne le dit.
- **Pendant une relance**, l'écran dit ce qui se passe et pourquoi le son est coupé :

```
   2 Fretless · chargement…

   Le son est coupé le temps de recharger les sons avec la calibration de cet instrument.
```

## 8. Changer d'instrument au pied

Un **bouton par instrument** : le CC de l'instrument (§ 13), dont la valeur donne le numéro.

| Geste | Ce qui se passe |
|---|---|
| le geste d'un autre instrument | `Passer à 2 Fretless ? Refais le geste (5 s)`. **Le son ne change pas.** |
| le même geste dans les 5 secondes | la session se relance avec cet instrument : son coupé, chargement annoncé, même set, même morceau, même son |
| aucun geste pendant 5 secondes | `Geste oublié : rien n'a changé` |
| tout autre geste pendant l'attente | il agit normalement, et le changement est abandonné |
| le geste de l'instrument joué | `Recalibrer 1 BB 735A sur iTwo ? Refais le geste (5 s)` |
| le même geste dans les 5 secondes | la calibration (§ 6), puis la relance |
| un numéro qui ne désigne aucun instrument | `CC 102 = 6 : aucun instrument 7`, rien ne change |

- **Après la relance**, si l'instrument n'est pas calibré sur la carte, c'est la question du § 5
  qui s'affiche, et elle se répond au pied.
- **Dans `browse`** comme dans `run --set`. Dans `browse`, une chaîne ou un set non enregistrés
  retiennent la relance, qui rouvre la session depuis les fichiers : le message le dit (`S`
  l'enregistre, puis refais le geste) plutôt que d'ouvrir une fenêtre qu'il faudrait fermer au
  clavier.
- **Pendant l'accordage**, le geste est accepté ; la relance rend le son, comme tout démarrage
  ([`accordeur.md`](accordeur.md) § 1, règle 6).
- **Pendant une saisie de texte**, le geste est ignoré, comme tout message MIDI.
- **Au clavier**, il n'y a pas de touche pour changer d'instrument dans `browse` : on quitte et on
  relance avec `--instrument`.

## 9. Changer de morceau au pied

Deux messages, pour un pédalier qui n'a pas un bouton par morceau : **morceau précédent** et
**morceau suivant** (§ 13). Ils font ce que font `←` et `→` : le morceau voisin du set, sur son
premier son, sans bouclage en fin de set ([`set.md`](set.md) § 5).

- **Une valeur de 64 ou plus agit, une valeur inférieure est ignorée** : un bouton qui envoie 127 à
  l'appui et 0 au relâcher ne change de morceau qu'une fois.
- Ils agissent **sans confirmation**, dans l'espace Set de `browse` et dans `run --set`, et
  **rendent le son** si la basse était coupée pour l'accordage ([`accordeur.md`](accordeur.md) § 3).
- **Sans set ouvert**, ils ne font rien, et l'écran le dit : `CC 104 : aucun set ouvert`.

## 10. `namfoop instruments` : la grille

```
$ namfoop instruments
                      iTwo     MTK      UMC404HD
 1 BB 735A (défaut)    ✓        ✓        ✓
 2 Fretless            ✓        —        sans
 3 Active              ✓        —        —
 4 Guitare             —        ✓        —

 ✓ calibrée   — jamais calibrée   sans : jouée sans calibration
 carte préférée : iTwo
```

Une ligne par instrument, dans l'ordre de leurs numéros ; une colonne par carte réglée sur cet
ordinateur, plus une par carte qui n'y est pas réglée mais porte une calibration. Sans carte, la
commande marche : elle ne lit que des fichiers.

## 11. Créer, reprendre, choisir

| Je veux | Je tape |
|---|---|
| créer un instrument, ou refaire sa calibration | `namfoop calibrate --instrument "bb 735a"` |
| reprendre la calibration que j'ai déjà, sans rejouer | `namfoop instruments --adopt "BB 735A"` |
| jouer un autre instrument que celui par défaut | `namfoop --instrument fretless browse` |
| changer l'instrument par défaut | `namfoop instruments --default fretless` |
| jouer sur une autre carte réglée | `namfoop --card mtk browse` |

- **Un instrument créé** reçoit le premier numéro libre ; le premier créé est l'instrument par
  défaut.
- **`--adopt`** range la calibration de la config (trim, cible, enregistrement) comme calibration
  de ce nouvel instrument sur la carte préférée. La config n'est pas modifiée.

## 12. Les cartes

- **`namfoop setup` règle une carte** : tout ce qui tient à elle — device, buffer, périodes, entrée
  de l'instrument, entrée du micro, rôle de chaque sortie, sorties qui reçoivent le clic — va dans
  son profil. Régler une seconde carte ne touche pas au profil de la première.
- **Une carte est « réglée »** quand `setup` a écrit son profil sur cet ordinateur. Un profil livré
  avec namfoop ne suffit pas : il ne dit pas où vont les sorties.
- **La carte préférée** est la première réglée. Quand une autre l'est déjà, `setup` demande
  `En faire ma carte préférée ? (o / N)`.
- **Le choix**, dans l'ordre : `--card <id>` ; la préférée si elle est branchée ; l'unique autre
  carte réglée qui est branchée, annoncée par une ligne (§ 4) ; sinon un refus qui liste les cartes
  vues, les cartes réglées, et renvoie vers `setup`.
- **Plusieurs cartes réglées branchées sans la préférée** : un refus, qui les nomme et montre
  `--card`.
- **Une config qui porte elle-même une carte et un routage** se relit : ses réglages valent pour
  la carte qu'elle nomme, qui est alors la préférée.
- **Le port MIDI** reste un réglage de l'ordinateur, pas d'une carte.

## 13. MIDI

| Geste | Message par défaut | Valeur | Réglage |
|---|---|---|---|
| l'instrument | **CC 102** | n choisit l'instrument de numéro n+1 | `cc_instrument` |
| le morceau précédent | **CC 103** | 64 ou plus agit, moins de 64 est ignoré | `cc_prev_song` |
| le morceau suivant | **CC 104** | 64 ou plus agit, moins de 64 est ignoré | `cc_next_song` |

- Aucun de ces trois gestes n'a de numéro convenu sur le marché : les numéros sont ceux de namfoop,
  dans une plage que la norme MIDI laisse libre (ADR 0011, amendement).
- Comme les autres gestes, ils se règlent par canal dans `namfoop midi`, se coupent un par un, et
  figurent dans la fiche pour le groupe ([`midi.md`](midi.md) § 4 et § 7). Les gabarits livrés ne
  les portent pas : un gabarit choisi laisse ces trois numéros en place.
- La valeur de l'instrument suit la forme du CC des sons : la valeur 0 désigne le premier.

**Un pédalier à quatre boutons, réglé pour la scène** — l'exemple du guide, pas une dépendance :

| Bouton | Appui court | Appui long |
|---|---|---|
| 1 | son 1 : CC 34 = 0 | instrument 1 : CC 102 = 0 |
| 2 | son 2 : CC 34 = 1 | instrument 2 : CC 102 = 1 |
| 3 | son 3 : CC 34 = 2 | morceau précédent : CC 103 = 127 |
| 4 | accordeur : CC 15 = 127 | morceau suivant : CC 104 = 127 |

L'accordeur se quitte par un bouton de son : le pédalier n'a pas de bascule à gérer.

## 14. Les fichiers

**Un instrument**, dans la bibliothèque — `library/instruments/<id>.toml`, ses enregistrements à
côté :

```toml
name = "BB 735A"
number = 1                    # sa ligne dans la grille, son numéro au pied
default = true                # un seul instrument le porte

[calibration.itwo]            # une table par carte : l'identifiant de son profil
trim_db = -17.5
target_dbfs = -30.0
wav = "bb-735a.itwo.wav"
date = 2026-11-24

[calibration.umc404hd]
skipped = true                # « jouée sans » : la question ne se repose pas
```

- Deux instruments du même numéro, ou deux instruments par défaut, sont une erreur qui nomme les
  deux fichiers.
- Sans `default`, l'instrument du plus petit numéro est celui par défaut.

**Une carte**, sur l'ordinateur — `profiles/<id>.toml`, à côté de la config :

```toml
name = "Soundcraft Signature MTK"
match = "MTK"
device = "hw:MTK"
buffer = 64
periods = 3
bass_in = 1
mic_in = 2

[[outputs]]
channel = 1
source = "processed"

[[outputs]]
channel = 2
source = "di"
```

**La config de l'ordinateur** ne garde de la carte que la préférée, et gagne les trois numéros
MIDI :

```toml
[audio]
profile = "itwo"              # la carte préférée

[midi]
cc_instrument = 102
cc_prev_song = 103
cc_next_song = 104
```

## 15. Les messages

| Cas | Message |
|---|---|
| le geste d'un autre instrument | `Passer à 2 Fretless ? Refais le geste (5 s)` |
| le geste de l'instrument joué | `Recalibrer 1 BB 735A sur iTwo ? Refais le geste (5 s)` |
| l'attente expire | `Geste oublié : rien n'a changé` |
| un autre geste pendant l'attente | `Changement d'instrument abandonné` |
| un numéro sans instrument | `CC 102 = 6 : aucun instrument 7` |
| un instrument joué sans calibration, à l'ouverture de l'écran | `2 Fretless joue sans calibration sur iTwo : son geste, deux fois, la calibre` |
| le geste d'un autre instrument pendant la question | `Réponds d'abord pour 2 Fretless` |
| un message de morceau sans set | `CC 104 : aucun set ouvert` |
| la réponse ne peut pas s'enregistrer | `bibliothèque protégée en écriture : retenu pour cette session seulement` |

`namfoop midi` traduit les trois messages en clair dans sa zone « reçu » : `instrument 2 Fretless`,
`morceau précédent`, `morceau suivant`.

## 16. Ce que la maquette simule

Le parcours en six étapes (arrivée, allumage, balance, concert, autre instrument, incident), le
lancement par le Pi (`run --set`) ou au clavier (`browse`), l'écran de scène, la question, la
calibration, la relance, l'accordeur, un pédalier à quatre boutons avec appui court et appui long,
la grille des calibrations dont chaque case se bascule, deux cartes, ce que la salle entend à
chaque instant, et une coupure de courant.

Elle ne simule pas : `setup`, le choix entre deux cartes branchées ensemble, `namfoop instruments`
en ligne de commande, les instruments au-delà du deuxième au pied, les tailles de terminal, ni la
mesure réelle d'un niveau.

## 17. Tests attendus

Fichiers : un instrument relu à l'identique après écriture, deux cartes dans un même fichier, deux
instruments par défaut refusés. Préparation, en fonctions pures : le choix de la carte (préférée
présente, absente, deux candidates, `--card` inconnu) ; le choix de l'instrument ; la décision de
poser la question selon l'état de la calibration, la présence d'un clavier, d'un pédalier, et la
carte simulée. La config de l'ordinateur reste identique à l'octet après une session jouée avec un
instrument qui n'est pas celui par défaut. Écran, état pur : un geste d'instrument seul s'oublie à
l'échéance ; deux gestes sortent avec leur motif ; un autre geste annule ; morceau suivant et
précédent aux bornes du set, valeur sous 64 ignorée, son rendu pendant l'accordage. De bout en
bout, sur la carte simulée avec un port MIDI simulé : aucune question ; une relance qui rouvre le
même set, le même morceau et le même son. Sur le matériel : deux instruments calibrés sur la carte
de scène, le passage de l'un à l'autre au pied, et la durée du silence.

## 18. Les décisions de forme, et leur raison

- **Deux listes plutôt qu'un profil par situation.** Un profil « guitare sur la MTK » obligerait à
  refaire `setup` dans chaque profil, et leur nombre est le produit des instruments par les cartes.
  Deux listes ne demandent qu'une chose par couple : 15 secondes.
- **Pas de calibration déduite.** Déduire une calibration d'une autre carte suppose que les gains
  n'ont pas bougé et que deux entrées chargent un micro passif de la même façon : une estimation
  qui a l'air d'une mesure. La grille montre les trous, et ils se bouchent à la maison.
- **Les instruments dans la bibliothèque.** Elle se copie entière d'un ordinateur à l'autre : la
  calibration d'une basse sur la carte de scène vaut sur chacun. Le précédent existe sur le marché
  — des réglages d'entrée nommés par instrument chez Boss, des préréglages d'entrée par guitare
  chez Kemper —, sans numéro MIDI pour les rappeler.
- **L'instrument par défaut à chaque lancement**, même après une coupure. Repartir sur le dernier
  instrument choisi serait plus juste pendant un incident, et faux le lendemain à la maison sans
  qu'on le voie. Prévisible d'abord : après une coupure, le geste au pied se refait.
- **Demander, même sur scène.** Un niveau faux ne se remarque pas toujours, et c'est lui qui trahit
  une capture. La question n'arrive que s'il manque une calibration, et sa réponse est retenue : un
  concert préparé ne la voit jamais, un redémarrage ne la revoit pas.
- **Un geste pour calibrer, pas une note.** On joue toujours une note pour vérifier qu'on a du son.
- **Relancer plutôt que changer à chaud.** Changer de calibration touche le trim, la référence du
  niveau de chaque chaîne et la banque préchargée. Passer par le démarrage, c'est un seul chemin à
  tester, et un écran qui n'a pas à connaître la calibration. Le silence est acceptable parce qu'on
  est en train de changer d'instrument.
- **Le même geste deux fois, sans menu.** Un bouton par instrument se lit et se règle sans écran ;
  le refaire est la confirmation la plus simple à faire au pied, et elle ne réserve aucun bouton.
- **Le morceau change sans confirmation.** C'est le geste le plus fréquent du concert, et il se
  rattrape d'un appui.
- **En direct partout pendant la mesure.** C'est ce que fait `namfoop calibrate`, et sur scène la
  calibration n'arrive qu'à l'installation.
- **Une carte préférée et un repli annoncé.** La carte de scène revient régulièrement à la maison :
  elle doit gagner sans option. Le repli se limite aux cartes réglées, parce que jouer dans la
  mauvaise sortie est pire qu'un refus.
- **Des numéros à nous.** Les incréments de scène de Fractal portent les numéros 123 et 124, qui
  sont aussi des messages de mode de la norme MIDI ; il n'existe de numéro convenu ni pour le
  morceau suivant, ni pour l'instrument. La plage 102-119 est libre.
