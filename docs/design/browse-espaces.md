# Spécification · les espaces de `browse`, et l'espace Bibliothèque

**Ce que couvre ce fichier** : la navigation entre les espaces de `browse`, l'espace **Bibliothèque**
(le banc d'essai), le passage de la Bibliothèque vers la chaîne, le **sélecteur sur place** d'un bloc
`nam` / `ir`, les tailles de terminal et l'aide à l'écran. L'espace **Chaîne** est spécifié dans
[`browse-chaine.md`](browse-chaine.md), l'espace **Set** dans [`set.md`](set.md), et l'ordre des
étages du clavier dans l'[ADR 0009](../adr/0009-clavier-de-browse.md).

Ce qui **fait foi** pour le comportement : le code et l'oracle du clavier
(`crates/namfoop/src/browse/state/keyboard.ref.txt`), qui sont vérifiés à chaque commit. La maquette
[`browse-espaces.html`](browse-espaces.html) est une **illustration** validée avec l'utilisateur : à
ouvrir dans un navigateur pour voir l'écran et ses tailles, sans en reprendre le JavaScript. Les noms
de modèles et d'IR qu'elle montre sont fictifs, et elle ne dessine ni les vumètres, ni les réglages
d'effets, ni l'éditeur `e`.

**Couleurs** : « magenta », « cyan », « vert », « jaune », « rouge » et « gris » désignent ici le
magenta électrique, le cyan néon, le vert signal, le jaune ambre, le rouge clip et le gris acier de la
[charte](../identite/charte.md), qui dit aussi quand le terminal les remplace par ses couleurs de base.

## 1. Le besoin et les trois règles

Le besoin, dans les mots de l'utilisateur : « quand je suis sur les NAM / IR, j'ai besoin de les
écouter et de les travailler sans les effets ; quand je suis dans les effets, c'est perturbant de
retourner sur cet écran ; et quand j'y reviens et que je change, ça change mes effets. » Un même geste
servait alors à deux activités : explorer la bibliothèque, et remplir un bloc de la chaîne.

1. **Parcourir n'écrit jamais dans la chaîne.** Écrire est un geste explicite (Entrée).
2. **On sait toujours ce qu'on écoute** : le banc d'essai ou la chaîne.
3. **On ne quitte pas la chaîne pour remplir un bloc** : le choix se fait sur place.

L'utilisateur mène un travail entier dans un espace ; il les fait se rencontrer à l'occasion (pousser un
son de la Bibliothèque vers la chaîne, choisir un modèle depuis la Chaîne).

## 2. Les trois espaces

| | **Bibliothèque** | **Chaîne** | **Set** |
|---|---|---|---|
| Contenu | onglets Modèles, IR, Mélange | synoptique, éditeur, réglages | la vue scène ([`set.md`](set.md)) |
| Ce qu'on entend | le **banc d'essai** : basse › modèle › IR, sans aucun effet | la **chaîne** éditée | le son du morceau joué |
| Couleur d'accent | magenta | cyan | — |

- **`1`** va à la Bibliothèque, **`2`** à la Chaîne, **`3`** à l'espace Set, **depuis n'importe
  où** sauf pendant une saisie (filtre, nom), où une lettre est une lettre. Devant une **fenêtre**
  (la banque, « mettre dans la chaîne », le mode pose, le sélecteur, le choix du set), la touche
  **referme la fenêtre** — avec l'annulation que cette fenêtre connaît, comme `Échap` — puis change
  d'espace : elle n'agit donc jamais *derrière* elle. C'est la règle de l'[ADR
  0009](../adr/0009-clavier-de-browse.md) : les gestes du pied et de l'oreille (`1 2 3`, `q`, `*`,
  `k`, `h`) répondent **pareil partout**, parce que rester coincé est un risque en concert.
- Dans la Bibliothèque, **Tab / Maj+Tab** (en boucle) et **→ / ←** (de proche en proche) passent entre
  Modèles, IR et Mélange. Ils ne mènent plus à la Chaîne ; → sur Mélange le dit :
  `→ ne va pas plus loin : 2 pour la Chaîne`.
- Dans la Chaîne, Tab ouvre les réglages de l'élément sous le curseur.
- **L'écoute suit l'espace.** Passer de l'un à l'autre change ce qu'on entend, avec le fondu habituel.
  Chaque espace garde son état : le curseur et le choix du banc d'un côté, la chaîne et son curseur de
  l'autre, le morceau visé du troisième. **Les espaces sont indépendants** : une touche d'un espace
  n'agit jamais sur l'objet d'un autre.
- Au lancement, `browse` ouvre la Bibliothèque (`import --review` aussi). Si la chaîne de départ contient
  un bloc `nam` (et un `ir`), le banc démarre sur ce modèle et cette IR.

En-tête, première ligne :

```
 namfoop browse  [1 Bibliothèque]  2 Chaîne ● 5 blocs  3 Set            <état d'enregistrement>
```

L'espace actif est en inverse, de sa couleur. `● N blocs` reste visible depuis la Bibliothèque. À
droite, l'indicateur d'enregistrement (`preset : <nom> ✓` / `● non enregistré (S)`).

Le cadre principal porte un **bandeau** dans son titre, de la couleur de l'espace :

- `♪ BANC D'ESSAI · basse › modèle › IR · sans effets, la chaîne n'est pas touchée`
- `♪ CHAÎNE — NAM actifs : N · parallèles : P` (le titre actuel, précédé de `♪ CHAÎNE`)

Sous le cadre, la **ligne d'écoute** dit ce qu'on entend, en clair :

```
 ♪ banc : basse › SVT Classic · clean › IR 8x10 SM57
   chaîne (non écoutée) : gate -60 › ◆[ direct | nam·B7K Ultra › ir·8x10 SM57 ] › peak › reverb
```

```
 ♪ chaîne : basse › gate -60 › ◆[ direct | nam·B7K Ultra › ir·8x10 SM57 ] › peak › reverb
```

En bas de l'écran, les vumètres, `lim`, `dsp` et `xruns` valent pour tous les espaces.

## 3. Bibliothèque : le banc d'essai

Les onglets Modèles, IR et Mélange parcourent la bibliothèque. Ce que l'espace garantit (la liste de
ce qui ne doit pas bouger est au § 8) :

- **↑↓ écoute, et rien d'autre.** Le modèle et l'IR du banc (`State::model`, `State::ir`) ne sont pas
  recopiés dans la chaîne. L'IR conseillée posée d'office pour un modèle sans baffle, et l'IR retirée
  pour une capture qui inclut le sien, ne concernent que le banc.
- **On entend le banc à nu** : aucun bloc de la chaîne, pas même le gate. `c`, `r`, `b`, l'éditeur `e`
  (écoute du gain, grain remesuré) et le Mélange s'écoutent donc sans effets.
- **Marque `●` verte** devant un modèle ou une IR que la chaîne utilise (dans la colonne de `◆` et `✎`).
  C'est un rappel ; le curseur n'y touche pas.
- `◀ écouté` (modèle) / `◀ écoutée` (IR) marque ce que joue le banc, à la place de `◀ en cours`.
- **Entrée** : mettre dans la chaîne (§ 4).
- **Panneau Bandes** (`z`) : un égaliseur graphique des sept zones de
  `library::profile::ZONES`, qui dit ce que la chaîne **écoutée** (modèle nu + IR du moment) fait à
  la basse. Mesure **figée** (profils en cache, aucune reconstruction de chaîne, rien dans le
  thread audio). Écart avec la référence `◆` quand il y en a une, titre débarrassé du début commun
  des deux noms ; `r` retourne la comparaison.

  ```
  ┌ Bandes — ce que la chaîne fait à ta basse ─┐
  │ +12 ┤                                      │
  │  +6 ┤                      ██  ██          │
  │   0 ┼──────────────██──██──██──██──        │
  │  -6 ┤  ██  ██  ██                          │
  │ -12 ┤  ▼▼  ██                              │
  │      sub  gr  bm  md  hm  pr air           │
  │      -26 -19  -9  -3  +3 +11 +19   dB      │
  └────────────────────────────────────────────┘
  ```

  Échelle ±12 dB sur 5 lignes (6 dB par ligne, 3 dB par demi-bloc), `▲` / `▼` hors échelle, couleurs
  des vumètres (vert ±3, jaune ±8, rouge au-delà). Le zéro est la moyenne 80 Hz – 5 kHz de la
  chaîne : les colonnes montrent sa **couleur**, pas son niveau. 9 lignes, 36 colonnes.
- Barre d'aide (Modèles) :
  `↑↓ écouter  Entrée mettre dans la chaîne  Tab/→ IR  e calibrer  c épingler réf.  r A/B  b avec/sans IR  / filtrer  2 Chaîne  q`

## 4. De la Bibliothèque vers la chaîne (Entrée)

Entrée envoie ce que joue le banc : le modèle (onglet Modèles), l'IR (onglet IR) ou le mélange (onglet
Mélange ; s'il n'est pas sauvé, il l'est d'abord, sous le nom proposé).

- `son direct` ou `aucune IR` : rien à envoyer, un message le dit.
- **Chaîne vide** : elle devient **ce qu'on entend au banc**, depuis n'importe lequel des trois
  onglets — `nam › ir`, `nam` seul pour une capture avec baffle (ou si `b` coupe l'IR), `ir` seule
  quand le banc ne joue pas de modèle —, sans fenêtre. Message : `chaîne créée : nam·B7K Ultra ›
  ir·8x10 SM57 · 2 pour l'habiller d'effets`. C'est le parcours « j'ouvre `browse`, je choisis un
  son » : on choisit un modèle (onglet Modèles), puis son baffle (onglet IR), et Entrée installe les
  **deux**, quel que soit l'onglet où l'on se trouve — sans quoi le modèle du banc serait perdu en
  envoyant depuis l'onglet IR.

  La **fenêtre**, elle, est par type de bloc : depuis l'onglet IR elle propose les blocs `ir`.
- **Sinon, toujours une fenêtre**, même s'il n'y a qu'un bloc du bon type :

```
┌ Mettre « SVT Classic · clean » dans la chaîne ─────────────────────────┐
│ ▶ remplacer  nam·Fuzz Face (Fuzz Face · bass) (branche fuzz)           │
│   remplacer  nam·SVT Classic (SVT Classic · clean) (branche ampli)     │
│   ajouter un bloc nam…  (tu choisis l'endroit dans la chaîne)          │
│                                                                        │
│  [x] avec son IR (8x10 SM57) dans le bloc ir voisin                    │
│                                                                        │
│  Entrée valider · Espace cocher · Échap annuler                        │
└────────────────────────────────────────────────────────────────────────┘
```

  - une ligne `remplacer` par bloc `nam` (ou `ir`) de la chaîne, dans l'ordre du synoptique, avec le nom
    complet du modèle en place et sa branche ; pour un `ir`, `(après nam·…)` s'il suit un modèle ;
  - `ajouter un bloc…` en dernier ;
  - la case **« avec son IR »** n'existe que pour un modèle qui a besoin d'une IR, quand le banc en joue
    une (et qu'elle n'est pas coupée par `b`). Cochée par défaut. Cochée : l'IR du banc va dans le bloc
    `ir` qui suit immédiatement le `nam` visé ; s'il n'y en a pas, un bloc `ir` est ajouté juste après ;
  - ↑↓ choisit, Espace coche, Entrée valide, Échap annule (`rien n'est changé dans la chaîne`).
- **Remplacer** : on reste dans la Bibliothèque. Message :
  `nam·B7K Ultra (branche > 150 Hz) remplacé par nam·SVT Classic › ir·8x10 SM57 · 2 pour voir la chaîne`.
- **Ajouter** : on passe dans la Chaîne en **mode pose**. Titre du cadre : `♪ CHAÎNE · où poser
  nam·SVT Classic › ir·8x10 SM57 ?`. ←→↑↓ déplacent le curseur comme d'habitude ; **Entrée pose après le
  curseur** (mêmes règles que `a` dans l'éditeur) ; Échap annule et ramène à la Bibliothèque.
  Après la pose on reste dans la Chaîne : `… posé (branche …) · [ ] pour le déplacer`.

Aucun bloc n'est jamais créé implicitement « à la fin de la chaîne » : l'endroit est toujours dit.

## 5. Chaîne : choisir un modèle ou une IR sur place

### Panneau sous le synoptique

Quand le curseur est sur un bloc `nam` ou `ir`, le panneau des réglages montre son contenu en entier, ce
que l'étiquette tronquée du synoptique ne peut pas faire :

```
┌ nam (branche fuzz) · modèle ──────────────────────────────┐
│ Fuzz Face · bass   [Pédales]                              │
│ f7aa  IR requise  ▁▄▇▇▆  saturé 22 %                      │
│ Entrée : changer de modèle, essayé dans la chaîne         │
└───────────────────────────────────────────────────────────┘
```

### Sélecteur

**Entrée** sur un bloc `nam` ou `ir` ouvre la liste **dans ce panneau** (cadre jaune). On ne change ni
d'espace ni d'écran, le synoptique reste visible au-dessus.

```
┌ nam (branche fuzz) · choisir le modèle ─────────────────────────────────────────────┐
│ / pour filtrer                                                                      │
│   k2pf   SVT Classic · grit      IR requise ▃▆▇▆▃ crunch 9 %  [Ampeg]               │
│   f7aa   Fuzz Face · bass        IR requise ▁▄▇▇▆ saturé 22 % [Pédales]  ● actuel   │
│ ▶ p9ab   Bassman 800 · poussé    IR incluse ▂▄▇▆▂ crunch 5 %  [Tone3000] ◀ à l'essai│
└─────────────────────────────────────────────────────────────────────────────────────┘
 ♪ chaîne : basse › hpf 30 › comp 4:1 › ◆[ … | nam·Bassman 800 › ir·4x10 SM57 › hpf 150 | … ] › gate -50
   à l'essai : rien n'est gardé tant que tu ne valides pas (Entrée) · c pour réentendre l'actuel
```

- La liste est celle de la Bibliothèque : mêmes lignes (mini-spectre, grain, étiquettes, pack), même
  filtre `/`. Pour un `ir` : les **mélanges enregistrés en tête**, puis les IR, avec `★` sur celles
  conseillées pour le modèle voisin. Pas de ligne « son direct » ni « aucune IR » : on retire le bloc
  avec `x`. Le curseur s'ouvre sur l'élément actuel.
- **↑↓ essaie dans la chaîne**, avec ses effets, après le délai d'écoute habituel. Le synoptique montre
  l'essai. `● actuel` marque ce qui était là, `◀ à l'essai` ce qu'on entend.
- **Entrée garde** ce qu'on entend ; **Échap** remet l'actuel (`retour à « Fuzz Face · bass »`).
- **`c`** bascule l'écoute entre l'actuel et l'essai, sans bouger le curseur : c'est l'A/B en contexte.
  `● actuel ◀ écouté (c)` quand on réentend l'actuel. Sans essai en cours, un message explique. Entrée
  pendant qu'on écoute l'actuel garde l'actuel.
- **`o`** isole : on entend `basse › nam › ir`, le bloc et son voisin immédiat (l'`ir` qui suit un
  `nam`, le `nam` qui précède un `ir`), comme au banc d'essai. Le reste du synoptique est assombri, le
  titre dit `ÉCOUTE ISOLÉE (o)`. `o` de nouveau revient à la chaîne. L'isolement prend fin avec le
  sélecteur.
- **`r`** : avec / sans effets, comme dans la Chaîne.
- **Panneau Spectre** à droite de la liste quand le terminal est assez large (même seuil que dans la
  Bibliothèque) : courbe de l'élément sous le curseur, et l'actuel comme référence (écarts par zone,
  grain).
- **Un essai n'est pas une modification** : tant qu'on n'a pas validé, l'état comparé par
  `unsaved()` (et l'indicateur de l'en-tête) est celui d'avant. Les touches qui ne sont ni de la
  pédale ni du sélecteur sont refusées pendant un choix (`valide (Entrée) ou annule (Échap) le choix
  d'abord`) ; `q`, `1`, `2`, `3`, `*` et `k`, elles, **referment** le sélecteur — l'essai est donc
  annulé, comme avec `Échap` — puis agissent (§ 2).
- Barre d'aide :
  `↑↓ essayer dans la chaîne  Entrée garder  Échap revenir à l'ancien  c actuel / essai  o écouter ce bloc seul  r sans effets  / filtrer`

### Catalogue (`a` / `i`)

Choisir `nam` ou `ir` dans le catalogue pose le bloc (`nam·(à choisir)`) et **ouvre aussitôt le
sélecteur**. Échap retire le bloc (`rien n'est ajouté`, barre d'aide : `Échap ne rien ajouter`). Il n'y a
pas d'actuel : `c` ne fait rien.

## 6. Ce qu'on entend, espace par espace

Aucun onglet ne « vise » de bloc de la chaîne, et Entrée sur un bloc `nam` / `ir` n'ouvre pas un
onglet : le sélecteur s'ouvre sur place (§ 5). La Chaîne est un espace, pas un quatrième onglet.

`plan()` est la seule réponse à « qu'est-ce qui joue » : dans la Bibliothèque, la chaîne `nam › ir` du
banc, avec l'A/B `r` / `c` / `b` appliqué (`effective_model` / `effective_ir`) ; dans la Chaîne, les
blocs édités, avec l'essai du sélecteur, la bascule `c`, l'isolement `o` et `r` (sans effets).

## 7. Tailles de terminal

L'écran cible sur le Pi 5 est un 10,1 pouces 1280×800 : 160×50 en police 8×16, 128×40 en 10×20, 106×33
en 12×24, 80×25 en 16×32. Le sélecteur étant contextuel, il doit rester utilisable en 80×25 : la liste
prend la hauteur qui reste sous le synoptique, **4 lignes au moins**, et défile. Le panneau Spectre
demande **128** colonnes (`SPECTRUM_MIN_WIDTH`), la largeur de la police 10×20 retenue sur le Pi.

Le **panneau Bandes** (§ 3) suit la même limite : à partir de 128 colonnes il se glisse **sous la
courbe du Spectre**, dans la colonne de droite, donc il ne coûte aucune ligne de liste et il est
affiché d'office (il remplace la ligne de texte des écarts par zone). En dessous de 128 colonnes il
n'y a pas de colonne de droite : il s'empile sous la liste, masqué par défaut, montré par `z` — et
seulement si le corps garde de quoi lire la liste (en 80×25 : 5 lignes de liste et le panneau).

## 8. Non-régression : ce qui ne doit pas bouger

Dans la Bibliothèque : listes, filtre `/` et `--filter`, mini-spectres, grain, étiquettes, badges IR
incluse / requise, `✎`, IR conseillées `★`, panneau Spectre (courbe, mélange, écarts par zone, ligne
grain), référence `c`, bascule `r`, `b` avec / sans IR, indicateur A/B, éditeur `e` (et `a` appliquer au
filtre), onglet Mélange (`espace`, `+` `-`, `p`, `a`, `x`, `s`), niveau « match » à chaque bascule.

Dans la Chaîne : tout [`browse-chaine.md`](browse-chaine.md) — synoptique, vumètres, navigation 2D,
`a` `i` `s` `+` `x` `[ ]` `{ }` `b` `m` `o` `p` `v`, réglages (Entrée et Tab), `r`, Échap qui ne quitte
jamais, `q` avec confirmation, et la ligne de commande équivalente, qui décrit la **chaîne** et non le
banc d'essai.

Le parcours nominal du guide — ouvrir `browse`, choisir un modèle et une IR, ajouter des effets,
quitter avec la commande `run` — passe par Entrée.

## 9. Deux règles qui ne se voient pas sur la maquette

- **Quitter avec une chaîne vide alors que le banc joue un son** : `q` le signale toujours, même s'il n'y
  a rien d'autre à enregistrer, et propose les options dans la fenêtre de confirmation :

  ```
  ┌ La chaîne est vide ─────────────────────────────────────────────────────┐
  │ Tu écoutes au banc SVT Classic · clean › 8x10 SM57, mais ce son n'est   │
  │ pas dans la chaîne : la commande affichée en quittant ne le jouera pas. │
  │                                                                         │
  │  Entrée  le mettre dans la chaîne et rester (2 pour la voir)            │
  │  Q       quitter quand même                                             │
  │  Échap   rester (par défaut)                                            │
  └─────────────────────────────────────────────────────────────────────────┘
  ```

  Entrée fait ce que fait Entrée dans la Bibliothèque sur une chaîne vide (§ 4). Si d'autres choses ne
  sont pas enregistrées, elles sont listées dessous. Banc sur `son direct` sans IR : rien à signaler.
- **`e` n'est pas accessible depuis le sélecteur de la Chaîne** : calibrer est réservé à la
  Bibliothèque, et `e` y répond `calibrer (e) se fait dans la Bibliothèque (1), sur le modèle`.
  **Aucune touche du sélecteur n'est muette** : celles qu'il ne traite pas répondent `valide (Entrée)
  ou annule (Échap) le choix d'abord`. Une touche qui ne fait rien **sans le dire** rend l'écran
  incompréhensible.

## 10. Aide à l'écran

Deux niveaux, parce que la barre du bas ne peut pas tout dire :

- **la barre permanente** (deux lignes, en bas) garde le contexte **et les touches de sortie** :
  elle finit toujours par `h aide`, les autres espaces (`1 Bibliothèque` / `2 Chaîne` / `3 Set`) et
  `q quitter`.
  Elle est coupée aux **séparateurs de touches**, jamais au milieu d'un mot ; si le terminal est
  trop étroit, les touches du milieu sautent et un `…` le signale — les trois dernières restent ;
- **l'overlay `h`** (ou `?`) donne **toutes** les touches de l'écran où l'on est : Bibliothèque
  (selon l'onglet), Chaîne (synoptique ou réglages), sélecteur, catalogue, fenêtre « mettre dans la
  chaîne », mode pose, éditeur `e`. N'importe quelle touche le referme ; pendant une saisie (filtre,
  nom, catalogue) `h` s'écrit. Il se dessine par-dessus tout l'écran, donc il tient en 80×25.

## 11. Tests attendus

Logique dans `browse::state` (pure, horloge fournie) : chaque règle des § 2 à 6, dont « ↑↓ dans la
Bibliothèque laisse la chaîne identique », « Échap du sélecteur rend la chaîne identique », « un essai
n'est pas dans `unsaved()` ». Rendu avec `TestBackend` / `screen_text` : en-tête, bandeaux, ligne
d'écoute, fenêtre « mettre dans la chaîne », sélecteur en 80×25 et en 160×50. De bout en bout :
pseudo-terminal, `--no-config --backend sim`, bibliothèque jetable, `NAMFOOP_BROWSE_TRACE` pour vérifier
la chaîne réellement installée (banc à nu dans la Bibliothèque, essai puis retour dans la Chaîne).
