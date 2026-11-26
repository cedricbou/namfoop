# ADR 0013 · Échanger la bibliothèque entre machines par une valise sur une clé USB

**Statut : adopté** (décision de l'utilisateur, au cadrage de l'épique « Emporter ma bibliothèque
d'une machine à l'autre avec une clé USB »). Le parcours (`namfoop export`, `namfoop import`) est
décrit dans le guide, § 6 ; le format et les contrats sont dans le code (`library::exchange`).

## Le problème

La même bibliothèque joue sur trois machines : l'ordinateur de studio, où l'on crée ; le portable
de répétition, où l'on retouche ; le Pi de scène. Un changement fait sur l'une doit se retrouver sur
les deux autres, dans tous les sens, **sans réseau** : en répétition, une clé USB est le seul lien.
Copier le dossier à la main oublie un modèle, ou écrase une retouche faite de l'autre côté sans le
dire.

## La décision

1. **Tout emporter plutôt que choisir.** Un échange où l'on coche ce qui part est un échange où
   l'on oublie un modèle, et l'oubli se découvre en répétition. Ne recopier que ce qui a changé
   rend « tout » aussi court que « un peu ».
2. **Un dossier plutôt qu'une archive.** Une archive se réécrit en entier à chaque export ; un
   dossier ne reçoit que ce qui a changé, se lit sans namfoop, et se joue en secours
   (`--library`).
3. **Une valise au milieu, pas des machines qui se parlent.** Chaque machine ne connaît que la
   valise : trois machines, trois relations, et aucune n'a besoin que les deux autres soient
   allumées.
4. **Le contenu plutôt que la date.** Une machine sans réseau ni pile d'horloge se croit à une
   autre date ; « le plus récent gagne » y remplacerait le bon preset par le mauvais sans rien
   dire. Les dates s'affichent, elles ne décident de rien.
5. **Retenir le dernier échange.** Chaque machine garde, pour chaque valise, l'état commun du
   dernier échange. Sans lui, toute différence serait une question, et une suppression ne se
   distinguerait pas d'un ajout de l'autre côté.
6. **L'un ou l'autre, jamais un mélange.** Un élément changé des deux côtés se tranche : garder
   celui d'ici, ou prendre celui de la valise. Fusionner deux chaînes d'effets donnerait un
   troisième son que personne n'a écouté.
7. **Abandonner n'écrit rien.** Toutes les questions passent avant la première écriture : un import
   à moitié fait est un état que personne ne sait décrire.
8. **Importer avant d'exporter.** Écrire par-dessus une valise qui porte des changements non vus
   perdrait le travail d'une répétition ; le refus coûte une commande, et `--overwrite` reste pour
   le jour où une machine fait foi.
9. **Le même verbe que pour les packs.** `import` fait entrer des sons dans la bibliothèque dans
   les deux cas ; ce qu'on lui donne suffit à savoir lequel.
10. **La bibliothèque voyage, la config reste.** La frontière est un dossier : elle se retient sans
    liste, et `export` comme `import` n'ont rien à savoir de la carte ni du MIDI. Le prix est un
    `setup` et un `namfoop midi` par machine, une fois.
11. **Une carte reste avec sa machine.** Son buffer tient au processeur, le rôle de ses sorties à
    la façon dont elle est câblée là où elle joue.
12. **Une calibration à part de son instrument.** Les calibrations se font carte par carte, donc
    machine par machine : rangées en un seul élément avec leur instrument, elles poseraient une
    question à chaque retour de concert. Il en va de même de la correction de chaque modèle.

## Ce qu'on a écarté

- **git pour les créations, rsync pour les sons** : l'historique est précieux, mais il faut un
  dépôt et un outil de plus sur chaque machine, deux mécanismes pour une même bibliothèque, et les
  conflits de git se règlent dans un fichier texte, pas en choisissant un son.
- **rsync dans les deux sens, ou unison** : rsync ne connaît pas l'état du dernier passage, et
  « le plus récent gagne » suit les dates ; unison retient cet état, mais demande la même version
  installée des deux côtés et des machines qui se joignent.
- **L'échange par le réseau, de machine à machine**, et **l'échange au pied sur le Pi** : hors du
  besoin d'une répétition sans réseau ; ils restent possibles plus tard, par-dessus la même valise.
