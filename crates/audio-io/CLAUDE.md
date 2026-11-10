# audio-io — accès à la carte son

**Responsabilité.** Ouvrir une carte (ALSA direct, JACK, carte simulée), régler buffer et
périodes, passer le thread audio en temps réel, et appeler un `Processor` à chaque cycle avec un
`Io` (canaux désentrelacés, nombre de canaux = celui de la carte). Rien d'autre.

**Interdit** (vérifié par `scripts/check-arch.sh`) : `clap`, la config utilisateur (`toml`),
`dsp`, `nam-sys`, `ratatui`, `namfoop`. Les options CLI et la lecture de la config vivent dans le
binaire (`crates/namfoop/src/cli_audio.rs`), qui produit un `Settings`. Erreurs : `thiserror`
(`error.rs`) ; les messages sont ceux que voit l'utilisateur, ne pas les changer au passage.

**Fichiers.** `lib.rs` (traits `Io`, `Processor`, `Stream`, `VecIo`, `Stats`, `Settings`, `start`,
`start_with`), `alsa.rs` (streams liés, `--capture-device` séparé, sonde `cards` / `probe_channels` / `proc_usb_channels`,
profil PipeWire mis à « off » puis restauré, y compris sur panic), `jack.rs` (relance avec la
libjack PipeWire si besoin : `pw-jack` n'est jamais nécessaire), `sim.rs` (basse synthétique sur
l'entrée 1, bruit sur la 2), `rt.rs` (SCHED_FIFO 80, RLIMIT_RTTIME, dénormaux).

**Règles temps réel** : celles du `CLAUDE.md` racine s'appliquent aux callbacks (aucune
allocation, lock, I/O, log ni panic ; buffers de n'importe quelle longueur).

**Tester sans matériel** : `cargo test -p audio-io`, puis le test de fumée
`namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`. Sur le
matériel (utilisateur seulement, câble sortie 1 -> entrée 2) : `scripts/test-backends.sh` sur JACK
et ALSA.

## Faits mesurés

- Latence aller-retour iTwo (PC) : ALSA 48×3 = 5,2 ms ; JACK/PipeWire 64 = 6,5–7 ms. Plancher
  matériel ~1–2 ms (USB + convertisseurs). 32 et 16 frames : xruns massifs avec l'iTwo.
- ALSA : format négocié dans `alsa/format.rs` (S32_LE > S24_3LE > S24_LE > S16_LE, chaque sens
  séparément ; iTwo = S32_LE, conversion S32 identique à l'ancienne, test à l'appui) ; côté
  capture, au moins 4 périodes (n'ajoute pas de latence, absorbe les à-coups).
- `start_with` attend 500 ms puis remet les xruns à zéro : PipeWire change de quantum juste après
  l'activation, les premiers callbacks sont à froid.

## Les backends

Tout changement qui touche l'audio doit marcher sur **JACK et ALSA** (testé sur le matériel) et sur
**sim** (sans matériel, utilisable par les agents). Le DSP est commun (trait `audio_io::Io`, nombre de
canaux de la carte) ; seuls l'accès à la carte et le threading diffèrent.
- `pw-jack` n'est jamais nécessaire (relance automatique avec la libjack PipeWire si besoin,
  `jack::open_client`) : ne pas l'utiliser dans les docs ni les scripts (vérifié par check-docs.sh).
- JACK : `namfoop --backend jack --device iTwo ...` ; le buffer demandé force le quantum PipeWire
  (puissances de 2 seulement). `--device` = nom exact du périphérique (écrit par setup) ou regex.
- sim : `namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 -m b7k` exerce toute la
  chaîne temps réel sans carte ; `--backend sim setup` ouvre l'assistant.
- ALSA : `namfoop --backend alsa --device hw:iTwo --buffer 48 --periods 3 ...` ; si PipeWire tient la
  carte, son profil passe à « off » pendant la session puis est restauré (y compris en cas de panic).
  Un SIGKILL laisse le profil à « off » : `pactl set-card-profile <carte> pro-audio` pour réparer.
- Les tests matériels (`scripts/test-backends.sh`) demandent un câble sortie 1 -> entrée 2 et le
  bouton Mixer de l'iTwo côté Playback. **Un agent ne peut pas brancher de câble : demander à
  l'utilisateur**, et prévenir avant de lancer des tests qui émettent des clics ou prennent la carte.
- Ne pas lancer `run` avec un gain audible sans l'accord de l'utilisateur (utiliser
  `--master-db -120` pour les tests de fumée).

## Dette connue

- Crépitements signalés en JACK avec un modèle saturé : origine non confirmée (xruns ou grain).

## Carte détaillée (reprise du `CLAUDE.md` racine)

```
crates/audio-io/  accès à la carte, sans clap, config ni DSP (voir crates/audio-io/CLAUDE.md) :
                  lib.rs (trait Io : canaux dynamiques ; Processor ; Settings ; start), error.rs
                  (thiserror), jack.rs, alsa.rs (direct, streams liés, probe, PipeWire « off » ; alsa/format.rs : S32 / S24_3 / S24 / S16),
                  sim.rs (carte simulée), rt.rs (SCHED_FIFO, dénormaux)
```
