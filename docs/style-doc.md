# Style de la documentation et des commentaires

Commentaires, rustdoc, `CLAUDE.md`, `docs/`. Le projet documente bien ses contrats et mal son
passé : ces règles visent le second sans abîmer le premier. `scripts/check-docs.sh` vérifie ce qui
se mécanise.

## Le principe

**Un fait va à l'endroit qui le porte le mieux**, et du texte n'est le meilleur endroit que par
défaut :

| Le fait… | …vit dans |
|---|---|
| se vérifie | ce qui le vérifie (un test, un contrôle de `check-*.sh`) |
| s'exécute | ce qui l'exécute (un script de `scripts/`) |
| change avec le code | ce qui l'engendre (`docs/cli.md`, `keyboard.ref.txt`, `budget.toml`) |
| est une décision | un ADR de `docs/adr/` |
| est arrivé | `git log` et `CHANGELOG.md` |
| est un contrat, un piège, un invariant | le code, en commentaire ou en rustdoc |

Devant une phrase, la question n'est donc pas « est-ce de l'histoire ? » — on y répond mal — mais
**« existe-t-il un endroit qui porterait ce fait mieux que du texte ? »**

## Les règles

**1. Le présent de l'état actuel.** Proscrits : « désormais », « auparavant », « l'ancien X »,
« avant #N », « depuis #N », « ne … plus », « c'était », « a été retiré », « ont disparu ». Un
ancien texte faux se **supprime** ; vrai, il se réécrit au présent.
Modèle : `crates/dsp/src/lib.rs` (les règles temps réel du trait `Stage`).
Deux de ces formules sont du vocabulaire du domaine ici — « l'**ancienne** chaîne » d'un fondu, « un
son qui **ne** figure **plus** dans la bibliothèque » — donc hors du contrôle automatique et à la
charge du relecteur.

**2. Pas de numéro d'issue ni de PR dans le code ni dans un document de référence.** L'historique
est dans git et le `CHANGELOG.md`. **Exception** : les ADR, dont le genre *est* la trace d'une
décision — et les renvois `ADR 000x` du code sont de **bonnes** références, ils pointent vers un
document vivant. À garder.

**3. Un fait, un endroit.** Un `CLAUDE.md` de crate ne reprend jamais le racine : il y renvoie.
« Les 52 touches » était faux en trois exemplaires **parce qu'il était recopié**.

**4. Une mesure remplace la précédente, elle ne s'empile pas.** Se garde : la valeur, les conditions
pour la reproduire, la conséquence. Part : les colonnes « avant / après », les alternatives
« essayé, retiré », « mesurée et écartée » — vers la section *Alternatives* de l'ADR concerné si la
rouvrir est plausible, sinon nulle part.

**5. La règle, pas son erratum.** Dans `docs/design/`, corriger la phrase plutôt que l'annoter :
zéro « Corrigé le JJ/MM/AAAA », zéro proposition abandonnée conservée au-dessus de la décision qui
la remplace.

**6. Pas de récit à la première personne, pas de journal de bug.** Le contrat qu'un bug a révélé se
formule **comme contrat** : « la garde est une liste blanche : une liste de refus échouerait
ouverte » se garde ; « `Retour` et `x` retiraient le bloc qu'on réglait » se jette.

**7. Pas d'état d'avancement dans un document de référence** : ni « où en est-on », ni colonne
« depuis #n », ni « prototype », ni liste « pas encore / à venir ». Ce rôle est au `CHANGELOG.md`,
au `README.md` et aux issues `epic`.

**8. Ce qui mérite un commentaire** : un invariant, un contrat, une unité ou une plage, un piège non
évident, une règle temps réel, une contrainte extérieure (le format ChordPro, ITU-R BS.1770, un
comportement d'ALSA). **Jamais la paraphrase de la ligne suivante.**

**9. Une règle écrite est soit vérifiée, soit annoncée comme non vérifiée.** Une règle **fausse
coûte plus qu'une règle absente** : elle est lue comme vraie et oriente le travail de travers.
`docs/architecture.md` a déclaré pendant des mois que « `dsp` interdit la config » alors que `dsp`
lit et écrit ses propres formats. Chaque interdiction porte donc soit son mécanisme de
vérification, soit la mention qu'elle n'en a pas.

**10. Un savoir-faire exécutable va dans un script.** Quarante lignes expliquaient comment piloter
un écran dans un pseudo-terminal ; elles sont devenues `scripts/check-e2e.py`, où les pièges sont
**implémentés**. Ne reste en texte que ce qu'un script ne peut pas absorber.

**11. Une formulation durable plutôt qu'un nombre exact.** Un nombre recopié à la main redevient
faux au prochain ajout. Écrire « l'oracle mesure **chaque** touche dans **chaque** contexte », et
laisser le compte au fichier engendré. Garder un nombre seulement si la grandeur porte un sens
qu'une phrase ne rend pas — et alors le **vérifier en l'écrivant**.

## Ce qui se garde, explicitement

Sans cette liste, un nettoyage zélé jette le bon :

- les en-têtes `//!` de `dsp` et `audio-io`, et les `///` d'API d'`engine` : ils donnent unités,
  valeurs par défaut, cas `None`, garanties temps réel ;
- les **règles temps réel** de `crates/dsp/CLAUDE.md` ;
- les tables de mesures **actuelles** : budgets par bloc, latences, RT60, seuils, tolérances ;
- le câblage exigé par `scripts/test-backends.sh` : information non évidente et irremplaçable ;
- les renvois `ADR 000x` ;
- une justification **durable** avec sa règle, comme dans `scripts/check-docs.sh` : « au-delà de
  ~600 lignes, un agent lit en morceaux et rate des invariants » — la raison ne vieillit pas.

## Ce que la machine vérifie, et ce qu'elle ne vérifie pas

`scripts/check-docs.sh` (étapes 9 à 12) tient les quatre règles qui s'écrivent en expression
régulière :

| Vérifié | Règle | Portée | Verdict |
|---|---|---|---|
| numéro d'issue ou de PR | 2 | commentaires et rustdoc de `crates/**/*.rs` | **échec** |
| le même, dans une chaîne littérale | 2 | `crates/**/*.rs`, compté à part (certains s'affichent) | **échec** |
| mots-signaux temporels, errata datés | 1, 5 | les mêmes commentaires, `docs/`, les `CLAUDE.md` | **échec** |
| chemin cité qui n'existe plus | — | les `CLAUDE.md` et `docs/` | **échec** |
| taille d'un document | — | les `CLAUDE.md` et `docs/` | avertissement au-delà de 600 lignes |

Les quatre premiers **refusent le commit** : une règle qui avertit sans refuser finit ignorée, comme
un test de coût qui imprime des µs que personne ne relit. Le cinquième reste un avertissement parce
que découper un document est un chantier, pas une correction. Une occurrence légitime se traite donc
en **exception inscrite dans le script**, avec sa raison — jamais en revenant à l'avertissement.

Exemptés, et pour une raison de genre : le `CHANGELOG.md` **est** de l'histoire ; un ADR dit ce qui
était le cas pour justifier sa décision, et ne se réécrit pas — il se supersède ; ce fichier doit
nommer les mots qu'il proscrit ; `docs/cli.md` est engendré.

**Les règles 3, 4, 6, 7, 8, 10 et 11 n'ont aucun mécanisme** : la duplication, la mesure qui
s'empile, le récit, l'état d'avancement, la paraphrase et le nombre recopié se voient à la lecture,
pas au `grep`.

## Quand relire

À chaque commit pour ce qui se mécanise. Le reste **par zone, à chaque publication** (procédure du
`CLAUDE.md` racine) : un récit sans mot-signal, une mesure qui s'empile proprement, un fait devenu
faux sans que son chemin change.
