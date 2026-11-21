# Style des issues

La backlog se lit **sans ouvrir le code**. Une issue dit ce que le bassiste pourra faire et à quoi
on verra que c'est fait ; le détail technique vit à côté, dans un commentaire. Ces règles valent
pour toute issue créée ou modifiée, par un humain ou par un agent, et `scripts/feature.sh` refuse
celles qui se mécanisent.

## Quatre types, une étiquette chacun

| Étiquette | Ce que c'est | Se code ? |
|---|---|---|
| `epic` | une key feature : elle regroupe des stories et dit où on en est | non, on code ses stories |
| `story` | ce que le bassiste veut pouvoir faire ; une issue, une branche, une PR | oui |
| `chore` | atelier : outillage, CI, documentation, sans effet pour le bassiste | oui |
| `bug` | un comportement incorrect | oui |

Une issue porte **exactement une** de ces étiquettes. `test matériel` s'y ajoute quand un essai sur
la carte reste dû.

## Le titre

Ce que je peux faire, ou le résultat visé, **dans les mots de celui qui joue** : « Entendre un
réglage d'effet au moment où je le tourne », pas « set_param en direct sans rebâtir la chaîne ».
Ni nom de crate en préfixe, ni nom de fonction, ni chemin de fichier. Une commande ou une touche que
le bassiste tape est permise.

## Le corps

**Story**

```markdown
**En tant que** bassiste <situation>, **je veux** <ce que je fais>, **afin de** <le bénéfice>.

## Aujourd'hui
2 à 4 phrases : ce qui gêne, vécu, avec le chiffre parlant s'il existe.

## Terminé quand
- [ ] 3 à 6 critères observables (ce que je vois, entends, fais)
- [ ] l'essai sur la carte, en ALSA, s'il y en a un

## Hors périmètre
## À trancher        (seulement s'il reste une vraie décision)
## Liens             (épique, dépend de, débloque)
```

**Épique** : deux ou trois phrases sur ce que la key feature change, puis `## Où on en est`,
`## Stories` (une case par story, cochée quand elle est livrée), `## Décisions déjà prises` (en mots
simples, avec le lien vers l'ADR ou la spécification), `## Plus tard, peut-être`.

**Chore** : `## Pourquoi`, `## Terminé quand`, `## Liens`.

**Bug** : `## Ce qui se passe`, `## Ce qui devrait se passer`, `## Contexte`.

Les modèles de `.github/ISSUE_TEMPLATE/` portent ces rubriques.

## Ce qui n'entre pas dans le corps

- un nom de crate, de fonction, de type ou de fichier source ;
- la rubrique « Crates touchées », le plan de tests automatiques, les mesures de laboratoire ;
- des consignes à l'agent qui réalise.

Tout cela va dans le **cadrage technique** : un commentaire de l'issue qui commence par
`## Cadrage technique`. C'est lui que lit l'agent avant de coder (crates, ADR, pistes dans le code,
tests, mesures) ; c'est là que les règles d'architecture s'appliquent — une issue touche au plus
deux crates. Une précision qui arrive en cours de route s'ajoute en commentaire, pas dans le corps.

## Écrire

- Première personne (« je »), phrases simples, vocabulaire du musicien.
- Les faits et chiffres parlants se gardent : « chaque appui coupe le son 150 ms ».
- Un critère de « Terminé quand » s'observe sans lire le code. « Le son suit sans coupure » se
  garde ; « la valeur sort de l'identité de la demande » va dans le cadrage.
- Court : une story tient en 15 à 30 lignes.
- Une story se rattache à une épique quand il en existe une pour son sujet, et l'épique gagne une
  case.

## Le parcours d'une issue

1. Écrire le corps dans un fichier, et le cadrage technique dans un autre.
2. `scripts/feature.sh issue story "Titre" corps.md cadrage.md` : contrôle le style, crée l'issue
   avec son étiquette, et poste le cadrage en commentaire.
3. `scripts/feature.sh start <n°>` : relit le style de l'issue avant d'ouvrir la branche.
   `scripts/feature.sh lint <n°>` fait ce contrôle seul.
4. Une limite découverte en chemin devient une issue du même style : une story si le bassiste la
   subit, un chore sinon.
5. Une issue livrée dont il ne reste qu'un essai à l'oreille se ferme, et l'essai devient une case
   de l'issue de session d'essais ouverte.

## Ce que le script refuse

Un type inconnu ou absent ; un titre préfixé d'une crate ou portant un chemin de source ; un corps
sans les rubriques de son type ou sans case à cocher ; un corps qui nomme une crate, un fichier
`.rs`, un chemin `crates/…`, un chemin Rust à `::`, ou qui porte une rubrique « Crates touchées ».
Le reste — un critère observable, un ton lisible — relève de la relecture.
