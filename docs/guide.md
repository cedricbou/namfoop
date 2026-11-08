# Guide utilisateur namfoop

namfoop transforme un PC Linux (ou, à terme, un Raspberry Pi 5) et une carte son USB en
multi-effet pour basse : modèles NAM (amplis, pédales, rigs complets capturés par réseau de
neurones), IR de baffle, réglage de niveau et limiteur, avec une latence de l'ordre de 5 ms.

Ce guide explique **comment s'en servir** et **pourquoi** chaque réglage existe. La liste brute de
toutes les options, générée depuis l'aide intégrée, est dans la [référence des commandes](cli.md).
Partout, `namfoop <commande> --help` affiche l'aide à jour.

Sommaire :

1. [Principe](#1-principe)
2. [Matériel et câblage](#2-matériel-et-câblage)
3. [Installation](#3-installation)
4. [Réglages du système](#4-réglages-du-système)
5. [La bibliothèque de sons](#5-la-bibliothèque-de-sons)
6. [Calibrer sa basse (à faire une fois)](#6-calibrer-sa-basse-à-faire-une-fois)
7. [Jouer](#7-jouer)
8. [Régler le son et le volume](#8-régler-le-son-et-le-volume)
9. [Choisir le backend audio](#9-choisir-le-backend-audio)
10. [Travailler sans jouer : rendu, bench, inventaire](#10-travailler-sans-jouer--rendu-bench-inventaire)
11. [Mesurer la latence](#11-mesurer-la-latence)
12. [Le fichier de configuration](#12-le-fichier-de-configuration)
13. [Dépannage](#13-dépannage)
14. [Limites actuelles](#14-limites-actuelles)

---

## 1. Principe

```
           ┌──────────────────────────── namfoop ────────────────────────────┐
basse ─▶ entrée 1 ─▶ trim ─▶ NAM ─▶ IR ─▶ niveau ─▶ master ─▶ limiteur ─▶ sortie 1 ─▶ sondier
           │                                                       └────▶ sortie 2 (même / DI / muet)
           └─────────────────────────────────────────────────────────────────┘
```

| Étage | Rôle | Réglages |
|---|---|---|
| trim | adapte le niveau de **ta** basse au niveau attendu par les modèles | `calibrate`, `--trim` |
| NAM | le modèle d'ampli / pédale / rig | `-m`, `--input-db`, `--output-db`, `--size` |
| IR | le baffle (seulement si le modèle n'en contient pas) | `-i`, `--ir-ms` |
| niveau | volume de sortie cohérent d'un modèle à l'autre | `--level`, `--target-db` |
| master | volume final | `--master-db` |
| limiteur | empêche la sortie de dépasser -1 dBFS | `--no-limiter` pour l'enlever |

---

## 2. Matériel et câblage

Configuration de référence : PreSonus **AudioBox iTwo** (2 entrées, 2 sorties, USB).

- Basse dans l'**entrée 1**, en position instrument.
- **Sortie 1** vers le sondier (ou une tranche de ta table). **Sortie 2** : voir `--out2` (même
  signal, DI propre, ou muet).
- Bouton **Mixer** de l'iTwo **à fond côté Playback**. Sinon le monitoring direct de la carte
  mélange la basse brute au son traité (effet de filtre en peigne), et provoque un larsen si un
  câble relie une sortie à une entrée.
- Gain d'entrée de l'iTwo : assez haut pour un bon niveau, jamais au point d'allumer la LED clip
  (namfoop affiche `[CLIP]` dans ce cas, voir [§ 7](#7-jouer)).

**Retours in-ear avec une table** (ex. Soundcraft Signature 12 MTK) : envoie la sortie de l'iTwo sur
une tranche (ex. 7/8) et le son de l'ordinateur sur une autre (ex. le retour USB sur 11/12). Vérifie
que chaque tranche alimente l'aux du casque : un retour USB affecté directement au master ne passe
pas par les aux.

---

## 3. Installation

Prérequis (Arch Linux) : `rustup` (Rust stable), `gcc`, `alsa-lib`, `pipewire-jack` (ou `jack2`),
`git`.

```sh
git clone --recurse-submodules https://github.com/cedricbou/namfoop.git
cd namfoop
cargo build --release          # compile aussi le moteur NAM en C++ (~30 s)
./target/release/namfoop --help
```

Pour taper `namfoop` au lieu de `./target/release/namfoop`, ajoute le dossier au PATH ou copie le
binaire : `cp target/release/namfoop ~/.local/bin/`. Les exemples de ce guide écrivent `namfoop`.

Si le dépôt a été cloné sans `--recurse-submodules` : `git submodule update --init --recursive`.

---

## 4. Réglages du système

### PipeWire (pour le backend JACK)

Par défaut, PipeWire ajoute beaucoup de marge sur les cartes USB (13 ms aller-retour mesurés).
Crée `~/.config/wireplumber/wireplumber.conf.d/51-itwo-lowlatency.conf` :

```
monitor.alsa.rules = [
  { matches = [ { node.name = "~alsa_(input|output).*AudioBox_iTwo.*" } ]
    actions = { update-props = {
      audio.rate = 48000
      api.alsa.period-size = 64
      api.alsa.period-num = 2
      api.alsa.headroom = 0
      api.alsa.disable-batch = true
      session.suspend-timeout-seconds = 0 } } }
]
```

puis passe la carte en profil **Pro Audio** et redémarre PipeWire :

```sh
pactl list cards short                                   # repérer le nom de la carte
pactl set-card-profile <nom de la carte> pro-audio
systemctl --user restart wireplumber pipewire pipewire-pulse
```

En Pro Audio, les ports JACK s'appellent `capture_AUX0/1` et `playback_AUX0/1` (`namfoop ports`
pour les voir).

### Temps réel

Le backend ALSA demande la priorité temps réel (SCHED_FIFO) à rtkit, sans configuration. Pour une
priorité plus élevée : installer `realtime-privileges` et s'ajouter au groupe `realtime`.

---

## 5. La bibliothèque de sons

namfoop cherche les sons dans `library/` (option `--library` pour un autre dossier) :

```
library/nam/<pack>/*.nam     modèles
library/ir/<pack>/*.wav      réponses impulsionnelles de baffle
```

**Importer des zips Tone3000** téléchargés en vrac dans `nam/` :

```sh
scripts/import-tone3000.sh nam library
```

**Inventaire** :

```sh
namfoop models
#  [Darkglass B7K Ultra]
#    Darkglass Drive - Microtubes B7K Ultra  — amp_cab overdrive | WaveNet 3ch + WaveNet 8ch | -39.7 dB
```

Pour chaque modèle : le **type de capture**, le style, les **tailles** disponibles et la loudness.

### Faut-il une IR ?

| Type | Contenu de la capture | IR |
|---|---|---|
| `amp_cab` | ampli **et** baffle micro (la plupart des rigs Tone3000) | **non**, namfoop prévient si tu en ajoutes une |
| `amp` | ampli seul (ex. MESA 800D « SEND ») | **oui** |
| `pedal` | pédale seule (ex. Rat) | oui, ou un autre modèle d'ampli plus tard |

### Désigner un modèle ou une IR

`-m` (`--model`) et `-i` (`--ir`) acceptent :

- un **chemin** : `-m "library/nam/Darkglass B7K Ultra/Darkglass Drive - Microtubes B7K Ultra.nam"` ;
- des **mots-clés** : tous doivent apparaître dans le chemin, sans tenir compte des majuscules.
  Si plusieurs fichiers correspondent, celui dont le nom contient l'**expression exacte** gagne ;
  sinon namfoop affiche la liste des candidats pour que tu précises.

```sh
namfoop run -m b7k
namfoop run -m "ultra lo sm57"
namfoop run -m "800d modern" -i "dg212 sm57"
```

### Taille des modèles (`--size`)

Les modèles récents (format NAM 0.7, « slimmable ») contiennent plusieurs tailles du même son :
`--size 0` choisit la plus légère (ex. 3 canaux), `--size 1` (défaut) la complète (ex. 8 canaux).
La version légère coûte environ deux fois moins de CPU : utile sur Raspberry Pi. Voir
`namfoop bench --all-sizes` ([§ 10](#10-travailler-sans-jouer--rendu-bench-inventaire)).

---

## 6. Calibrer sa basse (à faire une fois)

**Pourquoi.** Un modèle NAM a été entraîné avec le DI de la personne qui l'a capturé, à un niveau
numérique précis. Si ta basse arrive plus fort (basse active, gain de carte élevé), le modèle
sature beaucoup plus que sur les démos : un Bassman 800 « presque clair » devient très saturé.
Les modèles indiquent rarement leur niveau de calibration : on mesure donc **ton** niveau.

```sh
namfoop calibrate
```

1. Joue **fort**, comme sur tes passages les plus appuyés, pendant la durée indiquée (15 s par
   défaut, `--seconds` pour changer). Tu t'entends en direct sur les sorties.
2. namfoop affiche ta crête et propose un **trim** :
   ```
   Crête max -9.8 dBFS, crête robuste -12.1 dBFS, RMS joué -24.0 dBFS
   Trim d'entrée proposé : -18.0 dB (crête robuste visée -30 dBFS à l'entrée des modèles)
   ```
3. Il sauve le trim, ton enregistrement et le mode de volume **match** dans
   `~/.config/namfoop/` ([§ 12](#12-le-fichier-de-configuration)). Toutes les commandes `run` et
   `render` les utilisent ensuite.

Options :

| Option | Rôle |
|---|---|
| `--seconds` | durée d'enregistrement |
| `--target` | crête visée à l'entrée des modèles, -30 dBFS par défaut. Plus haut (ex. -27) = modèles plus saturés |
| `--channel` | entrée de la carte où est branchée la basse (1 par défaut) |
| `--dry-run` | analyse seulement, ne sauve rien |
| `--config` | fichier de config à écrire |

À refaire si tu changes de basse ou le gain de la carte. Si l'entrée sature pendant la
calibration, namfoop refuse : baisse le gain de la carte et recommence.

---

## 7. Jouer

```sh
pw-jack namfoop --device iTwo run -m b7k                          # via PipeWire
namfoop --backend alsa --device hw:iTwo --buffer 48 run -m b7k     # ALSA direct, latence minimale
```

Les **options globales** (`--backend`, `--device`, `--buffer`, `--periods`, `--rate`,
`--client-name`) se placent **avant** `run`, les options du son **après**. Voir
[§ 9](#9-choisir-le-backend-audio) pour le choix du backend.

Ctrl+C arrête. Pour changer de son, relance avec un autre `-m` (les presets et le MIDI viendront).

### La ligne d'état

```
  in  -14.2 dB  out  -9.8 dB  lim -1.2 dB  dsp   8.5 %  xruns 0
```

| Champ | Signification | Que faire |
|---|---|---|
| `in` | crête d'entrée | vers -12 / -6 dB en jouant fort. `[CLIP ×n]` : baisse le gain de la carte |
| `out` | crête de sortie | doit rester sous 0 dB |
| `lim` | réduction de gain du limiteur | s'il travaille souvent, baisse `--master-db` |
| `dsp` | part du temps disponible utilisée par le traitement (pic) | au-delà de 70 %, risque de craquements |
| `xruns` | décrochages audio depuis le démarrage | doit rester à 0 |
| `[CLIP SORTIE]` | échantillons écrêtés en sortie (limiteur désactivé) | baisse `--master-db` |

À l'arrêt, un résumé donne la charge DSP maximale, les xruns et les éventuels écrêtages.

### Options de `run`

| Option | Rôle |
|---|---|
| `-m`, `--model` | modèle NAM ([§ 5](#5-la-bibliothèque-de-sons)) |
| `-i`, `--ir` | IR de baffle |
| `--out2` | contenu de la sortie 2 : `same` (même son), `di` (basse directe, pour un DI propre au sondier), `mute` |
| `--seconds` | arrêt automatique (tests) |
| `-q`, `--quiet` | pas de ligne d'état |

Plus toutes les options de son du [§ 8](#8-régler-le-son-et-le-volume).

---

## 8. Régler le son et le volume

### Saturation : `--input-db`

Comme le gain d'un ampli : `--input-db 6` pousse le modèle, `--input-db -6` l'éclaircit. Il
s'ajoute au trim de calibration, et se règle **par modèle**.

```sh
namfoop run -m "bassman 800" --input-db 4
```

### Trim d'entrée : `--trim`

Le trim est mesuré par `calibrate` et vaut pour tous les modèles. `--trim` le force pour une
session (ex. `--trim -12`) sans toucher à la config.

### Volume de sortie : `--level`

Les captures sortent à des niveaux très différents (de -14 à -40 dB). Trois modes :

| Mode | Effet |
|---|---|
| `match` | (après `calibrate`) le son traité a **le même volume que ta basse en direct**. Au chargement, ton enregistrement de calibration passe dans la chaîne et un gain compense l'écart. Pas de saut de volume d'un modèle à l'autre ni entre clair et traité |
| `normalize` | (défaut sans calibration) la loudness annoncée par le modèle est ramenée à `--target-db` (-18 dB, la convention NAM) |
| `raw` | sortie brute du modèle, sans correction |

`--output-db` ajoute un gain après le modèle, `--master-db` règle le volume final.

### IR : `--ir-ms`

Les IR sont tronquées à 200 ms par défaut, avec un fondu : un baffle tient en ~100 ms, la suite est
de la pièce. Elles sont rééchantillonnées automatiquement si besoin (44,1 / 96 kHz) et leur niveau
est normalisé sur 80 Hz – 5 kHz.

### Limiteur

Actif par défaut à -1 dBFS, sans latence ajoutée : il empêche l'écrêtage numérique dur quand un
modèle sort trop fort. `--no-limiter` le désactive (la ligne d'état signale alors `[CLIP SORTIE]`).

### Config et tests reproductibles

`--config` choisit un autre fichier de configuration, `--no-config` l'ignore complètement
(réglages par défaut : pratique pour comparer ou pour les tests automatiques).

---

## 9. Choisir le backend audio

| | JACK (`--backend jack`, défaut) | ALSA direct (`--backend alsa`) |
|---|---|---|
| Lancement | `pw-jack namfoop --device iTwo ...` | `namfoop --backend alsa --device hw:iTwo ...` |
| Latence mesurée (iTwo) | 6,5–7 ms, varie un peu | **5,2 ms, stable** |
| Partage de la carte | oui (navigateur, Ableton... peuvent jouer sur l'iTwo) | non : namfoop prend la carte en exclusif |
| Usage conseillé | développement, mélange avec d'autres applis | live, Raspberry Pi |

Options :

- `--device` : la carte. En JACK, une partie du nom (`iTwo`) ; en ALSA, le device (`hw:iTwo`).
- `--buffer` : taille de période en frames. JACK n'accepte que des puissances de 2 (32, 64...),
  64 conseillé. ALSA : 48 conseillé avec l'iTwo ; 32 et en dessous provoquent des xruns.
- `--periods` : nombre de périodes côté ALSA (3 par défaut, meilleur compromis mesuré).
- `--rate` : fréquence d'échantillonnage (48000 ; les modèles sont entraînés à 48 kHz). En JACK,
  c'est le serveur qui décide : namfoop refuse de démarrer s'il tourne à une autre fréquence.
- `--client-name` : nom du client JACK (pour lancer plusieurs instances).

**En ALSA, si PipeWire utilise la carte**, namfoop coupe son profil le temps de la session et le
remet à l'arrêt, y compris en cas de plantage. Seul un `kill -9` peut laisser le profil sur « off »
(voir [§ 13](#13-dépannage)).

---

## 10. Travailler sans jouer : rendu, bench, inventaire

### `namfoop render` : traiter un fichier

Même chaîne que `run`, sur un WAV :

```sh
namfoop render -m b7k --in di.wav -o b7k.wav
namfoop render -m "800d modern" -i "dg212 sm57" -o test.wav
```

- `--in` : WAV d'entrée (sans : signal de test fourni avec le moteur NAM) ;
- `-o`, `--out` : WAV de sortie (flottant 32 bits). Sans : affiche seulement les niveaux ;
- `--block` : taille de bloc (le son ne dépend pas de ce réglage, c'est vérifié par les tests).

Toutes les options de son du [§ 8](#8-régler-le-son-et-le-volume) s'appliquent.

### `namfoop bench` : coût CPU

```sh
namfoop bench --all-sizes                          # toute la bibliothèque, chaque taille
namfoop bench --block 48 b7k "800d modern"         # quelques modèles
```

Affiche le temps moyen et maximal par bloc, et la part du budget temps réel. `--block`,
`--seconds` (durée simulée) et `--library` sont réglables ; `--all-sizes` mesure chaque taille des
modèles slimmable. À lancer sur la machine de scène (ex. le Pi) pour savoir quels modèles tiennent.

### `namfoop models` et `namfoop ports`

`namfoop models` liste la bibliothèque ([§ 5](#5-la-bibliothèque-de-sons)). `namfoop ports` liste
les ports JACK audio et MIDI (à lancer via `pw-jack`).

---

## 11. Mesurer la latence

Relie la **sortie 1** à l'**entrée 2** avec un câble, gain de l'entrée 2 bas, bouton Mixer côté
Playback, casque bas (la mesure émet des clics) :

```sh
pw-jack namfoop --device iTwo --buffer 64 latency
namfoop --backend alsa --device hw:iTwo --buffer 48 latency --count 20
```

`--out-ch` / `--in-ch` choisissent les canaux (1 et 2 par défaut), `--count` le nombre de
mesures. Le résultat est l'aller-retour complet (conversion de la carte comprise).

---

## 12. Le fichier de configuration

`~/.config/namfoop/config.toml`, écrit par `calibrate`, modifiable à la main :

```toml
trim_db = -18.0                      # trim d'entrée de l'instrument (dB)
level = "match"                      # "match", "normalize" ou "raw"
target_db = -18.0                    # cible du mode normalize
limiter = true                       # limiteur de sortie
calibration_wav = "calibration.wav"  # enregistrement de calibration (relatif à ce dossier)
```

Les options de la ligne de commande priment toujours sur ce fichier.

---

## 13. Dépannage

**Le son est beaucoup plus saturé que la démo Tone3000.** Niveau d'entrée trop chaud : lance
`namfoop calibrate`, ou essaie `--input-db -12`.

**Le son traité est bien plus fort que le son clair.** Passe en `--level match` (après
calibration), ou baisse `--master-db`.

**`[CLIP]` sur l'entrée.** Le préampli de la carte sature avant namfoop : baisse le gain de
l'entrée de la carte (aucun réglage logiciel ne rattrape cette saturation).

**Craquements, xruns.** Augmente `--buffer` (ALSA : 64), utilise le backend ALSA, ferme les
applications lourdes, ou choisis un modèle plus léger (`--size 0`).

**Larsen ou son « en double », filtré.** Bouton Mixer de l'iTwo pas à fond côté Playback.

**« occupé par une autre application » en ALSA.** Une autre application (ou un autre namfoop)
tient la carte. Ferme-la, ou utilise le backend JACK.

**Plus de son PipeWire sur l'iTwo après un plantage dur.** Le profil est resté sur « off » :
`pactl set-card-profile <nom de la carte> pro-audio`.

**« … est ambigu ».** Plusieurs fichiers correspondent aux mots-clés : namfoop affiche la liste,
ajoute un mot ou donne le chemin.

**Le son de l'ordinateur n'arrive pas dans le casque de la table.** Voir le
[§ 2](#2-matériel-et-câblage) : le retour USB doit passer par une tranche qui alimente l'aux.

---

## 14. Limites actuelles

namfoop est un prototype. Pas encore disponibles : effets (gate, compresseur, EQ, crossover,
modulation, delay, réverbe), presets, changement de son par MIDI, micro sur l'entrée 2, démarrage
automatique, version Raspberry Pi. Voir le [CHANGELOG](../CHANGELOG.md) pour suivre les versions.
