# ADR 0004 · Découpage en crates, dépendances vers le bas

**Contexte.** Le binaire portait 9 100 lignes sur 11 460, et cinq commandes montaient chacune
leur flux audio. Un agent qui touche une commande charge tout le reste, et rien ne l'empêche de
créer un couplage. Les fonctionnalités à venir (#3, #6, #7, #46) devraient être faites cinq fois.

**Décision.** Sept crates au plus (`nam-sys`, `dsp`, `audio-io`, `model`, `engine`, `library`,
`namfoop`), chacune avec une responsabilité et un `CLAUDE.md`. Les dépendances ne vont que vers le
bas ; `scripts/check-arch.sh` le vérifie en CI. Migration en 11 PR séquentielles (#47), à
comportement constant, sous le filet des rendus de référence.

**Conséquences.** Une PR de déplacement ne change aucun comportement ; une PR de comportement ne
déplace pas de code. Pas de bus de messages générique ni d'injection de dépendances : appels
directs, la seule frontière ajoutée est `engine` / `audio-io`.
