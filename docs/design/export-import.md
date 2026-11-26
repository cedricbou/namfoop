# Spécification · emporter ma bibliothèque d'une machine à l'autre

**Ce que couvre ce fichier** : faire passer ma bibliothèque — mes sons, mes créations, mes
instruments — entre mes ordinateurs avec une clé USB — ce qui voyage et ce qui reste sur chaque machine, la valise
posée sur la clé, les commandes `namfoop export` et `namfoop import`, le bilan affiché avant
d'écrire, le choix quand un élément a changé des deux côtés, les suppressions, le retour en arrière,
les refus et les fichiers. Les instruments et les cartes sont décrits dans
[`instruments.md`](instruments.md), l'import des packs de sons dans le guide, § 6, les couleurs dans
la [charte](../identite/charte.md).

**État** : `namfoop export` est livré, et son code fait foi (§ 3, § 5, § 12) ; `import`, le choix
quand un élément a changé des deux côtés et le retour en arrière (§ 6 à § 8, § 10) sont la cible
des stories qui suivent dans l'épique « Emporter ma bibliothèque d'une machine à l'autre avec une
clé USB ». Trois choix écrits ici se confirment avec leur story : le mot « valise », le nom de
l'option `--undo` (§ 8), et la touche unique de confirmation du bilan (§ 6).

**Couleurs** : `export` et `import` ne sont pas un espace de `browse` ; ils sont **neutres**, comme
`setup` et `run`. Le **vert signal** dit ce qui arrive ou ce qui est en règle (`+`, « complète »),
le **jaune ambre** ce qui demande attention (`~`, « à trancher », une confirmation attendue), le
**rouge clip** un refus et une suppression (`-`), le gris ce qui ne bouge pas (`=`).

## 1. Le besoin et les règles

Trois machines jouent la même bibliothèque : l'ordinateur de studio, où je crée ; le portable de
répétition, où je retouche ; le Pi de scène. Un changement fait sur l'une doit se retrouver sur les
deux autres, dans tous les sens, sans réseau : en répétition, la clé USB est le seul lien.

1. **Un échange emporte toute la bibliothèque.** Sons, créations, instruments : rien ne se choisit,
   donc rien ne s'oublie. Un modèle ajouté au studio est sur le portable après l'échange, sans y penser.
2. **Seul ce qui a changé est recopié.** La valise est complète, la copie est courte.
3. **Rien ne s'écrit avant le bilan.** `import` montre ce qui arrive, ce qui reste et ce qui est à
   trancher, puis attend une touche.
4. **Un élément changé des deux côtés ne se règle jamais seul.** Je garde celui d'ici, je prends
   celui de la valise, ou j'abandonne l'import ; abandonner n'écrit rien.
5. **« Changé » se décide sur le contenu, pas sur les dates.** L'horloge d'un Pi sans réseau est
   fausse ; les dates s'affichent, elles ne décident de rien.
6. **Une suppression suit**, annoncée dans le bilan.
7. **Un import se défait** : l'état d'avant est gardé jusqu'à l'import suivant.
8. **On importe avant d'exporter.** `export` refuse d'écrire sur une valise qui porte des
   changements que la machine n'a pas vus.
9. **La config de l'ordinateur ne voyage pas.** La carte, le MIDI et les réglages de jeu se règlent
   une fois par machine ; `export` et `import` ne les lisent ni ne les écrivent.

## 2. Ce qui voyage, ce qui reste

| | Voyage dans la valise | Reste sur la machine |
|---|---|---|
| Sons | modèles, IR, notices des packs | les mesures en cache, refaites au premier lancement |
| Créations | presets, sets et leurs prompteurs, mélanges d'IR, corrections de modèles | |
| Instruments | chaque instrument, son numéro, celui par défaut, chaque calibration et son enregistrement | |
| Carte | | tout ce que `setup` règle, et la carte préférée |
| MIDI | | le port, les numéros des gestes, les canaux, les gestes coupés |
| Jeu | | mode de niveau et sa cible, limiteur, IR préférée générale, accordeur, métronome, touches du set, set du moment |
| Machine | | son nom, l'emplacement de la bibliothèque |

La frontière est celle des dossiers : **la bibliothèque voyage, la config de l'ordinateur reste.**

- **Un élément** est ce que le bilan compte et ce qu'une question tranche : un modèle, une IR, un
  preset, un set, un prompteur, un mélange, la correction d'un modèle, un instrument, une
  calibration.
- **Une calibration est un élément à part** : celle de la Fretless sur le HAT du Pi et celle de la
  Fretless sur l'iTwo, faites le même soir sur deux machines, arrivent toutes les deux sans
  question.
- **La correction d'un modèle est un élément à part** : corriger deux modèles différents sur deux
  machines ne pose aucune question.

## 3. La valise

Un **dossier**, `valise-namfoop/`, dans le dossier que je désigne : la racine d'une clé USB, ou
n'importe quel dossier. Il contient la bibliothèque telle quelle et une
**étiquette** : qui a écrit la valise en dernier, quand, et l'empreinte de chaque élément.

- **Chaque machine porte un nom** — `studio`, `portable`, `pi`. Le premier `export` ou `import`
  le demande, et propose le nom de l'ordinateur sur le réseau. Il s'affiche partout où une version
  est montrée.
- **Chaque machine retient son dernier échange avec chaque valise** : c'est ce qui dit, élément
  par élément, de quel côté il a changé (§ 7).
- **Une seule valise sert les trois machines.** Deux clés sont deux valises : chacune a son
  identité, et une machine ne les confond pas.
- **Sans argument**, `export` et `import` reprennent le dossier du dernier échange. S'il est
  absent, le message le nomme et demande de brancher la clé ou de donner un dossier.
- **En secours, la valise se joue** : sa bibliothèque est un dossier ordinaire, que `--library`
  accepte.

## 4. Le parcours

| Moment | Ce que je fais | Ce que fait namfoop |
|---|---|---|
| La première fois sur une machine | je donne son nom | le retient |
| Au studio, avant la répétition | `namfoop export <clé>` | met la valise à jour, dit ce qui est parti |
| Sur le portable, avant de jouer | `namfoop import <clé>` | montre le bilan, attend une touche, écrit |
| En répétition, j'ai retouché un son | `namfoop export` | met la valise à jour |
| De retour au studio | `namfoop import` | montre ce qui revient, me fait trancher s'il le faut |
| Avant un concert, sur le Pi, au clavier | `namfoop import` | même bilan, même question |
| J'ai calibré un instrument sur le Pi | `namfoop export`, puis `import` sur les deux autres | la calibration arrive partout |
| Une machine neuve | `namfoop import <clé>`, puis sa carte | installe tout (§ 10) |

Sur le Pi, l'échange se fait **au clavier**, namfoop arrêté, avec les deux mêmes commandes.

## 5. `namfoop export`

```
$ namfoop export /run/media/cedric/NAMFOOP
valise      /run/media/cedric/NAMFOOP/valise-namfoop · vue ici le lun. 23/11
machine     studio

 sons         + 2 modèles  + 1 IR          Darkglass pack · 41 Mo
 créations    ~ 2 presets  + 1 set         Chill verse, Fuzz refrain · Live Mairie
 instruments  = 4 instruments, 6 calibrations

 44 Mo copiés en 6 s. La valise est complète : 312 sons, 16 presets, 3 sets.
 Tu peux retirer la clé.
```

- **Une ligne par famille**, dans l'ordre du § 2 : ce qui part (`+` nouveau, `~` modifié,
  `-` supprimé), ou `=` et le compte quand rien ne bouge. Les noms suivent, jusqu'à trois, puis
  `… et 5 autres`.
- **« Tu peux retirer la clé »** ne s'affiche qu'une fois tout écrit pour de bon : retirer la clé
  à ce moment ne perd rien.
- **Rien à emporter** : `La valise est déjà à jour avec studio.`, et rien n'est écrit.
- **`--dry-run`** montre le même bilan sans écrire : c'est la façon de savoir, avant de partir, ce
  qui n'est pas encore dans la valise.
- **La première fois** dans un dossier, `export` crée la valise et le dit :
  `valise      nouvelle, dans /run/media/cedric/NAMFOOP`.
- **Une valise en avance** est un refus (règle 8) :

```
$ namfoop export
valise      clé NAMFOOP · de portable, jeu. 26/11 23:40

 La valise porte 3 changements que studio n'a pas :
   ~ preset       Chill verse
   + preset       Répet 26
   ~ calibration  2 Fretless sur iTwo

 Importe d'abord : namfoop import
 Rien n'est écrit sur la clé.
```

- **`--overwrite`** passe outre : la valise devient exactement cette machine, et ce qu'elle portait
  d'autre est perdu. Le bilan le montre et demande `Écraser ces 3 changements ? (o / N)`.
- **Un export interrompu** — clé retirée, clé pleine — laisse une valise marquée incomplète :
  `import` la refuse, et relancer `export` la répare.

## 6. `namfoop import`

`import` reconnaît seul ce qu'on lui donne : un dossier qui contient une valise est un échange,
tout le reste est un pack de sons, comme dans le guide.

```
$ namfoop import /run/media/cedric/NAMFOOP
valise      de studio, mer. 25/11 22:10
machine     portable · dernier échange lun. 23/11

 arrive       + 2 modèles  + 1 IR  + 1 set  ~ 1 preset  - 1 preset
                + pack     Darkglass pack (2 modèles, 1 IR)
                + set      Live Mairie
                ~ preset   Fuzz refrain
                - preset   Vieux test
 reste ici    ~ 1 preset                    Slap couplet
 à trancher   1 preset                      Chill verse
 vérifié      16 presets et 3 sets trouvent leurs sons

 Entrée : continuer    a : abandonner, rien n'est écrit
```

| Ligne | Ce qu'elle dit |
|---|---|
| `arrive` | changé dans la valise seulement : sera écrit ici |
| `reste ici` | changé ici seulement : n'est pas touché, partira au prochain `export` |
| `à trancher` | changé des deux côtés : une question par élément (§ 7) |
| `vérifié` | après l'import, chaque preset et chaque set trouvera ses sons ; sinon la ligne les nomme, en jaune |

- **Le détail de `arrive`** liste chaque création par son nom ; les sons se regroupent par pack.
- **Une seule touche** : `Entrée` continue — vers les questions s'il y en a, sinon vers
  l'écriture —, `a` abandonne. `--dry-run` s'arrête après le bilan.
- **Rien à importer** : `portable est déjà à jour avec la valise.`
- **À la fin**, ce qui a changé et ce qu'il reste à faire :

```
 Importé : 2 modèles, 1 IR, 1 set, 1 preset. 1 preset supprimé.
 L'état d'avant est gardé : namfoop import --undo

 2 changements d'ici ne sont pas dans la valise : Slap couplet, Chill verse.
 Les y mettre maintenant ? (O / n)
```

- **« Les y mettre maintenant ? »** fait l'`export` dans la foulée : la clé repart complète, et la
  machine suivante n'a qu'à importer.
- **`--overwrite`** fait de la machine la copie exacte de la valise : ce qui a changé ici est
  remplacé sans question, après le bilan et un `Remplacer ces 2 changements d'ici ? (o / N)`.
  L'état d'avant est gardé comme pour tout import.
- **Si namfoop joue sur cette machine**, `import` le dit et s'arrête : la banque en mémoire ne
  suivrait pas.

## 7. Ce qui a changé des deux côtés

Pour chaque élément, `import` compare trois états : celui d'ici, celui de la valise, et celui du
dernier échange entre cette machine et cette valise.

| Ici | Dans la valise | Ce que fait `import` |
|---|---|---|
| comme au dernier échange | changé | il arrive |
| changé | comme au dernier échange | il reste ici |
| changé | changé, autrement | **à trancher** |
| changé | changé, pareil | rien : les deux côtés sont d'accord |
| supprimé | changé | **à trancher** |
| changé | supprimé | **à trancher** |

Sans échange retenu — une première fois —, un élément présent d'un seul côté arrive ou reste, et
un élément différent des deux côtés est à trancher.

```
 à trancher 1/2 · preset  Chill verse

   ici       portable   mar. 24/11 20:15   comp › b7k › dg212 › reverb
   valise    studio     mer. 25/11 21:40   comp › b7k › dg212
   diffère   reverb présente ici seulement · volume +2 dB dans la valise

   i  garder celui d'ici            I  celui d'ici pour tous les suivants
   v  prendre celui de la valise    V  celui de la valise pour tous les suivants
   a  abandonner l'import : rien n'est écrit
```

- **Chaque version** porte la machine qui l'a changée en dernier, la date qu'elle a notée, et le
  résumé de l'élément : la chaîne d'un preset, les morceaux d'un set, le trim d'une calibration.
- **`diffère`** nomme jusqu'à trois écarts, puis `… et 4 autres`.
- **Un élément supprimé d'un côté** s'écrit `supprimé sur studio` à la place du résumé ; garder
  ce côté, c'est supprimer.
- **Garder celui d'ici** en fait un changement d'ici : il part au prochain `export` et remplace
  celui de la valise.
- **Toutes les questions sont posées avant la première écriture** : `a` à la dernière question
  abandonne tout, comme à la première.
- **Sans clavier pour répondre** — une commande lancée par un script —, un élément à trancher
  abandonne l'import et le dit.

## 8. Les suppressions et le retour en arrière

- **Une suppression arrive comme un changement** : un preset supprimé au studio figure dans
  `arrive` avec un `-`, et disparaît d'ici à l'import.
- **Un son supprimé que cite encore un preset d'ici** fait passer la ligne `vérifié` en jaune :
  `Slap couplet cite un modèle qui part : Darkglass B7K v2`. L'import reste permis ; le set
  s'ouvre et le reste joue, comme pour tout son introuvable.
- **`namfoop import --undo`** remet la machine dans l'état d'avant le dernier import : ce qui
  est arrivé repart, ce que l'import a remplacé ou supprimé revient.

```
$ namfoop import --undo
 Import du mer. 25/11 22:31 (valise de studio) défait :
   2 modèles, 1 IR, 1 set retirés · 1 preset remis · 1 preset revenu

 portable est comme avant cet import.
```

- **Un seul niveau** : l'état gardé est celui d'avant le dernier import, et le suivant le
  remplace. Défaire deux fois de suite est un refus qui le dit.
- **Défaire ne touche pas la valise.**

## 9. Ce qui se règle sur chaque machine

- **La carte** : `namfoop setup`, une fois par carte et par machine
  ([`instruments.md`](instruments.md) § 12). Une carte que j'emmène sur une autre machine s'y
  règle une fois.
- **Les calibrations voyagent, et retrouvent leur carte par son identifiant** : il est le même sur
  toutes les machines pour une même carte, parce qu'il vient du nom que le système lui donne.
- **Une calibration arrivée pour une carte qui n'est pas réglée ici** est gardée telle quelle : elle
  apparaît dans la grille de `namfoop instruments`, et sert dès que la carte est réglée.
- **Le mode de niveau est un réglage de la machine** : « match » s'y active quand
  `namfoop calibrate` y tourne. Une calibration arrivée par la valise ne l'active pas ; sur cette
  machine, relancer `namfoop calibrate` ou écrire `level = "match"` dans sa config donne le même
  volume qu'ailleurs.
- **Le MIDI** : `namfoop midi`, une fois par machine. `namfoop midi --fiche` donne les numéros à
  reporter.
- **Le limiteur, l'accordeur, le métronome** : leurs valeurs par défaut sont les mêmes partout ;
  un réglage changé sur une machine se reporte à la main sur les autres.
- **La config de l'ordinateur reste identique à l'octet** après un `export` comme après un
  `import`.

## 10. Une machine vide

Sur une machine sans bibliothèque, `import` installe la bibliothèque entière, instruments
compris, à l'emplacement par défaut.

```
$ namfoop import /run/media/cedric/NAMFOOP
 Comment s'appelle cet ordinateur ? [raspberrypi] pi

valise      de studio, mer. 25/11 22:10
machine     pi · premier échange

 arrive       + 312 sons  + 16 presets  + 3 sets  + 4 instruments
 bibliothèque créée dans ~/.local/share/namfoop/library

 Entrée : continuer    a : abandonner, rien n'est écrit
```

Il reste ce qui se règle sur chaque machine (§ 9) : la carte, le MIDI, et les instruments à
calibrer sur cette carte — la grille de `namfoop instruments` montre les trous.

## 11. Les refus et les messages

| Cas | Message |
|---|---|
| le dossier ne contient ni valise ni son | `aucune valise dans /run/media/cedric/CLE : namfoop export la crée` |
| le dossier du dernier échange est absent | `clé NAMFOOP absente : branche-la, ou donne un dossier` |
| la valise porte des changements non vus | `La valise porte 3 changements que studio n'a pas` (§ 5) |
| la valise est incomplète | `valise incomplète (export de studio interrompu) : relance namfoop export sur studio` |
| un fichier de la valise est abîmé | `2 fichiers de la valise sont abîmés :`, leurs noms, `rien n'est écrit` |
| la valise vient d'un namfoop plus récent | `valise écrite par namfoop 0.10 : mets à jour namfoop ici (0.9)` |
| la clé est pleine | `il manque 120 Mo sur la clé : rien n'a changé dans la valise` |
| la clé est en lecture seule | `clé protégée en écriture : rien n'est écrit` |
| namfoop joue sur cette machine | `namfoop joue sur cette machine : quitte-le, puis relance l'import` |
| un élément à trancher sans clavier | `1 élément à trancher et personne pour répondre : import abandonné` |
| rien à défaire | `aucun import à défaire` |

Un refus n'écrit jamais rien, ni ici ni dans la valise.

## 12. Les fichiers

**La valise**, dans le dossier désigné :

```
valise-namfoop/
  valise.toml          l'étiquette
  library/             la bibliothèque, telle quelle, sans son cache
```

**L'étiquette**, `<clé>/valise-namfoop/valise.toml` :

```toml
format = 1
id = "v7k2m"                  # l'identité de la valise
namfoop = "0.9.0"

[export]
machine = "studio"
date = 2026-11-25T22:10:00+01:00
complete = true               # écrit en dernier ; false : la valise ne s'importe pas

[[element]]
kind = "preset"
name = "Chill verse"
path = "library/presets/chill-verse.preset.toml"
hash = "9c41d0a2b7e3f518"     # l'empreinte du contenu
machine = "studio"            # qui l'a changé en dernier
date = 2026-11-25T21:40:00+01:00
```

**Sur l'ordinateur**, à côté de la config, dans `echanges/` : le nom de la machine, et le dernier
échange avec chaque valise (`echanges/<id de la valise>.toml`, les empreintes de chaque élément à
ce moment-là). L'état d'avant le dernier import est gardé dans la bibliothèque, à côté de son
cache, et ne voyage pas. La config elle-même n'est ni lue ni écrite.

## 13. Hors périmètre

- **L'échange au pied sur le Pi**, sans clavier.
- **L'échange par le réseau**, de machine à machine.
- **Fusionner deux versions d'un même preset** : on garde l'une ou l'autre.
- **Écouter les deux versions** avant de trancher.
- **Mettre à jour le programme** d'une machine depuis une autre.

## 14. Tests attendus

Comparaison, en fonctions pures : chaque ligne de la table du § 7, avec et sans échange retenu ; une
calibration et une correction de modèle traitées à part de leur fichier. Fichiers : une étiquette
relue à l'identique après écriture ; une valise incomplète refusée ; une valise d'un format plus
récent refusée. De bout en bout, avec deux bibliothèques jetables et un dossier pour valise :
un aller-retour qui laisse les deux bibliothèques identiques ; un changement des deux côtés qui
pose sa question, et `a` qui laisse les deux côtés identiques à l'octet ; une suppression qui
suit ; un import défait qui rend la bibliothèque et la config identiques à l'octet ; un `export`
refusé sur une valise en avance ; une machine vide installée. La config de l'ordinateur reste
identique à l'octet après un export et après un import. Sur le matériel : la clé du
concert, formatée comme elle l'est, entre le studio, le portable et le Pi ; un nom de preset
accentué ; la durée d'un premier export complet et celle d'un export du quotidien.

## 15. Les décisions de forme, et leur raison

- **Tout emporter plutôt que choisir.** Un échange où l'on coche ce qui part est un échange où
  l'on oublie un modèle, et l'oubli se découvre en répétition. La copie de ce qui a changé seul
  rend « tout » aussi court que « un peu ».
- **Un dossier plutôt qu'une archive.** Une archive se réécrit en entier à chaque export ; un
  dossier ne reçoit que ce qui a changé, se lit sans namfoop, et se joue en secours.
- **Une valise au milieu, pas des machines qui se parlent.** Chaque machine ne connaît que la
  valise : trois machines, trois relations, et aucune n'a besoin que les deux autres soient
  allumées.
- **Le contenu plutôt que la date.** Une machine sans réseau ni pile d'horloge se croit à une
  autre date ; « le plus récent gagne » y remplacerait le bon preset par le mauvais sans rien
  dire.
- **Retenir le dernier échange.** Sans lui, toute différence serait une question, et une
  suppression ne se distinguerait pas d'un ajout de l'autre côté.
- **L'un ou l'autre, jamais un mélange.** Fusionner deux chaînes d'effets donne un troisième son
  que personne n'a écouté.
- **Abandonner n'écrit rien.** Les questions passent toutes avant la première écriture : un
  import à moitié fait est un état que personne ne sait décrire.
- **Importer avant d'exporter.** Écrire par-dessus une valise en avance perdrait le travail d'une
  répétition ; le refus coûte une commande, et `--overwrite` reste pour le jour où une machine
  fait foi.
- **Le même verbe que pour les packs.** `import` fait entrer des sons dans la bibliothèque dans
  les deux cas ; ce qu'on lui donne suffit à savoir lequel.
- **La bibliothèque voyage, la config reste.** La frontière est un dossier : elle se retient sans
  liste, et `export` comme `import` n'ont rien à savoir de la carte ni du MIDI. Le prix est un
  `setup` et un `namfoop midi` par machine, une fois.
- **Une carte reste avec sa machine.** Son buffer tient au processeur, le rôle de ses sorties à la
  façon dont elle est câblée là où elle joue.
- **Une calibration à part de son instrument.** Les calibrations se font carte par carte, donc
  machine par machine : rangées en un seul élément, elles poseraient une question à chaque
  retour de concert.
