# audio-io — accès à la carte son

**Responsabilité.** Ouvrir une carte (ALSA direct ou carte simulée), régler buffer et
périodes, passer le thread audio en temps réel, et appeler un `Processor` à chaque cycle avec un
`Io` (canaux désentrelacés, nombre de canaux = celui de la carte). Rien d'autre.

**Interdit** (vérifié par `scripts/check-arch.sh`) : `clap`, la config utilisateur (`toml` et
`serde` : rien ici ne se sérialise), `dsp`, `nam-sys`, `ratatui`, `namfoop`. Les options CLI et la lecture de la config vivent dans le
binaire (`crates/namfoop/src/cli_audio.rs`), qui produit un `Settings`. Erreurs : `thiserror`
(`error.rs`) ; les messages sont ceux que voit l'utilisateur, ne pas les changer au passage.

**Fichiers.** `lib.rs` (traits `Io`, `Processor`, `Stream`, `VecIo`, `Stats`, `Settings`, `start`,
`start_with`), `alsa.rs` (streams liés, `--capture-device` séparé, `alsa/mixer.rs` : contrôles
`Settings.mixer` réglés à l'ouverture, avertissement si absents, sonde `cards` / `probe_channels` /
`proc_usb_channels`, profil PipeWire mis à « off » puis restauré, y compris sur panic ; **aucun
device par défaut** — pas de carte configurée, ou carte absente (`ENODEV` / `ENOENT`), rend
`no_card_message` : les cartes vues et la marche à suivre, et jamais une carte choisie à la place de
l'utilisateur), `sim.rs` (basse synthétique sur l'entrée 1, bruit sur la 2), `rt.rs` (SCHED_FIFO 80, RLIMIT_RTTIME, dénormaux, `setup_audio_thread` : cœur `Settings.cpu`
et pile préchargée, appelé au démarrage des threads ALSA et sim). Le flush-to-zero a un bloc
**d'assembleur aarch64** qu'aucune compilation x86 ne regarde : `scripts/check-aarch64.sh` le
compile (en `cargo build`, un `cargo check` ne validant pas le contenu d'un `asm!`).

**Règles temps réel** : celles du `CLAUDE.md` racine s'appliquent aux callbacks (aucune
allocation, lock, I/O, log ni panic ; buffers de n'importe quelle longueur).

**Tester sans matériel** : `cargo test -p audio-io`, puis le test de fumée
`namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`. Sur le
matériel (utilisateur seulement, câble sortie 1 -> entrée 2) : `scripts/test-backends.sh`.

## Faits mesurés

- Priorité du thread audio (iTwo, ALSA 48×3, b7k 8 canaux + IR, 3 × 60 s) : rtkit (FIFO 20) ->
  p99 20–31 %, p99,9 32–64 %, jusqu'à 9 buffers > 100 % par minute (max 237 %) ; groupe
  `realtime` (FIFO 80) -> p99 10–11 %, p99,9 13–16 %, aucun buffer > 50 % (max 19–49 %), 0 xrun.
  C'est le seul réglage qui compte sur ce PC : gouverneur `performance`, `mlockall`, cœur dédié
  (`--cpu`) et `cpu_dma_latency` = 0 ne changent rien de mesurable. Médiane 8 %
  dans tous les cas (34 µs au bench, ~80 µs en jeu : non expliqué, sans conséquence).
- Buffer de l'iTwo : **48 frames tient**, 32 et 16 donnent des xruns massifs. Plancher matériel de
  l'aller-retour ~1–2 ms (USB + convertisseurs). Sa latence aller-retour **n'est pas mesurée** :
  seule la MTK l'est, ci-dessous.
- **Latence sous charge** (`latency --chains --count 10`, Soundcraft Signature 12 MTK, ALSA
  buffer 64 / 3 périodes, boucle sortie 3 (AUX) -> entrée 2, retour à 0 dBFS sur un bruit de fond à
  -90 dBFS, modèle d'exemple WaveNet 3 canaux) : aller-retour **305 frames = 6,35 ms**,
  **dispersion 0** sur les dix mesures comptées, **0 xrun**. La valeur vaut pour ce buffer-là : la
  boucle grandit avec lui, et cette pente n'est pas mesurée.
  **La charge ne décale pas la latence** : les cinq lignes « à côté » donnent 305 frames, à la frame
  près, jusqu'à quatre `nam › ir` en parallèle. L'écart de la ligne « dans le trajet » — 16 frames
  pour un modèle seul, 18 (0,4 ms) pour tout le reste du catalogue — est le temps que l'**attaque**
  du son met à monter, et non un retard que la chaîne ajouterait : il ne dépend ni du nombre de
  modèles ni des effets.
  Charge par bloc, passe « à côté » en régime établi : `nam` 2 / 3 %, `nam › ir` 2 / 4 %, avec
  effets 2 / 4 %, deux `nam › ir` 3 / 6 %, quatre 6 / 10 % (médiane / p99,9) — **aucun bloc au-delà
  de 80 %** du budget.
  Deux conditions à cette stabilité, toutes deux nécessaires : **attendre que le flux soit établi**
  (3 s, plus deux fenêtres d'échauffement non comptées ; sans elles la référence dispersait de
  42 frames) et chercher le **début** du retour et non sa crête (l'instant du maximum sautait d'un
  lobe à l'autre, 42 frames d'écart, alors que chaque passe était stable).
- ALSA : format négocié dans `alsa/format.rs` (S32_LE > S24_3LE > S24_LE > S16_LE, chaque sens
  séparément ; iTwo = S32_LE, conversion S32 vérifiée par un test) ; côté
  capture, au moins 4 périodes (n'ajoute pas de latence, absorbe les à-coups).
- `start_with` attend 500 ms puis remet les xruns à zéro : les premiers callbacks sont à froid, et
  les xruns comptés ensuite sont ceux du régime établi.

## Les backends

Deux, et le DSP leur est commun (trait `audio_io::Io`, nombre de canaux de la carte) : seuls l'accès
à la carte et le threading diffèrent. Tout changement qui touche l'audio doit marcher sur **ALSA**
(testé sur le matériel) et sur **sim** (sans matériel, donc vérifiable par un agent). Décision :
[ADR 0010](../../docs/adr/0010-retrait-de-jack.md).
- ALSA : `namfoop --backend alsa --device hw:iTwo --buffer 48 --periods 3 ...` ; si PipeWire tient la
  carte, son profil passe à « off » pendant la session puis est restauré (y compris en cas de panic).
  Un SIGKILL laisse le profil à « off » : `pactl set-card-profile <carte> pro-audio` pour réparer.
- sim : `namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 -m b7k` exerce toute la
  chaîne temps réel sans carte ; `--backend sim setup` ouvre l'assistant.
- Les tests matériels (`scripts/test-backends.sh`) demandent un câble sortie 1 -> entrée 2 et le
  bouton Mixer de l'iTwo côté Playback. **Un agent ne peut pas brancher de câble : demander à
  l'utilisateur**, et prévenir avant de lancer des tests qui émettent des clics ou prennent la carte.
- Ne pas lancer `run` avec un gain audible sans l'accord de l'utilisateur (utiliser
  `--master-db -120` pour les tests de fumée).

La place de cette crate dans le graphe des dépendances : la carte du `CLAUDE.md` racine et
[`docs/architecture.md`](../../docs/architecture.md).
