# ADR 0001 · ALSA direct sur scène, JACK / PipeWire pour le développement

**Statut : remplacé par l'[ADR 0010](0010-retrait-de-jack.md)**, qui retire le backend JACK.
Le texte ci-dessous est conservé tel qu'il a été adopté.

**Contexte.** Sur l'iTwo, ALSA direct en 48 × 3 donne 5,2 ms aller-retour ; JACK via PipeWire en
64 frames donne 6,5 à 7 ms, et dépend d'un serveur de plus qui peut tomber ou changer de quantum.
Sur le PC de développement, PipeWire tient déjà la carte et JACK permet de brancher d'autres
logiciels.

**Décision.** En live (et sur le Pi), namfoop ouvre la carte en ALSA direct (`hw:`), sans serveur
audio. JACK reste supporté pour le développement et les essais ; il n'est jamais obligatoire.
Les deux backends partagent le même DSP et sont testés tous les deux (`scripts/test-backends.sh`).

**Conséquences.** Le backend ALSA porte la gestion des xruns, des streams liés et du profil
PipeWire mis à « off » le temps de la session. Tout changement audio se vérifie sur JACK, ALSA et
sim.
