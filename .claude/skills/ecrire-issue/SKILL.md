---
name: ecrire-issue
description: Créer ou réécrire une issue GitHub de namfoop dans le style de la backlog (épique, story, chore, bug), lisible par un non-technicien, avec le cadrage technique en commentaire. Utiliser dès qu'il faut ouvrir une issue — une fonctionnalité demandée, une limite découverte en codant, un bug, une dette — ou reformuler une issue existante.
---

# Écrire une issue

La backlog se lit sans ouvrir le code. La référence est [`docs/style-issues.md`](../../../docs/style-issues.md) :
la lire en entier avant d'écrire.

## Méthode

1. **Choisir le type** : `story` si le bassiste voit ou entend le résultat, `chore` sinon, `bug`
   pour un comportement incorrect, `epic` seulement pour une key feature que l'utilisateur a nommée.
2. **Chercher l'épique** : `gh issue list --label epic`. Une story s'y rattache dans `## Liens`.
3. **Écrire deux fichiers** hors du dépôt (`/tmp`) :
   - le **corps**, aux rubriques du type, à la première personne, sans nom de crate, de fonction ni
     de fichier source ; les critères de « Terminé quand » s'observent en jouant ;
   - le **cadrage technique** : crates touchées (deux au plus), ADR, pistes dans le code, tests,
     mesures, essais matériels. Tout ce qu'un agent doit savoir avant de coder.
4. **Créer** : `scripts/feature.sh issue <type> "Titre" corps.md cadrage.md`. Le script contrôle le
   style et refuse ce qui s'en écarte : corriger le texte, ne pas contourner.
5. **Mettre à jour l'épique** : une case de plus dans `## Stories`.

## Réécrire une issue existante

Le corps d'origine ne se perd pas : le poster d'abord en commentaire `## Cadrage technique`, puis
remplacer le corps et le titre, puis `scripts/feature.sh lint <n°>`.

## Le test de relecture

Quelqu'un qui joue de la basse et ne lit pas le Rust comprend-il le titre, sait-il pourquoi c'est
gênant, et saura-t-il dire si c'est fait ? Si une phrase demande d'ouvrir le code, elle va dans le
cadrage.
