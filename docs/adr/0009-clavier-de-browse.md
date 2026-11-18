# ADR 0009 — Le clavier de `browse` : un propriétaire par touche

*Décidé le 16/11/2026 (issue #242, épique #240). Statut : adopté.*

## Le problème

`browse` a trois espaces, une dizaine de fenêtres et deux saisies de texte. Rien n'écrivait **à
qui appartient une touche** : l'appartenance n'existait que par **l'ordre** d'une cascade de 14
fonctions sur 9 fichiers, et par des gardes ajoutées à la main, une touche à la fois.

Trois bugs en sont sortis, chacun rustiné séparément :

| | ce qui s'est passé | la rustine de l'époque |
|---|---|---|
| **#175** | le `n` commun mangeait le « n » de `nam` tapé dans le filtre du catalogue | déplacer les saisies avant les communes |
| **#207** | `*` (tap) agissait dans le banc d'essai | une garde **dans** `tap_tempo`, qui rend un message |
| **#238** | `PgDn`/`PgUp` non liées dans l'espace Set retombaient sur le défilement de la Bibliothèque | lier les touches |

Et deux de la même famille, trouvés en mesurant (#241, #242) : depuis la vue scène, `b` **coupait
l'IR du son joué** ; avec la fenêtre `L` ouverte, `n` **effaçait la chaîne derrière la fenêtre**.

S'y ajoute une saturation : **38 caractères liés, 20 des 26 lettres prises**. Réutiliser une lettre
d'un espace à l'autre est la seule issue tenable, et c'est impossible à faire sûrement sans écrire
quelque part qui possède quoi.

## La décision

### Quatre étages, écrits en entier dans `on_key`

Une touche descend les étages, et **le premier qui la possède** la consomme.

| étage | touches | éclipsable par un espace ? |
|---|---|---|
| **les modes** | fenêtres de confirmation, aide, éditeur, **saisies de texte**, fenêtres modales | — (voir ci-dessous) |
| **1 · la pédale** | `1` `2` `3` `q` `h`/`?` `*` `k` | **non** |
| **2 · l'espace** | le clavier de l'espace où l'on est, et de lui seul | — |
| **3 · le commun** | `S` `P` `n` | **oui**, par déclaration |
| **4 · le dernier recours** | `Échap` (« ne quitte pas : q pour quitter ») | — |

L'ordre est la seule chose qui décide : il est donc écrit **en un seul endroit**, en entier, plutôt
que réparti dans les couches.

### Pourquoi la pédale n'est pas éclipsable

Ce n'est pas de l'esthétique, c'est de la fiabilité en concert : se retrouver coincé dans un espace
(`1 2 3`), sans sortie (`q`), sans aide (`h`) ou sans tap tempo (`*`) n'est pas un désagrément, c'est
un risque. `*` et `k` sont des **gestes physiques** — le pied, l'oreille — qui doivent répondre
pareil partout.

Quand une touche de la pédale n'a rien à faire là, elle le **dit** au lieu de ne rien faire. C'est ce
que `tap_tempo` fait depuis #207 (« le tap tempo est dans l'espace Chaîne (2) »), et ce précédent
devient la règle.

### Les saisies de texte ne sont pas une éclipse, mais un mode

Pendant une saisie (nom, filtre de liste, filtre du catalogue, champ de l'éditeur), le clavier veut
dire **des lettres**. Les étages sont alors inatteignables **par construction** — c'est la règle de
#175, et ce n'est pas une exception à l'ordre mais un mode au-dessus de lui. Ces couches restent du
**code** et non des données : elles sont gloutonnes sur `Char(_)` et sélectives sur le reste (le
filtre ne rend `Tab`, `Maj+Tab`, `↑` et `↓`).

### Une modale ne retombe jamais

Tant qu'une fenêtre est ouverte, **elle prend tout**. Une touche qui la traverse agit derrière elle,
sur un écran qu'elle recouvre : c'était le cas de `n`, `P`, `S`, `1`, `2`, `3`, `*`, `k` et `q` avec
la fenêtre `L`, parce qu'elle vivait dans `set_key`, donc **sous** les communes.

Les modales se classent par leur **priorité propre**, pas par l'espace qui les possède : le catalogue
de blocs appartient à la Chaîne mais passe avant tout (#175).

Conséquence pratique : un `?` (`Option`) sur un champ interne à une modale est un **piège**, parce
que son `None` signifie « pas à moi » pour l'appelant. Dans une modale ouverte, tout chemin doit
rendre `Some`.

### Un panneau non plus (complément, #249)

*La règle vaut pour tout ce qui **recouvre** un écran, fenêtre ou non.* Les réglages d'un effet
(`FxFocus::Params`) ne sont pas une modale mais une **colonne** de l'espace Chaîne : ils vivaient
donc *dans* `fx_key`, au-dessus du `match` d'édition, et tout ce qu'ils ne traitaient pas y tombait.
`Retour` et `x` retiraient **le bloc dont on réglait les paramètres** — en laissant le focus sur un
bloc qui n'en a aucun —, `s` le mettait en parallèle, `a` ouvrait le catalogue, `n` vidait la chaîne,
depuis un écran qui montre une liste de valeurs. C'est #241 (agir sur un écran qu'on ne regarde pas)
appliqué à un quasi-modal, et c'est l'oracle qui l'a montré : la table le disait depuis #244, mais
rien ne le faisait échouer, parce que la jointure ne va que dans le sens « déclaré ⇒ possédé »
(#252).

Un panneau se garde donc par une liste **blanche**, comme la scène et pour la même raison : une
liste de refus échoue *ouverte*, et la prochaine touche d'édition agirait de nouveau derrière lui.
**La règle de la liste** : le panneau refuse ce qui **édite** la chaîne, et laisse passer ce qui
**écoute** ou **montre** — le volume du preset, l'A/B sans effets, les vumètres —, parce que c'est
exactement ce qu'on fait en réglant un paramètre.

Avec une exception qui n'en est pas une : **`PgDn` / `PgUp` ne se bloquent jamais.** C'est le geste
du pédalier (#158), et la fiabilité live prime. Le preset remplace la chaîne entière, donc le
panneau n'a plus de sujet : il se referme de lui-même. Le même raisonnement rouvrira la question
pour les modales (#253), où `q` et `*` sont aujourd'hui inertes.

### L'éclipse se déclare

Un espace peut s'approprier une touche commune : il suffit qu'il la consomme à l'étage 2.

Le cas qui vient (#220) : `S` doit enregistrer **le set** dans l'espace Set, et le preset partout
ailleurs. C'est cohérent — `S` enregistre **ce qu'on a devant soi** — et c'est mieux que de brûler
une des 8 lettres libres pour un geste qui a déjà un nom évident. L'espace qui éclipse une touche
l'annonce à sa place dans sa barre du bas.

### Une touche sans propriétaire est retenue, pas affichée

Arrivée au dernier recours sans propriétaire, la touche est posée dans `State::unhandled` ; la boucle
l'écrit dans `NAMFOOP_BROWSE_TRACE` puis la remet à `None`. Sur scène, « j'ai appuyé et rien ne s'est
passé » devient diagnosticable.

**Rien ne s'affiche à l'utilisateur** : on n'apprend pas à ignorer la zone de message, celle où #238
mentait déjà.

### La propriété est statique ; les gardes dynamiques ne font que parler

`tab == Blend`, `Spot::Merge`, `FxFocus::Params`, `songs == 0` ne décident plus *qui reçoit* la
touche — seulement **quoi en dire**. Avant, elles provoquaient une retombée, donc une action
ailleurs ; maintenant la touche reste à son propriétaire, qui explique.

## Ce qui garantit tout ça

**L'oracle du clavier** (`crates/namfoop/src/browse/state/keyboard.rs`) mesure **chaque touche
dans chaque contexte** et fige le résultat dans `keyboard.ref.txt`. Il compare par **(contexte,
touche)** et jamais par ligne. C'est lui qui a permis de faire cette restructuration en sachant
qu'elle ne changeait **que** ce qu'on voulait : l'inversion des communes et des espaces a été prouvée
gratuite (aucun espace ne liait `1 2 3 S P n * k`), et les 8 seuls changements de comportement sont
tous des fuites fermées.

C'est aussi ce qui rend l'ordre des étages vérifiable : lire la cascade ne suffisait pas, et c'est
précisément pourquoi les trois bugs ci-dessus ont vécu.

## Ce qu'on a écarté

- **Un « framework » de raccourcis** (trait `KeyHandler`, `Box<dyn>`, registre, priorités
  numériques) : l'ordre des étages doit rester lisible comme une fonction droite. C'est la seule
  propriété qui a vraiment manqué trois fois.
- **Un enum d'action traversant le dispatch** pour garantir au compilateur que tout ce qui est
  annoncé est traité : l'exhaustivité est séduisante mais se paie en plomberie dans le fichier le
  plus délicat du binaire, pour une garantie que la jointure avec l'oracle donne presque
  gratuitement (#243).
- **Un type `Taken { By, Fall, Restart }`** : les trois issues existent déjà (`Some`, `None`, et
  l'appel récursif d'une saisie qui se termine) et sont maintenant **nommées dans la
  documentation** ; en faire un type aurait touché neuf signatures pour ne renommer que du correct.
- **Des modes à la Vim**, préfixes, accords, *leader key* : sur scène, un geste = une touche. Une
  séquence à deux touches qui part à moitié est un danger, pas une ergonomie.
- **Un clavier configurable.** Deux touches le sont (`[set] next_song` / `prev_song`) parce qu'un
  footswitch les envoie. Une troisième ? une entrée nommée de plus, pas un système.
- **Unifier avec `setup`** : son `on_key` est un assistant linéaire plat qui n'a pas ce problème.
