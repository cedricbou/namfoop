# Spécification · l'accordeur silencieux

**Ce que couvre ce fichier** : l'accordeur de namfoop, et la coupure de la basse qui va avec — ce
qui est coupé, ce qui lance et arrête l'accordeur, sa place dans chaque écran, son affichage, ses
couleurs, ses messages et son clavier. Les conventions MIDI sont dans
l'[ADR 0011](../adr/0011-midi-conventions-fractal.md), l'ordre des étages du clavier dans
l'[ADR 0009](../adr/0009-clavier-de-browse.md), la vue scène dans [`set.md`](set.md) § 6 et § 9, les
couleurs dans la [charte](../identite/charte.md).

**État** : spécification d'un écran **à construire**. Rien ici n'est vérifié par un test : c'est la
cible que suivent les stories de l'épique « M'accorder en silence, en répétition comme entre deux
morceaux ». À mesure qu'elles sont livrées, le code et l'oracle du clavier font foi, et ce
paragraphe se retire.

La maquette [`accordeur.html`](accordeur.html) est une **illustration actionnable** validée avec
l'utilisateur : à ouvrir dans un navigateur, elle se pilote au clavier, simule une basse, le MIDI et
les sorties de la carte. Le set, les sons et les textes qu'elle montre sont fictifs, et son
JavaScript n'est pas à reprendre.

**Couleurs** : « violet », « vert », « vert atténué », « jaune », « blanc » et « gris » désignent ici
le violet UV, le vert signal, le vert signal atténué (`#07854A`), le jaune ambre, le blanc pur et le
gris acier de la [charte](../identite/charte.md), qui dit aussi quand le terminal les remplace par
ses couleurs de base.

## 1. Le besoin et les règles

S'accorder sans que la salle entende rien, en répétition comme sur scène : entre deux morceaux,
pendant une scénette, la basse se coupe — à la main, au pied, ou parce qu'Ableton le demande — et
l'accordeur s'affiche.

1. **Silence garanti** : tant que l'accordeur est affiché, ma basse ne sort nulle part, ni traitée
   ni en DI.
2. **Un seul mode** : basse coupée et accordeur affiché, c'est la même chose. Il n'existe pas de
   muet sans accordeur, ni d'accordeur qui laisse passer le son.
3. **Jamais muet au début d'un titre** : changer de morceau ou de son rend le son, d'où que vienne
   le changement.
4. **L'accordeur ne ment pas** : plutôt « rien » qu'une note fausse.
5. **Seul l'accordeur s'allume** : derrière lui, l'écran s'éteint en gris, et la seule couleur vive
   qui reste est le vert de « juste ».
6. **Au démarrage, la basse n'est jamais coupée** : l'état ne s'enregistre pas.

## 2. Ce qui est coupé

| Sortie | Pendant l'accordage | Pourquoi |
|---|---|---|
| basse traitée (façade, in-ears) | **coupée** ; la queue de réverbe ou de delay de la dernière note finit de sonner | le morceau se termine naturellement |
| DI | **coupée** | la DI part en façade : sans elle, la salle entendrait l'accordage |
| micro | **continue** | je parle pendant une scénette |
| clic du métronome | **continue** | le groupe garde le tempo |

La coupure et le retour se font en 20 ms : ni l'un ni l'autre ne fait de clic. La chaîne continue
de tourner, alimentée en silence, comme la queue d'un changement de son ([`set.md`](set.md) § 4).
L'accordeur lit la basse **avant** la chaîne : une fuzz ne trompe pas la lecture.

## 3. Entrer et sortir

| Geste | Entre | Sort |
|---|---|---|
| la touche `u` (réglable) | oui | oui |
| CC 15 de valeur 64 ou plus (réglable) | oui | — |
| CC 15 de valeur inférieure à 64 | — | oui |
| un changement de morceau : Program Change, `→` / `←`, `Entrée` sur un morceau | — | oui |
| un changement de son : CC 34, `PgDn` / `PgUp` | — | oui |
| `Start` du transport | — | oui |
| `Continue`, l'horloge, le tap | — | non |

- **Le CC 15 n'est pas une bascule** : un CC 15 ≥ 64 alors que la basse est déjà coupée ne change
  rien, et le dit. Seule la touche `u` bascule.
- **Un Program Change suivi d'un CC 34** ne fait qu'un retour, comme il ne fait qu'un changement.
- Le CC 15 vient du monde Fractal : c'est le numéro de l'accordeur sur l'Axe-Fx II, de la même
  famille que le CC 14 du tap (ADR 0011). Fractal envoie 127 pour entrer et 0 pour sortir.

## 4. Sur scène : l'espace Set et `namfoop run --set`

L'accordeur prend **la colonne du prompteur**. La set list reste à gauche, et le **morceau suivant**
reste sous l'accordeur avec ses premières lignes : pendant une scénette, sa cue se lit.

```
 namfoop browse  1 Bibliothèque  2 Chaîne  [3 Set] Groupe 1 · Live Mairie   🔇 BASSE COUPÉE    ♩ 124 4/4 (du morceau)
┌ ♪ Groupe 1 · Live Mairie · 2/6 ──────────┐┌ ACCORDEUR · ma basse est coupée partout · micro et clic continuent ──┐
│   1 Ouverture             ♩96            ││                                                                      │
│ ▶ 2 Le Phare              ♩124           ││                        ██████████                                    │
│      ● Fuzz Face                         ││                        ██                                            │
│      ○ Clean SVT                         ││                        ████████                                      │
│   3 Néon                  ♩100           ││                        ██                                            │
│   4 Marée basse           ♩82  6/8       ││                        ██████████  octave 1                          │
│   5 Sur le fil            ♩117           ││                                                                      │
│   6 Rappel                ♩110           ││            -50       -25        0        +25       +50               │
│                                          ││             ┆·········┆·········┃·········┆·········┆                │
│                                          ││                                    ▲                                 │
│                                          ││                                                                      │
│                                          ││                     +7 cents    ▼ descends                           │
│                                          ││                                                                      │
│                                          ││ ── ensuite (→) : 3 · Néon  ♩100  Synth octave                        │
│                                          ││    › scénette : je présente le groupe, Léa lance la boucle           │
│                                          ││    INTRO    | Am . . . | Am . . . | F . . . | G . . . |              │
└──────────────────────────────────────────┘└──────────────────────────────────────────────────────────────────────┘
 ♪ basse coupée · la queue de la réverbe finit de sonner (0,8 s) · micro et clic continuent
 CC 15 = 127 → basse coupée, accordeur
 u rendre le son   →/← morceau (rend le son)   PgDn/PgUp son (rend le son)   q quitter
```

- **En-tête** : le bandeau `🔇 BASSE COUPÉE`, en aplat violet, s'ajoute après le nom du set ; il ne
  remplace ni le set, ni le tempo.
- **Set list** : inchangée, mais grise (§ 7). Le son joué garde sa surbrillance, en gris.
- **Ligne d'écoute** : `♪ basse coupée` en violet ; tant que la queue sonne, le temps qui lui reste.
- `run --set` garde son verrou : `u` y entre et sort comme `*` y tape (étage pédale, ADR 0009).

## 5. Ailleurs : une fenêtre

Dans la Bibliothèque, la Chaîne et `namfoop run --preset`, l'accordeur est une **fenêtre centrée**,
par-dessus l'écran grisé. Elle porte le même contenu que la colonne de scène, sans le morceau
suivant.

```
┌ ACCORDEUR · ma basse est coupée partout · micro et clic continuent ──┐
│                                                                      │
│                          ███████                                     │
│                        ██      ██                                    │
│                        ██████████                                    │
│                        ██      ██                                    │
│                        ██      ██  octave 1                          │
│                                                                      │
│            -50       -25        0        +25       +50               │
│             ┆·········┆·········┃·········┆·········┆                │
│                           ▲                                          │
│                                                                      │
│                     −25 cents    ▲ monte                             │
└──────────────────────────────────────────────────────────────────────┘
```

Les touches de l'écran derrière restent actives : on peut changer de bloc dans la Chaîne, et un
changement de son rend le son (§ 3).

## 6. L'accordeur

- **La note** en lettres de 5 lignes, chaque trait doublé pour se lire de loin, **en bémols** : C,
  D♭, D, E♭, E, F, G♭, G, A♭, A, B♭, B. L'octave en petit à droite, en gris.
- **Chromatique** : toutes les notes, du La à 27,5 Hz (et donc le Si grave d'une basse 5 ou 6
  cordes) jusqu'aux aigus d'une guitare. Le **La de référence** vaut 440 Hz et se règle (§ 10).
- **L'aiguille** : de -50 à +50 cents, 41 cases de 2,5 cents, un repère à 0, ±25 et ±50. Le pointeur
  `▲` sous la case de l'écart.
- **L'écart** en cents (`+7 cents`, `−25 cents`, `±0 cents`) et **le geste à faire** : `▼ descends`
  au-dessus, `▲ monte` en dessous, `✓ juste` à ±3 cents. La flèche dit le geste, pas l'état.
- **Lissage** : 20 lectures par seconde, la médiane des lectures d'environ 250 ms, et une note
  n'est affichée qu'après deux lectures d'accord : l'attaque d'une corde, qui monte de quelques
  cents, ne fait pas sauter l'aiguille, et une erreur d'octave isolée ne s'affiche pas.
- **Quand la corde s'éteint** : la dernière note reste **grisée 2 s**, aiguille vide, puis
  `joue une corde` en gris, au centre.

## 7. Les couleurs

**Le violet ne dit qu'une chose : la basse est coupée.** Le rouge garde son sens de danger
(saturation, erreurs) ; une coupure voulue n'en est pas un.

| Élément | Couleur |
|---|---|
| bandeau `🔇 BASSE COUPÉE` | aplat violet, texte noir d'encre |
| filet et titre du cadre de l'accordeur, ou de sa fenêtre | violet |
| `♪ basse coupée` sur la ligne d'écoute | violet |
| **tout le reste de l'écran** : onglets, set list, son joué, chaîne, ligne d'écoute | gris, sans gras ni aplat |
| le bloc « ensuite (→) » | gris ; une cue y garde son jaune |
| la note, l'écart et le pointeur : au-delà de ±10 cents | blanc, gras |
| de ±3 à ±10 cents | vert atténué, gras |
| à ±3 cents, et `✓ juste` | vert, gras |
| les flèches `▲ monte` / `▼ descends` | blanc, gras |
| la dernière note d'une corde éteinte, `joue une corde`, les graduations | gris |
| au retour du son : bandeau `♪ SON RENDU` pendant 2 s | aplat vert, texte noir d'encre |

Les trois paliers de l'accordeur sont ceux des gradations de la charte (blanc pour « écart loin »,
la couleur atténuée pour « proche ») ; ils restent verts quel que soit l'espace, pour que « juste »
ait toujours la même couleur sous les yeux. Sur un terminal à couleurs de base, le violet devient
le bleu clair : c'est alors surtout l'écran gris qui dit que la basse est coupée.

**Le gris se pose sur l'image, pas dans les couleurs** : l'écran se dessine comme d'habitude, puis
tout ce qu'il a dessiné passe en gris, puis l'accordeur (avec le bloc « ensuite ») et le bandeau se
dessinent par-dessus. Aucun écran n'a à savoir que la basse est coupée.

## 8. Les messages

| Cas | Message (zone de message) |
|---|---|
| `u`, basse jouée | `u → basse coupée, accordeur` |
| `u`, basse coupée | `u → son rendu` |
| CC 15 ≥ 64 | `CC 15 = 127 → basse coupée, accordeur` |
| CC 15 ≥ 64, déjà coupée | `CC 15 = 127 : la basse est déjà coupée, rien ne change` |
| CC 15 < 64 | `CC 15 = 0 → son rendu` |
| CC 15 < 64, basse jouée | `CC 15 = 0 : la basse n'est pas coupée, rien ne change` |
| un changement de morceau | `PC 3 → 3 · Néon · son rendu`, `→ → 3 · Néon · son rendu` |
| un changement de son | `CC 34 = 1 → son 2 Clean SVT · son rendu` |
| `Start` | `Start → son rendu` |

`namfoop midi` traduit le CC 15 en clair : `accordeur` pour une valeur de 64 ou plus, `son rendu`
en dessous. Sa fenêtre « Régler » le capture sans l'appliquer, comme tout message
([`midi.md`](midi.md) § 5).

## 9. Le clavier

| Touche | Ici | Étage (ADR 0009) |
|---|---|---|
| `u` | entre dans l'accordeur, ou en sort | pédale : agit sous toute fenêtre, et dans `run --set` |
| `→` / `←`, `PgDn` / `PgUp`, `Entrée` sur un morceau | leur geste habituel, qui rend le son | inchangé |
| toute autre touche | son geste habituel ; la basse reste coupée | inchangé |

Barre d'aide pendant l'accordage, sur scène :
`u rendre le son   →/← morceau (rend le son)   PgDn/PgUp son (rend le son)   q quitter`. Ailleurs :
`u rendre le son` suivi de la barre habituelle de l'écran. Hors accordage, `u accordeur` entre
dans la barre de chaque écran.

## 10. La config

```toml
[tuner]
a4 = 440.0       # La de référence, en Hz
key = "u"        # nom crossterm, comme les touches de [set], pour coller à un pédalier qui tape des touches

[midi]
cc_tuner = 15    # ≥ 64 : accordeur ; < 64 : son rendu
```

## 11. Tailles de terminal

Mêmes cibles que `browse` ([`browse-espaces.md`](browse-espaces.md) § 7). L'accordeur tient en 14
lignes et 50 colonnes. Sous 70 colonnes, la set list disparaît déjà ([`set.md`](set.md) § 6) :
l'accordeur prend toute la largeur. Sous 20 lignes de contenu, les grandes lettres laissent place
à la note sur une ligne, en gras (`E♭1`), et le bloc « ensuite » se réduit à son titre.

## 12. Ce que la maquette simule

Une basse et une guitare (Si grave, Mi, La, Ré, Sol, Do aigu, Mi et Mi aigu de guitare) qu'on
gratte : la corde sonne, monte de quelques cents à l'attaque, puis s'éteint ; un curseur règle son
accordage, et le La de référence passe de 432 à 442 Hz. Le MIDI (CC 15, Program Change du morceau
suivant, CC 34, `Start`), un micro, et les niveaux de chaque sortie : façade traitée, DI, in-ears,
micro.

Elle ne simule pas : la Bibliothèque (la Chaîne montre la fenêtre), les tailles de terminal, le
choix d'un set, ni la lecture réelle d'une note.

## 13. Tests attendus

Moteur : basse coupée ⇒ sorties traitées et DI à zéro après 20 ms, micro et clic intacts ; la
queue d'une réverbe présente après la coupure ; aucun saut d'échantillon à la coupure ni au retour ;
un changement de son pendant la coupure. Lecture de hauteur : sur des notes de synthèse riches en
harmoniques, fondamentale affaiblie, du La à 27,5 Hz jusqu'aux aigus d'une guitare. Écran, avec
`TestBackend` / `screen_text` : l'accordeur dans chaque espace et dans `run`, en 80×25 et en
160×50 ; avec la basse coupée, aucune cellule hors de l'accordeur et du bandeau ne porte une
couleur d'espace. L'oracle du clavier : `u` dans chaque contexte. De bout en bout : un port simulé
qui envoie `cc 15 127`, `cc 15 0`, `pc 3` et `start`.

## 14. Les décisions de forme, et leur raison

- **Couper la basse, pas tout.** Le micro sert à parler pendant une scénette, et le clic tient le
  groupe ensemble : couper « tout » couperait la voix au moment où on en a besoin.
- **Un seul mode.** Sur les multi-effets du marché (Fractal, Line 6, Neural DSP), l'accordeur coupe
  le son par défaut : un muet à part ajouterait un geste et un état à surveiller sur scène, pour un
  usage que l'accordeur couvre déjà.
- **La queue finit de sonner.** Ableton envoie l'accordeur sur le dernier temps d'un morceau :
  couper la réverbe avec la basse tronquerait la fin.
- **La colonne du prompteur, pas le plein écran.** La set list et le morceau suivant servent pendant
  une scénette ; l'accordeur en très grand, lisible de loin, relève de l'écran de scène, qui reste à
  concevoir et reprendra celui-ci.
- **Le retour automatique.** Rester muet au début d'un titre est la pire panne de scène ; tout
  changement de morceau ou de son vaut une demande de son.
- **Le violet plutôt que le rouge.** La palette est celle des néons des années 90 : magenta, cyan,
  vert. Le rouge y est une alarme, et une coupure voulue n'est pas une alarme. Le violet de la
  lumière noire est la couleur qui manquait à cette famille, et il ne disait encore rien d'autre.
- **Griser l'écran plutôt que colorer l'accordeur davantage.** Une seule couleur vive à l'écran se
  lit de plus loin que trois couleurs vives côte à côte.
- **Les bémols.** Les accordages descendus d'un demi-ton (Mi♭) sont courants à la basse, et se
  disent en bémols.
