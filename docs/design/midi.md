# Spécification · l'écran MIDI (`namfoop midi`)

**Ce que couvre ce fichier** : l'écran plein écran de `namfoop midi` — ses trois onglets (Appareils
et canaux, Plan du set, Fiche), la fenêtre « Régler », la zone « reçu », l'enregistrement et la
sortie, et le clavier. Les conventions de message sont dans l'[ADR 0011](../adr/0011-midi-conventions-fractal.md),
l'horloge et le tap dans l'[ADR 0008](../adr/0008-tempo-et-divisions.md), l'ordre des étages du
clavier dans l'[ADR 0009](../adr/0009-clavier-de-browse.md), le format d'un set dans
[`set.md`](set.md), le CC de l'accordeur dans [`accordeur.md`](accordeur.md) § 3.

**État** : spécification d'un écran **à construire**. Rien ici n'est vérifié par un test : c'est la
cible que suivent les stories de l'épique « Régler le MIDI avec mon groupe, sans écrire la config ».
À mesure qu'elles sont livrées, le code et l'oracle du clavier font foi, et ce paragraphe se retire.

La maquette [`midi.html`](midi.html) est une **illustration actionnable** : à ouvrir dans un
navigateur, elle se pilote au clavier et simule le matériel par des boutons. Les appareils, morceaux
et sons qu'elle montre sont fictifs, et son JavaScript n'est pas à reprendre.

**Couleurs** : l'écran MIDI n'est pas un espace de `browse` ; il est **neutre**, comme `setup` et
`run` ([charte](../identite/charte.md), « Dans le terminal ») : texte blanc, texte secondaire et cadres
gris acier, onglet actif et ligne visée en inverse blanc. Les autres couleurs portent un état et rien
d'autre :

| Couleur | Ce qu'elle dit ici |
|---|---|
| vert signal | reçu, enregistré, l'horloge suivie |
| jaune ambre | une saisie en cours (la fenêtre « Régler »), et ce qui demande attention : hors accord, non enregistré, horloge ignorée, plusieurs canaux qui parlent |
| rouge clip | un refus : numéro hors bornes ou déjà pris, message du mauvais type |
| violet UV | « basse coupée », seulement dans la zone « reçu », quand un message lancerait l'accordeur |

Ces noms désignent les teintes de la charte, qui dit aussi quand le terminal les remplace par ses
couleurs de base.

## 1. Le besoin et les règles

Deux activités différentes règlent le MIDI, et l'écran sert les deux :

| | Régler un appareil branché | S'accorder avec le groupe |
|---|---|---|
| Avec qui | seul | l'ingé son, le batteur |
| L'appareil est là | oui | non : c'est l'Ableton d'un autre |
| Geste naturel | appuyer | taper ce qu'on a convenu |
| Ce qui en sort | un réglage | un texte à envoyer, puis un réglage |
| Vérification | tout de suite | plus tard, en balance |

1. **Une fenêtre, deux gestes.** Régler une ligne, c'est taper son numéro **ou** appuyer sur
   l'appareil : la même fenêtre accepte les deux.
2. **Chaque message reçu dit ce que namfoop en fait**, ou pourquoi il n'en fait rien.
3. **Régler ne change jamais le son** : tant qu'une fenêtre de réglage est ouverte, un message est
   lu, pas joué.
4. **Ce qui arrive sur un canal non réglé est ignoré sans bruit** : compté, pas listé.
5. **Sans rien régler, rien ne change** : tous les appareils, tous les canaux, les numéros de
   l'ADR 0011 et le CC 15 de l'accordeur.

## 2. Des appareils et des canaux

namfoop ne connaît pas de rôles — « le groupe », « mon pédalier » : dès qu'un clavier ou un autre
appareil envoie du MIDI, ils perdent leur sens. Il raisonne en **appareils** et en
**canaux**.

**Un appareil** est une entrée MIDI du système, désignée par **son nom** : l'entrée DIN d'une
interface (`UM-ONE`), chaque appareil USB (`MC6 Pro`, `Digital Piano`). Le nom ne dépend pas de
l'ordre de branchement.

- **Tous les appareils sont écoutés**, ensemble. On en décoche un pour que namfoop l'ignore — un
  clavier qui parle sur le canal du pédalier, par exemple.
- Un appareil absent n'empêche pas de démarrer ; il est repris dès qu'on le branche.
- **L'appareil sert à une seule chose de plus : l'horloge et le transport** (`Start`, `Stop`,
  `Continue`, la position). Ces messages n'ont pas de canal, et deux horloges ne se mêlent jamais :
  - sans choix, l'horloge suivie est celle du **premier appareil qui en envoie** ; il la garde tant
    qu'il bat (500 ms sans top : perdue, ADR 0008), puis le suivant prend la main ;
  - choisi, un appareil est **le seul** dont l'horloge compte ; s'il se tait, le tap reprend ;
  - une horloge venue d'un autre appareil est **ignorée et signalée** sur sa ligne.
- Sur une prise fusionnée (Ableton relayé par le pédalier), il ne doit arriver qu'une horloge : le
  guide dit de couper celle du pédalier.

**Un canal** porte les numéros de ses gestes : le morceau (Program Change), le son du morceau, le
tap tempo, l'accordeur.

- **Les numéros appartiennent au canal**, quel que soit l'appareil qui l'emprunte : le canal 5
  appelle les sons par le CC 20 pendant que le canal 1 garde le CC 34.
- **Sans canal réglé**, tous les canaux sont écoutés avec les mêmes numéros : c'est la ligne
  « Tous les canaux ». **Dès qu'un canal est réglé**, seuls les canaux réglés sont écoutés ; les
  autres sont comptés, pas listés.
- **Le dernier message est suivi**, d'où qu'il vienne. Il n'y a pas de mode secours : si Ableton se
  tait, un tap au pied reprend le tempo (ADR 0008).
- Un son n'a pas de numéro de Program Change ([`set.md`](set.md) § 3) : un appareil qui ne sait
  envoyer que des Program Change ne peut pas appeler un son. C'est une limite connue, que la fiche
  rend visible.
- Deux appareils de même nom branchés ensemble ne se distinguent pas ; namfoop n'envoie rien.

## 3. Le squelette de l'écran

`namfoop midi` n'ouvre pas la carte son. L'écran reprend le squelette de `browse`
([`browse-espaces.md`](browse-espaces.md) § 2) : une ligne d'en-tête, une ligne d'onglets, un cadre
à bandeau, une zone de message, puis ce qui vaut pour tous les onglets, et la barre d'aide.

```
 namfoop midi                                                          config ● non enregistrée (S)
   Appareils et canaux ●  Plan du set (6)    Fiche
┌ MIDI · APPAREILS ET CANAUX — 3 appareils · horloge : UM-ONE · 2 canaux réglés ────────────────┐
│ …                                                                                             │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
 le son du morceau, canal 5 : CC 20
 reçu  UM-ONE      can. 5   CC 20  1   → son 2 « Clean SVT »
       UM-ONE      can. 5   CC 20  5   hors accord : « Le Phare » n'a que 2 sons
       canaux non réglés : 2 messages sur le canal 10, ignorés sans bruit

 ↑↓ ligne   Entrée régler   a ajouter un canal   x retirer   b couper / écouter   Tab/→ Plan du set
 S enregistrer   h aide   q quitter
```

- **En-tête, à droite** : l'état d'enregistrement de ce que l'onglet règle — `config ✓` /
  `config ● non enregistrée (S)` sur Appareils et canaux et Fiche, `set : <fichier> ✓` /
  `set : <fichier> ● non enregistré (S)` sur Plan du set.
- **Onglets** : l'onglet actif est en inverse blanc. Un `●` jaune suit le nom d'un onglet dont le
  contenu n'est pas enregistré : il se voit depuis les deux autres.
- **Zone « reçu »** (§ 8) : les trois derniers messages et le compte des canaux non réglés. Elle est
  commune aux trois onglets, comme les vumètres le sont aux espaces de `browse`.
- **L'écran est pensé comme un espace** : le jour où il entre dans `browse`, la première ligne
  devient celle des espaces et le reste ne bouge pas.

## 4. Onglet Appareils et canaux

Une seule liste en deux sections. `↑↓` passe d'une ligne à l'autre en sautant les titres de section
et les lignes d'information ; la liste défile quand elle déborde.

```
┌ MIDI · APPAREILS ET CANAUX — 3 appareils · horloge : UM-ONE · 2 canaux réglés ────────────────┐
│ APPAREILS · tous écoutés, sauf ceux décochés                                                  │
│   ☑ UM-ONE             branché   canaux 5 · 10    ♩ 124 ⏵ horloge · choisie                   │
│   ☑ MC6 Pro            branché   canal 1          horloge ignorée : UM-ONE la donne           │
│   ☐ Digital Piano      branché   décoché · 14 messages ignorés                                │
│                                                                                               │
│ CANAUX · seuls les canaux réglés sont écoutés                                                 │
│   Canal 5                                         dernier : UM-ONE · il y a 3 s               │
│     Morceau            Program Change             ● PC 2 « Le Phare » · il y a 40 s           │
│ ▶   Son du morceau     CC 20 · défaut : 34        ● valeur 1 · il y a 12 s                    │
│     Tap tempo          CC 14                      ○ jamais reçu                               │
│     Accordeur          CC 15                      ○ jamais reçu                               │
│   Canal 1                                         dernier : MC6 Pro · il y a 3 min            │
│     Morceau            Program Change             ○ jamais reçu                               │
│     Son du morceau     CC 69 · défaut : 34        ● valeur 1 · il y a 3 min                   │
│     Tap tempo          CC 14                      ● il y a 3 min                              │
│     Accordeur          CC 15                      ○ jamais reçu                               │
│   Autres canaux        ignorés                    12 messages sur le canal 10                 │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Les appareils** : une ligne par appareil branché, et par appareil absent que la config nomme
(décoché, ou choisi pour l'horloge), grisé avec `absent`. Colonnes : la case d'écoute, le nom,
`branché` ou `absent`, les canaux vus en direct, et l'horloge.

| Colonne horloge | Quand |
|---|---|
| `♩ 124 ⏵ horloge · le premier qui bat`, en vert | sans choix, l'appareil dont l'horloge est suivie |
| `♩ 124 ⏵ horloge · choisie`, en vert | l'appareil choisi, qui bat |
| `horloge choisie · muette : le tap reprend`, en jaune | l'appareil choisi, qui ne bat pas ou est absent |
| `horloge ignorée : UM-ONE la donne`, en jaune | un autre appareil qui envoie une horloge |
| rien | un appareil qui n'envoie pas d'horloge |

**Les canaux** : un bloc par canal réglé, par numéro croissant, puis la ligne `Autres canaux` qui
compte ce qui est ignoré. Sans canal réglé, un seul bloc, `Tous les canaux`, et sa ligne d'en-tête
parle : `vus : 1 · 5 · 10 · plusieurs canaux parlent`, en jaune, dès que deux canaux ou plus ont été
vus.

Trois colonnes sur une ligne de geste : le geste, sa valeur, et **ce qui a été reçu en dernier**
pour lui — `●` vert et le temps écoulé, ou `○ jamais reçu`. Cette colonne est la vérification
d'avant concert : on appuie sur chaque bouton, et sa ligne s'allume. La ligne du canal dit quel
appareil l'a emprunté en dernier.

| Ligne | Valeur | `Entrée` | `b` | `x` |
|---|---|---|---|---|
| Appareil | case, nom, état | lui confier l'horloge ; sur l'appareil choisi, rendre l'horloge au premier qui bat | décocher / cocher | — |
| Canal n | — | la fenêtre « Régler » (§ 5), pour changer de numéro | — | retire le canal |
| Tous les canaux | — | la fenêtre « Régler » : le numéro choisi devient le premier canal réglé | — | — |
| Gabarit | son nom, ou `personnalisé` | la liste des gabarits, avec leurs numéros ; choisir pose les trois CC | — | — |
| Morceau | `Program Change` | rien à régler, un message le dit | coupe / rétablit | — |
| Son du morceau | `CC n`, suivi de `· défaut : 34` s'il en diffère | la fenêtre « Régler » | coupe / rétablit | — |
| Tap tempo | `CC n` (défaut 14) | la fenêtre « Régler » | coupe / rétablit | — |
| Accordeur | `CC n` (défaut 15 : 64 ou plus coupe la basse, moins de 64 rend le son) | la fenêtre « Régler » | coupe / rétablit | — |

- **`a` ajoute un canal**, depuis n'importe quelle ligne, par la fenêtre « Régler » : on tape le
  numéro ou on appuie sur un bouton de l'appareil.
- **Le premier canal réglé reprend les numéros de « Tous les canaux »** : ce qu'on y avait tapé
  n'est pas perdu. Les suivants partent des numéros de l'ADR 0011. Retirer le dernier canal rend
  « Tous les canaux », avec ses numéros.
- **`x` retire le canal visé**, sans fenêtre : rien n'est écrit avant `S`. Ailleurs que sur une ligne
  « Canal n », il dit `x retire un canal : vise sa ligne « Canal n »`.
- **Un geste coupé** affiche `coupé (b pour écouter)` à la place de ce qui a été reçu, et sa valeur
  est grisée. **Un appareil décoché** est grisé, sa case vide, et compte ce qu'il a envoyé. Sur une
  ligne qui n'est ni un geste ni un appareil, `b` dit `b coupe un geste ou un appareil : vise sa ligne`.
- Décocher l'appareil choisi pour l'horloge garde le choix : la colonne dit alors
  `horloge choisie · muette : le tap reprend`.
- Barre d'aide :
  `↑↓ ligne   Entrée régler   a ajouter un canal   x retirer   b couper / écouter   Tab/→ Plan du set`.

## 5. La fenêtre « Régler »

La même fenêtre sert à toutes les lignes à numéro : un canal (ajouté, ou changé), un numéro de CC,
le numéro de Program Change d'un morceau (§ 6).

```
┌ Régler · Canal 5 › Son du morceau ─────────────────────────────────────┐
│ Numéro de CC :  20▏   de 1 à 127 · vide = 34, le défaut                │
│                                                                        │
│ Ou appuie sur le bouton, si l'appareil est branché :                   │
│ ○ rien reçu pour l'instant                                             │
│ Tant que cette fenêtre est ouverte, aucun message ne change le son.    │
│                                                                        │
│ Entrée garder   Retour effacer   Échap ne rien changer                 │
└────────────────────────────────────────────────────────────────────────┘
```

- **C'est une saisie, donc un mode** (ADR 0009) : seuls les chiffres, `Retour`, `Entrée` et `Échap`
  y agissent. `h` et `q` y sont inertes. Son cadre, son titre et la saisie sont en jaune.
- **Une saisie vide rend le défaut** : le numéro de l'ADR 0011 pour un CC, `pas de numéro` pour un
  morceau. Pour un canal, elle est refusée : on retire un canal par `x`.
- **Appuyer remplit la saisie** : un message arrivé pendant que la fenêtre est ouverte est
  **capturé**. Il écrit son numéro dans la saisie, la ligne du dessous dit ce qui a été reçu et d'où
  (`● reçu : CC 69 · valeur 1 · canal 1 · MC6 Pro`, en vert), et il n'a **aucun autre effet**.
  `Entrée` reste nécessaire pour garder.
- **Ce qui est capturé dépend de la ligne** : le canal de n'importe quel message à canal ; le numéro
  d'un CC ou d'un Program Change arrivé sur le canal de la ligne (sur n'importe quel canal pour
  « Tous les canaux »).
- **Un appui sur un canal non réglé propose de l'ajouter** : sur une ligne de geste, un CC reçu sur
  le canal 1, qui n'est pas réglé, remplit la saisie et dit, en jaune,
  `canal 1 non réglé : Entrée l'ajoute, CC 69 pour le son du morceau`. `Entrée` ajoute
  alors le canal 1 et règle ce geste-là ; la ligne visée ne change pas.

Les refus s'écrivent en rouge, laissent la fenêtre ouverte et ne changent rien :

| Cas | Ce que dit la fenêtre |
|---|---|
| un message du mauvais type | `c'est un Program Change : cette ligne attend un CC` / `c'est un CC : un morceau s'appelle par Program Change` |
| un message d'un autre canal réglé | `ce message arrive sur le canal 1 : cette ligne règle le canal 5` |
| un message d'un appareil décoché | `Digital Piano est décoché : b sur sa ligne pour l'écouter` |
| une horloge, `Start`, `Stop` | `l'horloge n'a ni canal ni numéro : rien à prendre` |
| un canal hors bornes, ou vide | `un canal va de 1 à 16` / `un canal va de 1 à 16 : x retire un canal` |
| un canal déjà réglé | `le canal 5 est déjà réglé` |
| un CC hors bornes, ou 0 | `un numéro de CC va de 1 à 127` / `CC 0 est réservé au choix du set` |
| un CC déjà pris dans le même canal | `CC 14 appelle déjà le tap tempo du canal 5 : un bouton, un geste` |
| un numéro de morceau hors bornes, ou pris | `un numéro de PC va de 1 à 128` / `PC 2 est déjà pris par « Le Phare »` |

`Échap` ferme sans rien changer (`rien de changé`).

**Les listes** (le canal du Plan du set et de la Fiche, § 6) sont des fenêtres du même genre, mais
pas des saisies : cadre gris acier, `↑↓` choisit, `Entrée` garde, `Échap` ne change rien.

## 6. Onglet Plan du set

Le set du moment, vu par ce qu'Ableton ou le pédalier doit envoyer.

```
┌ MIDI · PLAN DU SET — Groupe 1 · Live Mairie · 6 morceaux · 1 sans numéro · sons du canal 5 ───┐
│     Morceau                   Tempo   Program Change      Reçu                                │
│   1 Ouverture                 ♩96     PC 1                ● il y a 6 min                      │
│   2 ♪ Le Phare                ♩124    PC 2                ● il y a 40 s                       │
│       ○ son 1 · Fuzz Face             CC 20 = 0           ● il y a 40 s                       │
│       ● son 2 · Clean SVT             CC 20 = 1           ● il y a 12 s                       │
│   3 Néon                      ♩100    PC 3                ○ pas encore                        │
│ ▶ 6 Rappel                    ♩110    PC —                pas de numéro : on ne peut pas l'…  │
│                                                                                               │
│ Les sons et leur ordre se changent dans le Set. Ici, seul le numéro du morceau se règle.      │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
```

- `↑↓` vise un morceau. `i` déplie ou replie ses sons, comme dans l'espace Set.
- `Entrée` règle son numéro par la fenêtre du § 5 : au clavier, ou en recevant le Program Change.
- Le numéro d'un morceau vaut pour tous les canaux écoutés. La valeur de CC d'un son est **sa
  place** dans le morceau ; le numéro de CC est celui du **canal choisi**, affiché dans le bandeau,
  et ne se règle pas ici.
- **`C` choisit le canal** parmi les canaux réglés, par une liste ; par défaut, le premier. Sans
  canal réglé, le plan suit « Tous les canaux » et `C` le dit :
  `aucun canal réglé : le plan vaut pour tous les canaux`. Le choix est commun au Plan du set et à
  la Fiche.
- **Un morceau sans numéro** est en jaune : personne ne peut l'appeler.
- **La colonne « Reçu » est la liste de contrôle de la balance** : la ligne d'un morceau ou d'un son
  s'allume quand son message arrive, sur n'importe quel canal écouté, avec le temps écoulé. `♪`
  marque le morceau joué, `●` son son.
- `L` choisit un autre set, par la fenêtre de [`set.md`](set.md) § 7.
- **C'est la seule entorse** à « un espace n'agit pas sur l'objet d'un autre » : le numéro d'un
  morceau fait partie de l'accord avec le groupe, donc il se règle où l'accord se règle. Rien d'autre
  du set ne se modifie ici.
- Barre d'aide :
  `↑↓ morceau   Entrée n° de PC   i détail   C canal   L sets   Tab/→ Fiche   Maj+Tab/← Appareils`.

## 7. Onglet Fiche

Le texte à envoyer au groupe pour **le canal choisi** (`C`, § 6), **calculé** depuis ses réglages et
le set du moment — même non enregistrés. `↑↓` le fait défiler ; `… N lignes plus bas (↓)` quand il
déborde.

```
Basse (namfoop) · ce que j'attends en MIDI

Canal 5. Les autres sont ignorés.
Morceau : Program Change, compté de 1 à 128
  (logiciel qui compte de 0 à 127 : retire 1)
Son du morceau : CC 20, valeur 0 = 1er son
Tap tempo : CC 14
Tempo : horloge MIDI, Start, Stop, position
Accordeur : CC 15 = 127 coupe ma basse
  CC 15 = 0, un PC ou Start la rendent
Je n'envoie rien. Le reste est ignoré.

Set « Groupe 1 · Live Mairie »
PC 1   Ouverture         ♩96
       CC 20 = 0  Clean SVT
PC 2   Le Phare          ♩124
       CC 20 = 0  Fuzz Face
       CC 20 = 1  Clean SVT
PC ?   Rappel            ♩110  à convenir
       CC 20 = 0  Fuzz Face
```

- **Du texte nu** : lignes de 46 colonnes au plus, ni tableau ni cadre, aucune mise en forme propre
  à une messagerie. Collé dans un message, il se lit sur un téléphone.
- **Il dit comment compter** : namfoop numérote les Program Change de 1 à 128, et bien des logiciels
  de 0 à 127.
- Un geste coupé n'y figure pas. Sans canal réglé, la ligne du canal devient
  `Canal : à choisir. Lequel tu me réserves ?`, et les numéros sont ceux de « Tous les
  canaux ».
- La ligne `Tempo` ne dépend pas du canal : l'horloge et le transport n'en ont pas.
- `Entrée` écrit la fiche dans le fichier « fiche-midi.txt » du dossier courant, et la zone de
  message donne son chemin. Une option de `namfoop midi` imprime le même texte sans ouvrir l'écran,
  pour un canal donné ou, sans lui, le premier canal réglé.
- Le bandeau nomme le canal : `MIDI · FICHE — canal 5 · tirée des réglages et du set`.
- Barre d'aide : `↑↓ défiler   Entrée écrire la fiche dans un fichier   C canal   Maj+Tab/← Plan du set`.

## 8. La zone « reçu »

Trois lignes pour les trois derniers messages, puis une ligne de compte. Une ligne se lit :
l'appareil, le canal, le message, ce que namfoop en fait.

| Ce qui arrive | Ce que la zone dit |
|---|---|
| un message reconnu | `→ morceau « Le Phare », son 1 « Fuzz Face »` · `→ son 2 « Clean SVT »` · `→ tap tempo` |
| le CC de l'accordeur | `→ basse coupée, accordeur`, en violet · `→ son rendu` |
| l'horloge suivie, `Start` | `→ ♩ 124,0 BPM` · `→ la battue repart du temps 1 ⏵` — une ligne par changement, jamais par top |
| un message sur un canal écouté qui ne correspond à rien, en jaune | `hors accord : aucun morceau du set n'a le numéro 9` · `hors accord : « Le Phare » n'a que 2 sons` · `hors accord : aucun morceau n'est joué` · `relié à aucun geste` |
| un geste coupé | `coupé : le son de ce canal est ignoré (b)` |
| un message sur un canal non réglé | rien dans la liste : `canaux non réglés : 212 messages sur les canaux 2 · 10, ignorés sans bruit` |
| un message d'un appareil décoché, une horloge ignorée | rien dans la liste : la ligne de l'appareil les compte (§ 4) |

L'écran tient son propre « morceau joué » — le dernier Program Change reconnu — pour savoir quel son
un CC désigne, et sa propre « basse coupée » pour dire ce que ferait l'accordeur. Un message capturé
par la fenêtre « Régler » n'entre pas dans la zone.

## 9. Enregistrer et quitter

- **`S` enregistre ce qu'on a devant soi** (ADR 0009, « l'éclipse se déclare ») : la config sur
  Appareils et canaux, le set sur Plan du set, les deux sur Fiche. Il faut donc deux `S` pour tout
  garder, et c'est voulu. Ce que la config garde : les appareils décochés, l'appareil choisi pour
  l'horloge, et chaque canal réglé avec ses numéros et ses gestes coupés.
- **Changer d'onglet ne demande rien** — comme changer d'espace dans `browse` — mais prévient :
  `config non enregistrée : S sur Appareils et canaux pour la garder` en jaune, et le `●` reste sur
  l'onglet.
- **`q` sans modification quitte.** Avec une modification, la fenêtre de `browse`
  ([`browse-chaine.md`](browse-chaine.md) § 8), qui dit ce qui serait perdu :

```
┌ Modifications non enregistrées ────────────────────────────────┐
│ La config MIDI a changé depuis le dernier enregistrement.      │
│ Le set « groupe-1 » a changé depuis le dernier enregistrement. │
│                                                                │
│ S       enregistrer puis quitter                               │
│ Q       quitter sans enregistrer                               │
│ Échap   rester (par défaut)                                    │
└────────────────────────────────────────────────────────────────┘
```

  Toute autre touche reste.
- `Échap`, hors fenêtre, dit `Échap ne quitte pas : q pour quitter`.

## 10. Le clavier

L'écran suit l'ordre de l'ADR 0009 — les modes, puis les sorties et l'aide, l'onglet, les communes,
le dernier recours — et ne prend aucune lettre qui dise autre chose ailleurs.

| Touche | Ici | D'où elle vient |
|---|---|---|
| `Tab` / `Maj+Tab` | onglet suivant / précédent, en boucle | les onglets de la Bibliothèque |
| `→` / `←` | onglet voisin ; au bout : `→ ne va pas plus loin` | idem |
| `↑↓` | viser une ligne, un morceau, ou défiler | partout : viser ne change rien |
| `Entrée` | le geste explicite : régler, confier l'horloge, écrire la fiche | partout |
| `a` | ajouter un canal | ajouter un bloc dans la Chaîne, un morceau dans l'édition du set |
| `x` | retirer le canal visé | retirer un bloc dans la Chaîne |
| `b` | couper / rétablir un geste ; décocher / cocher un appareil | couper un bloc dans la Chaîne |
| `i` | déplier les sons d'un morceau | l'espace Set |
| `C` | choisir le canal du Plan du set et de la Fiche, par une liste | comme `L`, une fenêtre de choix |
| `L` | choisir un set | l'espace Set |
| `S` | enregistrer ce qu'on a devant soi | commune, éclipsée comme dans l'espace Set |
| `h` / `?` | l'aide de l'onglet ; n'importe quelle touche la referme | la pédale |
| `q` | quitter, par la fenêtre s'il le faut | la pédale |
| `Échap` | ne quitte pas | le dernier recours |

- La barre du bas tient sur deux lignes : les touches de l'onglet, puis `S`, `h`, `q`, toujours dans
  cet ordre.
- Une touche sans propriétaire ne dit rien à l'écran.
- `[` et `]` ne choisissent pas le canal : ils **déplacent** un bloc ou un morceau dans `browse`.
  `←` et `→` restent aux onglets.
- `*` et `k` (le tap et le clic) n'ont rien à faire sans carte son : ils disent où ils vivent, comme
  toute touche de la pédale qui n'a pas d'objet. `P` et `n` sont éclipsées.

## 11. Tailles de terminal

Mêmes cibles que `browse` ([`browse-espaces.md`](browse-espaces.md) § 7). En 80×25, l'onglet
Appareils et canaux garde ses deux sections et défile : la colonne « reçu en dernier » se tronque
avant la valeur, la colonne horloge d'un appareil se réduit à `♩ 124 ⏵`, `ignorée` ou `muette`, la
zone « reçu » passe à deux lignes, et la fenêtre « Régler » tient dans 70 colonnes. Le Plan du set
défile.

## 12. Ce que la maquette simule

Trois appareils : `UM-ONE`, l'entrée DIN où arrivent Ableton (canal 5) et les lights (canal 10) ;
`MC6 Pro`, un pédalier USB sur le canal 1, qui envoie aussi une horloge ; `Digital Piano`, un clavier
USB qui parle sur le canal 1. Le parcours : décocher le clavier, confier l'horloge à `UM-ONE`,
ajouter le canal d'Ableton en appuyant, puis celui du pédalier, relier un CC du pédalier en
appuyant, taper le CC convenu avec le groupe, donner son numéro à un morceau, écrire la fiche,
enregistrer.

Elle simule aussi le débranchement du clavier, et le CC de l'accordeur. Elle ne simule pas : le
choix d'un autre set (`L` le dit), l'écriture réelle d'un fichier, les tailles de terminal, ni `*`
et `k`.

## 13. Tests attendus

Logique pure (horloge fournie) : chaque ligne des tableaux des § 4, 5 et 8, dont « un message
capturé n'a aucun autre effet », « un refus ne change rien », « `Q` ne garde rien », « le premier
canal reprend les numéros de Tous les canaux », « une seule horloge suivie ». Rendu avec
`TestBackend` / `screen_text` : les trois onglets, avec et sans canal réglé, la fenêtre « Régler »,
la fenêtre de sortie, en 80×25 et en 160×50. L'oracle du clavier : chaque touche dans chaque contexte
de l'écran. De bout en bout : pseudo-terminal, `--no-config`, deux appareils simulés
(`--midi-in sim:<chemin>`) qui jouent le parcours de la maquette, puis la config relue.
