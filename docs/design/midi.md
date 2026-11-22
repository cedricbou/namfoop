# Spécification · l'écran MIDI (`namfoop midi`)

**Ce que couvre ce fichier** : l'écran plein écran de `namfoop midi` — ses trois onglets (Sources,
Plan du set, Fiche), la fenêtre « Régler », la zone « reçu », l'enregistrement et la sortie, et le
clavier. Les conventions de message sont dans l'[ADR 0011](../adr/0011-midi-conventions-fractal.md),
l'horloge et le tap dans l'[ADR 0008](../adr/0008-tempo-et-divisions.md), l'ordre des étages du
clavier dans l'[ADR 0009](../adr/0009-clavier-de-browse.md), le format d'un set dans
[`set.md`](set.md).

**État** : spécification d'un écran **à construire**. Rien ici n'est vérifié par un test : c'est la
cible que suivent les stories de l'épique « Régler le MIDI avec mon groupe, sans écrire la config ».
À mesure qu'elles sont livrées, le code et l'oracle du clavier font foi, et ce paragraphe se retire.

La maquette [`midi.html`](midi.html) est une **illustration actionnable** validée avec
l'utilisateur : à ouvrir dans un navigateur, elle se pilote au clavier et simule le matériel par des
boutons. Les ports, morceaux et sons qu'elle montre sont fictifs, et son JavaScript n'est pas à
reprendre.

**Couleurs** : « magenta », « cyan », « vert », « jaune », « rouge » et « gris » désignent ici le
magenta électrique, le cyan néon, le vert signal, le jaune ambre, le rouge clip et le gris acier de la
[charte](../identite/charte.md), qui dit aussi quand le terminal les remplace par ses couleurs de base.

## 1. Le besoin et les règles

Deux activités différentes règlent le MIDI, et l'écran sert les deux :

| | Appairer un appareil | S'accorder avec le groupe |
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
4. **Ce qui n'est pas sur mon canal est ignoré sans bruit** : compté, pas listé.
5. **Sans rien régler, rien ne change** : tous les canaux, les numéros de l'ADR 0011.

## 2. Les sources

namfoop écoute **des sources**. Une source est un port, un canal, et ses propres numéros de
message :

| | Le groupe | Mon pédalier |
|---|---|---|
| Rôle | la source normale en concert (Ableton) | les répétitions, et le secours en concert |
| Envoie | le morceau, le son, l'horloge, `Start` / `Stop` | le preset ou le morceau, le son, le tap |
| Se règle | en tapant ce qu'on a convenu | en appuyant, ou par un gabarit |

- **Les numéros appartiennent à la source** : le groupe peut appeler les sons par le CC 20 pendant
  que le pédalier garde le CC 34.
- **Les sources sont toujours écoutées, la dernière qui parle est suivie.** Il n'y a pas de mode
  secours : si Ableton se tait, un tap au pied reprend le tempo (ADR 0008).
- **L'horloge, `Start` et `Stop` n'ont pas de canal** : le réglage du canal ne les filtre pas.
- Un son n'a pas de numéro de Program Change ([`set.md`](set.md) § 3) : une source qui ne saurait
  envoyer que des Program Change ne peut pas appeler un son. C'est une limite connue, que la fiche
  rend visible.

L'onglet Sources dessine **une section par source écoutée**. Avec une seule source, il n'a qu'une
section, et tout le reste de ce fichier vaut tel quel.

## 3. Le squelette de l'écran

`namfoop midi` n'ouvre pas la carte son. L'écran reprend le squelette de `browse`
([`browse-espaces.md`](browse-espaces.md) § 2) : une ligne d'en-tête, une ligne d'onglets, un cadre
à bandeau, une zone de message, puis ce qui vaut pour tous les onglets, et la barre d'aide.

```
 namfoop midi                                                          config ● non enregistrée (S)
   Sources ●  Plan du set (6)    Fiche
┌ MIDI · SOURCES — 2 écoutées · la dernière qui parle est suivie ───────────────────────────────┐
│ …                                                                                             │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
 le son du morceau : CC 20
 reçu  groupe    can. 5   CC 20  1   → son 2 « Clean SVT »
       groupe    can. 5   CC 20  5   hors accord : « Le Phare » n'a que 2 sons
       pas pour moi : 2 messages sur le canal 10, ignorés sans bruit

 ↑↓ ligne   Entrée régler   b couper / écouter   Tab/→ Plan du set
 S enregistrer   h aide   q quitter
```

- **En-tête, à droite** : l'état d'enregistrement de ce que l'onglet règle — `config ✓` /
  `config ● non enregistrée (S)` sur Sources et Fiche, `set : <fichier> ✓` /
  `set : <fichier> ● non enregistré (S)` sur Plan du set.
- **Onglets** : l'onglet actif est en inverse, de la couleur de l'écran (cyan). Un `●` jaune suit le
  nom d'un onglet dont le contenu n'est pas enregistré : il se voit depuis les deux autres.
- **Zone « reçu »** (§ 8) : les trois derniers messages et le compte de ce qui n'est pas pour moi.
  Elle est commune aux trois onglets, comme les vumètres le sont aux espaces de `browse`.
- **L'écran est pensé comme un espace** : le jour où il entre dans `browse`, la première ligne
  devient celle des espaces et le reste ne bouge pas.

## 4. Onglet Sources

Une seule liste, une section par source, une ligne par réglage. `↑↓` passe d'une ligne à l'autre
en sautant les titres de section.

```
┌ MIDI · SOURCES — 2 écoutées · la dernière qui parle est suivie ───────────────────────────────┐
│ LE GROUPE · ce qu'on a convenu                                                                │
│   Port                UM-ONE · 24:0                 ● branché                                 │
│   Canal               5                             pas pour moi : canal 10                   │
│   Morceau             Program Change                ● PC 2 « Le Phare » · il y a 40 s         │
│ ▶ Son du morceau      CC 20 · défaut : 34           ● valeur 1 · il y a 12 s                  │
│   Tempo et battue     horloge · Start · Stop        ● il y a 3 s · ♩ 124 ⏵                    │
│   Tap tempo           pas convenu                                                             │
│                                                                                               │
│ MON PÉDALIER · répétitions et secours                                                         │
│   Port                MC6 MKII MIDI 1 · 28:0        ● branché                                 │
│   Canal               tous                          vu : canal 1                              │
│   Gabarit             Fractal                                                                 │
│   Preset ou morceau   Program Change                ○ jamais reçu                             │
│   Son du morceau      CC 34                         ○ jamais reçu                             │
│   Tap tempo           CC 14                         ● il y a 3 min                            │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
```

Trois colonnes : le réglage, sa valeur, et **ce qui a été reçu en dernier** pour lui — `●` vert et
le temps écoulé, ou `○ jamais reçu`. Cette colonne est la vérification d'avant concert : on appuie
sur chaque bouton, et sa ligne s'allume.

| Ligne | Valeur | `Entrée` | `b` |
|---|---|---|---|
| Port | le port écouté | une liste des ports vus | — |
| Canal | `tous`, ou 1 à 16 | la fenêtre « Régler » (§ 5) | — |
| Gabarit (pédalier) | son nom, ou `personnalisé` | une liste de gabarits avec leurs numéros | — |
| Morceau / Preset ou morceau | `Program Change` | rien à régler, un message le dit | coupe / rétablit |
| Son du morceau | `CC n`, suivi de `· défaut : 34` s'il en diffère | la fenêtre « Régler » | coupe / rétablit |
| Tempo et battue (groupe) | `horloge · Start · Stop` | rien à régler, un message le dit | coupe / rétablit |
| Tap tempo | `CC n`, ou `pas convenu` | la fenêtre « Régler » | coupe / rétablit |

- **La ligne Canal parle** : `vu : canal 1` quand un seul canal a été vu ;
  `vus ici : 5 · 10 · plusieurs appareils parlent`, en jaune, tant que le canal vaut `tous` et que
  deux canaux ou plus ont été vus sur cette source ; `pas pour moi : canal 10` une fois le canal fixé.
- **Un geste coupé** affiche `coupé (b pour écouter)` à la place de ce qui a été reçu, et sa valeur
  est grisée. Sur une ligne qui n'est pas un geste, `b` dit `b coupe un geste : vise une ligne de
  message`.
- **Le gabarit** vaut `personnalisé` dès qu'un numéro du pédalier ne correspond à aucun gabarit : il
  se déduit, il ne s'enregistre pas.
- Barre d'aide : `↑↓ ligne   Entrée régler   b couper / écouter   Tab/→ Plan du set`.

## 5. La fenêtre « Régler »

La même fenêtre sert aux deux sources et à toutes les lignes à numéro : le canal, un numéro de CC,
le numéro de Program Change d'un morceau (§ 6).

```
┌ Régler · Le groupe › Son du morceau ───────────────────────────────────┐
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
  y agissent. `h` et `q` y sont inertes.
- **Une saisie vide rend le défaut** : `tous` pour un canal, le numéro de l'ADR 0011 pour un CC,
  `pas convenu` pour le tap du groupe, `pas de numéro` pour un morceau.
- **Appuyer remplit la saisie** : un message arrivé pendant que la fenêtre est ouverte est
  **capturé**. Il écrit son numéro dans la saisie, la ligne du dessous dit ce qui a été reçu
  (`● reçu : CC 69 · valeur 1 · canal 1`), et il n'a **aucun autre effet**. `Entrée` reste
  nécessaire pour garder.
- **Ce qui est capturé dépend de la ligne** : le canal de n'importe quel message à canal, le numéro
  d'un CC, le numéro d'un Program Change.

| Cas | Ce que dit la fenêtre |
|---|---|
| un message du mauvais type | `c'est un Program Change : cette ligne attend un CC` / `c'est un CC : un morceau s'appelle par Program Change` |
| un message d'une autre source | `ce message vient du câble du groupe : il n'est pas pris ici` |
| une horloge | `l'horloge n'a ni canal ni numéro : rien à prendre` |
| un canal hors bornes | `un canal va de 1 à 16` |
| un CC hors bornes, ou 0 | `un numéro de CC va de 1 à 127` / `CC 0 est réservé au choix du set` |
| un CC déjà pris dans la même source | `CC 14 appelle déjà le tap tempo : un bouton ne fait qu'un geste` |
| un numéro de morceau hors bornes, ou pris | `un numéro de PC va de 1 à 128` / `PC 2 est déjà pris par « Le Phare »` |

Un refus laisse la fenêtre ouverte et ne change rien. `Échap` ferme sans rien changer
(`rien de changé`).

**Les listes** (port, gabarit) sont des fenêtres du même genre : `↑↓` choisit, `Entrée` garde,
`Échap` ne change rien.

## 6. Onglet Plan du set

Le set du moment, vu par ce que le groupe doit envoyer.

```
┌ MIDI · PLAN DU SET — Groupe 1 · Live Mairie · 6 morceaux · 1 sans numéro ─────────────────────┐
│     Morceau                   Tempo   Le groupe envoie    Reçu                                │
│   1 Ouverture                 ♩96     PC 1                ● il y a 6 min                      │
│   2 ♪ Le Phare                ♩124    PC 2                ● il y a 40 s                       │
│       ○ son 1 · Fuzz Face             CC 20 = 0           ● il y a 40 s                       │
│       ● son 2 · Clean SVT             CC 20 = 1           ● il y a 12 s                       │
│   3 Néon                      ♩100    PC 3                ○ pas encore                        │
│ ▶ 6 Rappel                    ♩110    PC —                pas de numéro : le groupe ne peut…  │
│                                                                                               │
│ Les sons et leur ordre se changent dans le Set. Ici, seul le numéro du morceau se règle.      │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
```

- `↑↓` vise un morceau. `i` déplie ou replie ses sons, comme dans l'espace Set.
- `Entrée` règle son numéro par la fenêtre du § 5 : au clavier, ou en recevant le Program Change.
- La valeur de CC d'un son est **sa place** dans le morceau : elle s'affiche avec le numéro de CC de
  la source du groupe, et ne se règle pas ici.
- **Un morceau sans numéro** est en jaune : le groupe ne peut pas l'appeler.
- **La colonne « Reçu » est la liste de contrôle de la balance** : la ligne d'un morceau ou d'un son
  s'allume quand son message arrive, avec le temps écoulé. `♪` marque le morceau joué, `●` son son.
- `L` choisit un autre set, par la fenêtre de [`set.md`](set.md) § 7.
- **C'est la seule entorse** à « un espace n'agit pas sur l'objet d'un autre » : le numéro d'un
  morceau fait partie de l'accord avec le groupe, donc il se règle où l'accord se règle. Rien d'autre
  du set ne se modifie ici.
- Barre d'aide :
  `↑↓ morceau   Entrée n° de PC   i détail   L sets   Tab/→ Fiche   Maj+Tab/← Sources`.

## 7. Onglet Fiche

Le texte à envoyer au groupe, **calculé** depuis les réglages de la source du groupe et le set du
moment — même non enregistrés. `↑↓` le fait défiler ; `… N lignes plus bas (↓)` quand il déborde.

```
Basse (namfoop) · ce que j'attends en MIDI

Canal 5. Le reste est ignoré.
Morceau : Program Change, compté de 1 à 128
  (si ton logiciel compte de 0 à 127, retire 1)
Son du morceau : CC 20, valeur 0 = 1er son
Tempo : horloge MIDI, Start, Stop, position
Je n'envoie rien. Tout autre message est ignoré.

Set « Groupe 1 · Live Mairie »
PC 1   Ouverture         ♩96
       CC 20 = 0  Clean SVT
PC 2   Le Phare          ♩124
       CC 20 = 0  Fuzz Face
       CC 20 = 1  Clean SVT
PC ?   Rappel            ♩110   numéro à convenir
       CC 20 = 0  Fuzz Face
```

- **Du texte nu** : lignes courtes, ni tableau ni cadre, aucune mise en forme propre à une
  messagerie. Collé dans un message, il se lit sur un téléphone.
- **Il dit comment compter** : namfoop numérote les Program Change de 1 à 128, et bien des logiciels
  de 0 à 127.
- Un geste coupé ou non convenu n'y figure pas. Sans canal fixé, la ligne devient
  `Canal : pas encore choisi. Lequel tu me réserves ?`.
- `Entrée` écrit la fiche dans le fichier « fiche-midi.txt » du dossier courant, et la zone de message
  donne son chemin. Une option de `namfoop midi` imprime le même texte sans ouvrir l'écran.
- Barre d'aide : `↑↓ défiler   Entrée écrire la fiche dans un fichier   Maj+Tab/← Plan du set`.

## 8. La zone « reçu »

Trois lignes pour les trois derniers messages, puis une ligne de compte. Une ligne se lit : la
source, le canal, le message, ce que namfoop en fait.

| Ce qui arrive | Ce que la zone dit |
|---|---|
| un message reconnu | `→ morceau « Le Phare », son 1 « Fuzz Face »` · `→ son 2 « Clean SVT »` · `→ tap tempo` |
| l'horloge, `Start` | `→ ♩ 124,0 BPM` · `→ la battue repart du temps 1 ⏵` — une ligne par changement, jamais par top |
| un message sur mon canal qui ne correspond à rien, en jaune | `hors accord : aucun morceau du set n'a le numéro 9` · `hors accord : « Le Phare » n'a que 2 sons` · `hors accord : aucun morceau n'est joué` · `relié à aucun geste` |
| un geste coupé | `coupé : le son de cette source est ignoré (b)` |
| un message sur un autre canal | rien dans la liste : `pas pour moi : 212 messages sur le canal 2 · 10, ignorés sans bruit` |

L'écran tient son propre « morceau joué » — le dernier Program Change reconnu — pour savoir quel son
un CC désigne. Un message capturé par la fenêtre « Régler » n'entre pas dans la zone.

## 9. Enregistrer et quitter

- **`S` enregistre ce qu'on a devant soi** (ADR 0009, « l'éclipse se déclare ») : la config sur
  Sources, le set sur Plan du set, les deux sur Fiche. Il faut donc deux `S` pour tout garder, et
  c'est voulu.
- **Changer d'onglet ne demande rien** — comme changer d'espace dans `browse` — mais prévient :
  `config non enregistrée : S sur Sources pour la garder` en jaune, et le `●` reste sur l'onglet.
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
| `Entrée` | le geste explicite : régler, ou écrire la fiche | partout |
| `b` | couper / rétablir un geste | couper un bloc dans la Chaîne |
| `i` | déplier les sons d'un morceau | l'espace Set |
| `L` | choisir un set | l'espace Set |
| `S` | enregistrer ce qu'on a devant soi | commune, éclipsée comme dans l'espace Set |
| `h` / `?` | l'aide de l'onglet ; n'importe quelle touche la referme | la pédale |
| `q` | quitter, par la fenêtre s'il le faut | la pédale |
| `Échap` | ne quitte pas | le dernier recours |

- La barre du bas tient sur deux lignes : les touches de l'onglet, puis `S`, `h`, `q`, toujours dans
  cet ordre.
- Une touche sans propriétaire ne dit rien à l'écran.
- `*` et `k` (le tap et le clic) n'ont rien à faire sans carte son : ils disent où ils vivent, comme
  toute touche de la pédale qui n'a pas d'objet. `P` et `n` sont éclipsées.

## 11. Tailles de terminal

Mêmes cibles que `browse` ([`browse-espaces.md`](browse-espaces.md) § 7). En 80×25, l'onglet
Sources garde ses deux sections : la colonne « reçu en dernier » se tronque avant la valeur, la zone
« reçu » passe à deux lignes, et la fenêtre « Régler » tient dans 70 colonnes. Le Plan du set
défile.

## 12. Ce que la maquette simule

Elle déroule un parcours en sept étapes : un bouton du pédalier que namfoop ne connaît pas, le
relier en appuyant, un message des lights qui change le morceau tant que le canal vaut `tous`, le
canal du groupe, le CC que le groupe a choisi, le numéro d'un morceau, la fiche, l'enregistrement.

Elle ne simule pas : le choix d'un autre set (`L` le dit), l'écriture réelle d'un fichier, les
tailles de terminal, ni `*` et `k`.

## 13. Tests attendus

Logique pure (horloge fournie) : chaque ligne des tableaux des § 4, 5 et 8, dont « un message
capturé n'a aucun autre effet », « un refus ne change rien », « `Q` ne garde rien ». Rendu avec
`TestBackend` / `screen_text` : les trois onglets, la fenêtre « Régler », la fenêtre de sortie, en
80×25 et en 160×50. L'oracle du clavier : chaque touche dans chaque contexte de l'écran. De bout en
bout : pseudo-terminal, `--no-config`, un port simulé (`--midi-in sim:<chemin>`) qui joue le
parcours de la maquette, puis la config relue.
