---
name: relire-docs
description: Relire la documentation d'une zone de namfoop (un fichier ou un répertoire) selon docs/style-doc.md, et proposer un diff. Utiliser quand on veut nettoyer ou vérifier des CLAUDE.md, des commentaires, docs/, les ADR ou les spécifications d'écran — par exemple à chaque publication de version, ou avant de travailler dans une zone dont la documentation semble douteuse.
---

# Relire la documentation d'une zone

Les contrôles de `scripts/check-docs.sh` (étapes 9 à 11) refusent au commit ce qui s'écrit en
expression régulière. **Cette relecture attrape le reste** : un récit qui ne contient aucun
mot-signal, une mesure qui s'empile proprement, un fait devenu faux sans que son chemin change, une
duplication entre deux fichiers.

Argument : un chemin (`crates/dsp`, `docs/design`, `crates/engine/CLAUDE.md`). Sans argument,
demander la zone plutôt que relire tout le dépôt.

## Ce que tu produis

Un **diff proposé** et rien d'autre : pas de commit, pas de PR. Pour chaque proposition, une ligne
qui dit la règle invoquée et, s'il s'agit d'un fait, **comment tu l'as vérifié**. Si tu ne peux pas
vérifier un fait, tu le signales au lieu de le retirer ou de le garder en silence.

Termine par un compte : lignes proposées en retrait, faits vérifiés, faits non vérifiables.

## La marche à suivre

1. **Lire [`docs/style-doc.md`](../../../docs/style-doc.md)** en entier — ses onze règles, ses
   exemples, et surtout sa section « Ce qui se garde, explicitement ». C'est la référence ; ce
   fichier-ci n'en est que le mode d'emploi.
2. **Lire la zone**, et le `CLAUDE.md` de la crate si la zone est dans une crate.
3. **Passer les règles mécanisées d'abord** : `bash scripts/check-docs.sh`. Ce qu'il signale est
   déjà du travail identifié, et il bloque — commence par là.
4. **Puis chercher les quatre défauts qu'aucun `grep` ne voit** (règles 3, 4, 6, 7, 10, 11) :
   - **la duplication** : un paragraphe qui existe aussi ailleurs. Vérifie-le
     (`grep -rF "<début de phrase>" crates docs *.md`) ; le garder à l'endroit qui le porte le
     mieux, et renvoyer depuis l'autre ;
   - **la mesure empilée** : une colonne « avant / après », une alternative « essayée et écartée »,
     deux valeurs pour la même grandeur. Garder la valeur **actuelle**, ses conditions de
     reproduction et sa conséquence ;
   - **le récit** : un paragraphe qui raconte un bug, une PR, une migration, à la première personne
     ou non. Le contrat qu'il cache se formule **comme contrat** (voir la règle 6) ;
   - **l'état d'avancement** : « où en est-on », « à venir », « prototype », « pas encore », un
     tableau d'issues. Ce rôle est au `CHANGELOG.md`, au `README.md` et aux issues `roadmap`.
5. **Vérifier chaque fait que tu gardes.** C'est le cœur du travail, et ce qui le distingue d'une
   relecture de style :
   - un **chemin** ou un **symbole** cité : `git ls-files | grep <nom>`, `grep -rn "<symbole>" crates/` ;
   - un **nombre** tiré du code (un seuil, une constante, un compte de touches) : le relire dans le
     code. S'il y est engendré (`docs/cli.md`, `keyboard.ref.txt`, `budget.toml`), remplacer le
     nombre par un renvoi — c'est la règle 11 ;
   - une **mesure** (µs, ms, dB, Mio) : vérifier que la commande citée existe encore et que la
     valeur est celle de la version en cours. Une mesure qu'on ne peut pas rejouer se garde avec ses
     conditions, et se signale comme non revérifiée ;
   - une **règle annoncée** (« interdit X ») : trouver ce qui la vérifie (`scripts/check-arch.sh`,
     un test). Si rien ne la vérifie, la règle doit le dire (règle 9).
6. **Relire ta propre proposition contre la liste de ce qui se garde** (ci-dessous), puis présenter
   le diff.

## Ce qui se garde — ne pas y toucher

Sans cette liste, un nettoyage zélé jette le bon avec le bruit :

- les en-têtes `//!` de `dsp` et `audio-io`, et les `///` d'API d'`engine` : ils donnent les unités,
  les valeurs par défaut, les cas `None`, les garanties temps réel ;
- les **règles temps réel** de `crates/dsp/CLAUDE.md` et la procédure « Ajouter un effet » ;
- les **tables de mesures actuelles** : budgets par bloc, latences, RT60, seuils, tolérances ;
- le câblage exigé par `scripts/test-backends.sh` : une information irremplaçable ;
- les renvois **`ADR 000x`** : ils pointent vers un document vivant (et ne portent pas de `#`) ;
- une justification **durable** avec sa règle, comme « au-delà de ~600 lignes, un agent lit en
  morceaux et rate des invariants » : la raison ne vieillit pas ;
- le vocabulaire du domaine qui ressemble à un mot-signal : « l'**ancienne** chaîne » d'un fondu, un
  son qui « ne figure **plus** » dans la bibliothèque.

## Les genres exemptés, et pourquoi

Ne rien proposer sur : `CHANGELOG.md` (il **est** de l'histoire), `docs/adr/` (un ADR dit ce qui
motive sa décision et se supersède au lieu de se réécrire), `docs/cli.md` et
`crates/namfoop/src/browse/state/keyboard.ref.txt` (engendrés), `docs/style-doc.md` (il doit nommer
les mots qu'il proscrit), et les maquettes `docs/design/*.html`.

## Vérifier avant de rendre

- `bash scripts/check-docs.sh` passe (il bloque sur quatre règles).
- Si la zone contient du `.rs`, `cargo test -p <crate>` passe et
  `git diff -U0 | grep '^[-+]' | grep -v '^[-+][-+]'` ne montre **que** des lignes de commentaire :
  une relecture de documentation ne change aucun comportement.
- Sur une zone déjà relue, tu ne dois (presque) rien proposer. Si tu proposes beaucoup, dis-le :
  c'est soit la zone qui n'a jamais été traitée, soit ta lecture qui est trop zélée.
