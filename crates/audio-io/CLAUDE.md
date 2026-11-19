# audio-io — accès à la carte son

**Responsabilité.** Ouvrir une carte (ALSA direct ou carte simulée), régler buffer et
périodes, passer le thread audio en temps réel, et appeler un `Processor` à chaque cycle avec un
`Io` (canaux désentrelacés, nombre de canaux = celui de la carte). Rien d'autre.

**Interdit** (vérifié par `scripts/check-arch.sh`) : `clap`, la config utilisateur (`toml` et
`serde` : rien ici ne se sérialise), `dsp`, `nam-sys`, `ratatui`, `namfoop`. Les options CLI et la lecture de la config vivent dans le
binaire (`crates/namfoop/src/cli_audio.rs`), qui produit un `Settings`. Erreurs : `thiserror`
(`error.rs`) ; les messages sont ceux que voit l'utilisateur, ne pas les changer au passage.

**Fichiers.** `lib.rs` (traits `Io`, `Processor`, `Stream`, `VecIo`, `Stats`, `Settings`, `start`,
`start_with`), `alsa.rs` (streams liés, `--capture-device` séparé, `alsa/mixer.rs` : contrôles `Settings.mixer` réglés à
l'ouverture, avertissement si absents, sonde `cards` / `probe_channels` / `proc_usb_channels`,
profil PipeWire mis à « off » puis restauré, y compris sur panic), `sim.rs` (basse synthétique sur
l'entrée 1, bruit sur la 2), `rt.rs` (SCHED_FIFO 80, RLIMIT_RTTIME, dénormaux, `setup_audio_thread` : cœur `Settings.cpu`
et pile préchargée, appelé au démarrage des threads ALSA et sim).

**Règles temps réel** : celles du `CLAUDE.md` racine s'appliquent aux callbacks (aucune
allocation, lock, I/O, log ni panic ; buffers de n'importe quelle longueur).

**Tester sans matériel** : `cargo test -p audio-io`, puis le test de fumée
`namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`. Sur le
matériel (utilisateur seulement, câble sortie 1 -> entrée 2) : `scripts/test-backends.sh`.

## Faits mesurés

- Priorité du thread audio (#3, iTwo, ALSA 48×3, b7k 8 canaux + IR, 3 × 60 s) : rtkit (FIFO 20) ->
  p99 20–31 %, p99,9 32–64 %, jusqu'à 9 buffers > 100 % par minute (max 237 %) ; groupe
  `realtime` (FIFO 80) -> p99 10–11 %, p99,9 13–16 %, aucun buffer > 50 % (max 19–49 %), 0 xrun.
  C'est le seul réglage qui compte sur ce PC : gouverneur `performance`, `mlockall`, cœur dédié
  (`--cpu`) et `cpu_dma_latency` = 0 (essayé, retiré) ne changent rien de mesurable. Médiane 8 %
  dans tous les cas (34 µs au bench, ~80 µs en jeu : non expliqué, sans conséquence).
- Latence aller-retour iTwo (PC), **ALSA 48×3 : 5,2 ms** (`latency`, boucle sortie 1 -> entrée 2,
  impulsion émise à 0,7 = -3,1 dBFS : 256 frames, écart min–max **18 frames**). Plancher matériel
  ~1–2 ms (USB + convertisseurs). 32 et 16 frames : xruns massifs avec l'iTwo. `latency` cherche le
  **maximum** du retour et non un seuil fixe, parce que le niveau d'arrivée dépend du gain du chemin
  et non seulement de la latence.
- ALSA : format négocié dans `alsa/format.rs` (S32_LE > S24_3LE > S24_LE > S16_LE, chaque sens
  séparément ; iTwo = S32_LE, conversion S32 identique à l'ancienne, test à l'appui) ; côté
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

## Carte détaillée (reprise du `CLAUDE.md` racine)

```
crates/audio-io/  accès à la carte, sans clap, config ni DSP :
                  lib.rs (trait Io : canaux dynamiques ; Processor ; Settings ; start), error.rs
                  (thiserror), alsa.rs (direct, streams liés, probe, PipeWire « off » ; alsa/format.rs : S32 / S24_3 / S24 / S16),
                  sim.rs (carte simulée), rt.rs (SCHED_FIFO, dénormaux)
```
