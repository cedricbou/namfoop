# Guide utilisateur namfoop

namfoop transforme un PC Linux (ou, à terme, un Raspberry Pi 5) et une carte son USB en
multi-effet pour basse : modèles NAM (amplis, pédales, rigs complets capturés par réseau de
neurones), IR de baffle, réglage de niveau et limiteur, avec une latence de l'ordre de 5 ms.

Ce guide explique **comment s'en servir** et **pourquoi** chaque réglage existe. La liste brute de
toutes les options, générée depuis l'aide intégrée, est dans la [référence des commandes](cli.md).
Partout, `namfoop <commande> --help` affiche l'aide à jour.

En bref, après l'installation :

```sh
namfoop setup         # une fois : carte, entrées, sorties, buffer (assistant plein écran)
namfoop calibrate     # une fois : niveau de ta basse
namfoop browse        # choisir modèle, IR, mélange en jouant
namfoop run -m b7k    # jouer
```

Sommaire :

1. [Principe](#1-principe)
2. [Matériel et câblage](#2-matériel-et-câblage)
3. [Installation](#3-installation)
4. [Réglages du système](#4-réglages-du-système)
5. [Configurer la carte : l'assistant](#5-configurer-la-carte--lassistant)
6. [La bibliothèque de sons](#6-la-bibliothèque-de-sons)
7. [Calibrer sa basse](#7-calibrer-sa-basse)
8. [Jouer](#8-jouer)
9. [Régler le son et le volume](#9-régler-le-son-et-le-volume)
10. [Backends, carte et routage en ligne de commande](#10-backends-carte-et-routage-en-ligne-de-commande)
11. [Travailler sans jouer : rendu, bench, inventaire](#11-travailler-sans-jouer--rendu-bench-inventaire)
12. [Mesurer la latence](#12-mesurer-la-latence)
13. [Le fichier de configuration](#13-le-fichier-de-configuration)
14. [Dépannage](#14-dépannage)
15. [Limites actuelles](#15-limites-actuelles)

---

## 1. Principe

```
            ┌─────────────────────────────── namfoop ───────────────────────────────┐
basse ─▶ entrée basse ─▶ trim ─▶ NAM ─▶ IR ─▶ niveau ─▶ master ─▶ limiteur ─┬─▶ sorties « traité »
            │                │                                                └─▶ (sondier, in-ear)
            │                └──────────────────────────────────────────────────▶ sorties « DI »
micro ─▶ entrée micro ─────────────────────────────────────────────────────────▶ sorties « micro »
            └───────────────────────────────────────────────────────────────────────┘
```

| Étage | Rôle | Réglages |
|---|---|---|
| trim | adapte le niveau de **ta** basse au niveau attendu par les modèles | `calibrate`, `--trim` |
| NAM | le modèle d'ampli / pédale / rig | `-m`, `--input-db`, `--output-db`, `--size` |
| IR | le baffle (seulement si le modèle n'en contient pas) | `-i`, `--ir-ms` |
| niveau | volume de sortie cohérent d'un modèle à l'autre | `--level`, `--target-db` |
| master | volume final | `--master-db` |
| limiteur | empêche la sortie de dépasser -1 dBFS | `--no-limiter` pour l'enlever |
| routage | quelle entrée est la basse / le micro, ce que reçoit chaque sortie | `setup`, `--bass-in`, `--mic-in`, `--route` |

---

## 2. Matériel et câblage

Configuration de référence : PreSonus **AudioBox iTwo** (2 entrées, 2 sorties, USB). Toute carte
reconnue par Linux fonctionne, avec n'importe quel nombre de canaux (ex. une table Soundcraft
Signature 12 MTK : 14 entrées / 12 sorties USB).

- Basse dans une entrée instrument (l'assistant repère laquelle, [§ 5](#5-configurer-la-carte--lassistant)).
- Une sortie vers le sondier, éventuellement une autre pour les in-ears ou un DI propre.
- iTwo : bouton **Mixer à fond côté Playback**. Sinon le monitoring direct de la carte mélange la
  basse brute au son traité (effet de filtre en peigne), et provoque un larsen si un câble relie une
  sortie à une entrée.
- Gain d'entrée de la carte : assez haut pour un bon niveau, jamais au point d'allumer la LED clip
  (namfoop affiche alors `[CLIP]`, [§ 8](#8-jouer)).

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

**Pas besoin de `pw-jack`.** Le backend JACK de namfoop parle à la bibliothèque JACK du système. Sur
Arch, le paquet `pipewire-jack` remplace cette bibliothèque par celle de PipeWire : namfoop parle donc
directement à PipeWire. Sur les distributions qui l'installent à part (Debian, Ubuntu, Raspberry Pi
OS), namfoop la trouve et se relance tout seul avec : la commande `pw-jack` n'est jamais nécessaire.

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

## 5. Configurer la carte : l'assistant

```sh
namfoop setup
```

Un assistant plein écran, à faire une fois (et à refaire si tu changes de carte ou de câblage).
Flèches pour naviguer, Entrée pour valider, ← pour revenir, Échap pour quitter sans rien
enregistrer. Six étapes :

1. **Backend** : ALSA direct (latence la plus basse, carte réservée à namfoop : conseillé en live),
   JACK / PipeWire (carte partagée avec le navigateur, Ableton...), ou carte simulée (démo).
2. **Carte** : la liste des cartes avec leur nombre d'entrées et de sorties. Une carte « utilisée par
   PipeWire » n'est pas un problème : namfoop la libère le temps de son utilisation.
3. **Entrées** : un vumètre par entrée, en direct. **Joue de la basse** : l'entrée qui bouge est la
   bonne. Entrée l'affecte à la basse, `m` au micro, `x` retire le micro. Le repère « ◀ signal »
   signale une entrée active qui n'est pas encore affectée (« tu joues sur l'entrée 2, pas la 1 »).
4. **Sorties** : le rôle de chaque sortie, à changer avec ←/→ ou espace : sondier ou in-ear (basse
   traitée), DI (basse directe), micro, rien. `t` envoie un **bip court à -20 dBFS sur cette sortie
   seulement**, pour savoir où elle arrive. **Baisse le volume avant le premier bip.**
5. **Buffer** : chaque taille est testée 5 s, en silence, avec un vrai modèle de ta bibliothèque
   comme charge. La plus petite taille sans xrun est présélectionnée.
6. **Résumé** : Entrée enregistre les sections `[audio]` et `[routing]` de la config
   ([§ 13](#13-le-fichier-de-configuration)). Le reste (trim, niveau...) n'est pas touché.

Ensuite, `namfoop run`, `calibrate` et `latency` utilisent cette carte et ce routage sans aucune
option. `--library` indique où chercher le modèle utilisé comme charge au test des buffers.

Pour essayer l'assistant sans matériel : `namfoop --backend sim --no-config setup`.

---

## 6. La bibliothèque de sons

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
#  [MESA _ BOOGIE BASS 800D]
#    m4jn   MESA BASS 800D - SEND - MODERN ROCK METAL  — amp bass | WaveNet 3ch + WaveNet 8ch | -17.5 dB
#           IR requise — conseillée : iwqk Mesa Powerhouse 410 (SM57 + MD421) (même marque (mesa), baffle basse)
#  ...
#  [1970 Fender Bassman 2x15 Cabinet Original CTS Speakers] 58 IR
#    idw7   1970 Bassman Cabinet CTS - 121 Lower - Cap Edge
```

Pour chaque modèle : son **identifiant**, le **type de capture**, le style, les **tailles**
disponibles, la loudness, et si une **IR** est nécessaire. Pour chaque IR : son identifiant.

### Faut-il une IR ?

| Type | Contenu de la capture | IR |
|---|---|---|
| `amp_cab` | ampli **et** baffle micro (la plupart des rigs Tone3000) | **non**, namfoop prévient si tu en ajoutes une |
| `amp` | ampli seul (ex. MESA 800D « SEND », Trace Elliot AH600SMX) | **oui** |
| `pedal` | pédale seule (ex. Rat) | oui, ou un autre modèle d'ampli plus tard |

Pour un modèle qui en a besoin, namfoop **conseille des IR**. Il cherche d'abord la même marque
(un ampli Mesa avec un baffle Mesa), et préfère les baffles basse (8x10, 4x10, 2x15…) aux baffles
guitare (4x12, V30…). Une seule IR par pack figure en tête de liste. `-i auto` prend la première
conseillée (ou aucune si le baffle est inclus). `run` sans `-i` sur un tel modèle affiche les
trois meilleures suggestions. Pour imposer ta préférée, mets `default_ir` dans la config
([§ 13](#13-le-fichier-de-configuration)).

```sh
namfoop run -m "800d modern" -i auto
```

### Désigner un modèle ou une IR

`-m` (`--model`) et `-i` (`--ir`) acceptent :

- un **identifiant** affiché par `namfoop models` (`m…` pour un modèle, `i…` pour une IR) : le plus
  sûr pour cibler un fichier précis. Il ne change pas quand tu ajoutes d'autres fichiers ;
- un **chemin** : `-m "library/nam/Darkglass B7K Ultra/Darkglass Drive - Microtubes B7K Ultra.nam"` ;
- des **mots-clés** : tous doivent apparaître dans le chemin, sans tenir compte des majuscules.
  Si plusieurs fichiers correspondent, namfoop départage dans l'ordre : les mots entiers dans le
  nom du fichier, l'expression exacte, les mots entiers dans le chemin, puis la version « de
  base ». Ainsi `angel` désigne « GP12SMX Angel », et `angel v2` désigne « GP12SMX Angel_v2 ». Si
  le doute subsiste, namfoop affiche les candidats **avec leur identifiant**.

```sh
namfoop run -m b7k
namfoop run -m "ultra lo sm57"
namfoop run -m "ah600smx angel v2" -i "1970 121 lower cap"
namfoop run -m "800d modern" -i "dg212 sm57"
```

### Choisir son son en jouant : `namfoop browse`

```sh
namfoop browse
```

Le navigateur plein écran **joue en direct**, avec la carte et le routage de `setup`. Le son
change **sans coupure ni clic**, par un fondu de 20 ms.

- **Onglet Modèles** : identifiant, badge « IR incluse » ou « IR requise », type de capture.
  ↑↓ écoute le modèle sous le curseur, après 150 ms d'arrêt (on peut défiler vite sans tout
  recharger). Pour un modèle sans baffle, la meilleure IR conseillée est posée d'office. Pour une
  capture qui inclut son baffle, l'IR est retirée.
- **Onglet IR** (Tab) : « aucune IR », puis les IR conseillées pour le modèle (★), puis toutes les
  autres. Dans un pack, le début commun des noms est masqué : on lit « 121 Lower - Cap » au lieu de
  « 1970 Bassman Cabinet CTS - 121 Lower - Cap ». **Espace** ajoute ou retire l'IR du mélange.
- **Onglet Mélange** : `+`/`-` règle le gain de l'IR choisie (par pas de 1 dB), `p` inverse sa
  polarité, `a` active ou coupe l'alignement automatique, `x` la retire. Chaque changement
  s'écoute aussitôt. `s` sauve le mélange dans `library/cabs/` (Entrée valide le nom proposé).
- **`/` filtre** la liste : tape des mots (`121 lower`), Entrée pour valider. Hors filtre, les
  lettres sont des commandes.
- En bas : le son en cours, les vumètres, la charge DSP et les xruns. `q` ou Échap quitte.

En quittant, le navigateur affiche la commande pour rejouer le même son, par exemple
`namfoop run -m m4jn -i c7k2`. Options : `--library`, `--size` (modèles légers), et le routage
`--bass-in`, `--mic-in`, `--route` ([§ 10](#10-backends-carte-et-routage-en-ligne-de-commande)).
Sans matériel : `namfoop --backend sim --no-config browse`.

### Mélanger plusieurs IR

Les packs multi-micros (ex. le Fender 1970 : 8 micros × haut-parleur haut/bas × 4 positions =
58 IR) se combinent comme en studio : un micro dynamique pour l'attaque, un ruban pour le corps.
Un **mélange** est un fichier `library/cabs/<nom>.cab.toml`, utilisable partout où l'on attend une
IR (`-i`, identifiant `c…`, ou mots-clés) :

```toml
name = "Fender 70 121 + Beta52"
align = true          # alignement temporel automatique (défaut : true)
max_ms = 200          # optionnel, comme --ir-ms
[[ir]]
file = "ir/1970 Fender Bassman 2x15 Cabinet Original CTS Speakers/1970 Bassman Cabinet CTS - 121 Lower - Cap.wav"
gain_db = 0.0
[[ir]]
file = "ir/1970 Fender Bassman 2x15 Cabinet Original CTS Speakers/1970 Bassman Cabinet CTS - Beta52 Lower - Cap.wav"
gain_db = -3.0
invert = false        # inversion de polarité
```

- `file` : relatif à la bibliothèque (ou au dossier du fichier), ou chemin absolu.
- `gain_db` : niveau de chaque IR dans le mélange.
- `align` : chaque IR est recadrée sur son départ puis ajustée par corrélation sur ±1 ms.
  Sans cela, deux micros à des distances différentes se combinent en filtre en peigne (creux dans
  le grave ou les médiums). Le décalage corrigé s'affiche au chargement. Les IR d'un même pack sont
  souvent déjà alignées : moins de 0,1 ms dans le Fender 1970.
- `invert` : inverse la polarité d'une IR. namfoop prévient si une IR semble en opposition de
  phase avec la première.

Le mélange est calculé **une seule fois au chargement**, puis normalisé comme une IR seule : en
live il ne coûte pas plus cher qu'une IR. Le plus simple pour en créer un : `namfoop browse`
(ci-dessus), à l'oreille. On peut aussi écrire le fichier à la main :

```sh
namfoop models                         # le mélange apparaît avec son identifiant c…
namfoop run -m "800d modern" -i <id du mélange>
```

### Taille des modèles (`--size`)

Les modèles récents (format NAM 0.7, « slimmable ») contiennent plusieurs tailles du même son :
`--size 0` choisit la plus légère (ex. 3 canaux), `--size 1` (défaut) la complète (ex. 8 canaux).
La version légère coûte environ deux fois moins de CPU : utile sur Raspberry Pi. Voir
`namfoop bench --all-sizes` ([§ 11](#11-travailler-sans-jouer--rendu-bench-inventaire)).

---

## 7. Calibrer sa basse

**Pourquoi.** Un modèle NAM a été entraîné avec le DI de la personne qui l'a capturé, à un niveau
numérique précis. Si ta basse arrive plus fort (basse active, gain de carte élevé), le modèle
sature beaucoup plus que sur les démos : un Bassman 800 « presque clair » devient très saturé.
Les modèles indiquent rarement leur niveau de calibration : on mesure donc **ton** niveau.

```sh
namfoop calibrate
```

1. Joue **fort**, comme sur tes passages les plus appuyés, pendant la durée indiquée (15 s par
   défaut, `--seconds` pour changer). Tu t'entends en direct sur les sorties « traité » du routage.
2. namfoop affiche ta crête et propose un **trim** :
   ```
   Crête max -9.8 dBFS, crête robuste -12.1 dBFS, RMS joué -24.0 dBFS
   Trim d'entrée proposé : -18.0 dB (crête robuste visée -30 dBFS à l'entrée des modèles)
   ```
3. Il sauve le trim, ton enregistrement et le mode de volume **match** dans
   `~/.config/namfoop/` ([§ 13](#13-le-fichier-de-configuration)). Toutes les commandes `run` et
   `render` les utilisent ensuite.

Options :

| Option | Rôle |
|---|---|
| `--seconds` | durée d'enregistrement |
| `--target` | crête visée à l'entrée des modèles, -30 dBFS par défaut. Plus haut (ex. -27) = modèles plus saturés |
| `--channel` | entrée de la basse, si elle diffère du routage (`setup`) |
| `--dry-run` | analyse seulement, ne sauve rien |

À refaire si tu changes de basse ou le gain de la carte. Si l'entrée sature pendant la
calibration, namfoop refuse : baisse le gain de la carte et recommence.

---

## 8. Jouer

```sh
namfoop run -m b7k
namfoop run -m "800d modern" -i "dg212 sm57"
```

La carte, le backend, le buffer et le routage viennent de `namfoop setup`. Ctrl+C arrête. Pour
changer de son, relance avec un autre `-m` (les presets et le MIDI viendront).

### La ligne d'état

```
  in  -14.2 dB  out  -9.8 dB  lim -1.2 dB  dsp   8.5 %  xruns 0
```

| Champ | Signification | Que faire |
|---|---|---|
| `in` | crête de l'entrée basse | vers -12 / -6 dB en jouant fort. `[CLIP ×n]` : baisse le gain de la carte |
| `out` | crête de la basse traitée | doit rester sous 0 dB |
| `lim` | réduction de gain du limiteur | s'il travaille souvent, baisse `--master-db` |
| `dsp` | part du temps disponible utilisée par le traitement (pic) | au-delà de 70 %, risque de craquements |
| `xruns` | décrochages audio depuis le démarrage | doit rester à 0 |
| `[CLIP SORTIE]` | échantillons écrêtés en sortie (limiteur désactivé) | baisse `--master-db` |

Au démarrage, une ligne `routage :` rappelle l'entrée de la basse et le rôle de chaque sortie. À
l'arrêt, un résumé donne la charge DSP maximale, les xruns et les éventuels écrêtages.

### Options de `run`

| Option | Rôle |
|---|---|
| `-m`, `--model` | modèle NAM ([§ 6](#6-la-bibliothèque-de-sons)) |
| `-i`, `--ir` | IR de baffle |
| `--bass-in`, `--mic-in`, `--route` | routage pour cette session ([§ 10](#10-backends-carte-et-routage-en-ligne-de-commande)) |
| `--seconds` | arrêt automatique (tests) |
| `-q`, `--quiet` | pas de ligne d'état |

Plus toutes les options de son du [§ 9](#9-régler-le-son-et-le-volume).

---

## 9. Régler le son et le volume

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

---

## 10. Backends, carte et routage en ligne de commande

L'assistant ([§ 5](#5-configurer-la-carte--lassistant)) enregistre ces réglages ; les options
ci-dessous les remplacent pour une session. Les **options globales** se placent **avant** la
commande (`namfoop --backend alsa --buffer 64 run -m b7k`).

### Backends

| | JACK (`--backend jack`) | ALSA direct (`--backend alsa`) | Simulé (`--backend sim`) |
|---|---|---|---|
| Exemple | `namfoop --backend jack --device iTwo run` | `namfoop --backend alsa --device hw:iTwo run` | `namfoop --backend sim run` |
| Latence mesurée (iTwo) | 6,5–7 ms, varie un peu | **5,2 ms, stable** | — |
| Partage de la carte | oui | non : namfoop la réserve | — |
| Usage conseillé | développement, mélange avec d'autres applis | live, Raspberry Pi | tests et démo sans matériel |

- `--device` : la carte. En JACK, son nom ou une partie (`iTwo`) ; en ALSA, le device (`hw:iTwo`,
  `hw:MTK`) ; en simulé, les canaux (`4x6` = 4 entrées, 6 sorties).
- `--buffer` : taille de période en frames. JACK n'accepte que des puissances de 2 (32, 64...).
  Avec l'iTwo : 48 en ALSA, 64 en JACK ; 32 et en dessous provoquent des xruns.
- `--periods` : nombre de périodes côté ALSA (3 par défaut, meilleur compromis mesuré).
- `--rate` : fréquence d'échantillonnage (48000 ; les modèles sont entraînés à 48 kHz). En JACK,
  c'est le serveur qui décide : namfoop refuse de démarrer s'il tourne à une autre fréquence.
- `--client-name` : nom du client JACK (pour lancer plusieurs instances).

**En ALSA, si PipeWire utilise la carte**, namfoop coupe son profil le temps de la session et le
remet à l'arrêt, y compris en cas de plantage. Seul un `kill -9` peut laisser le profil sur « off »
([§ 14](#14-dépannage)).

### Routage

- `--bass-in N` : entrée de la basse (à partir de 1).
- `--mic-in N` : entrée du micro (son traitement viendra ; pour l'instant le micro est envoyé brut).
- `--route N=SOURCE` : ce que reçoit la sortie N, répétable. SOURCE = `processed` (basse traitée),
  `di` (basse directe, pour un DI propre au sondier), `mic` (micro), `mute`. Les sorties non citées
  gardent le rôle de la config ; sans config, les sorties 1 et 2 reçoivent la basse traitée.

```sh
namfoop run -m b7k --route 2=di                     # sortie 2 : DI propre pour le sondier
namfoop run -m b7k --bass-in 2 --route 1=processed  # basse branchée sur l'entrée 2
```

### Config

`--config` choisit un autre fichier de configuration, `--no-config` l'ignore complètement
(réglages par défaut : pratique pour comparer ou pour les tests automatiques ; rien n'est
enregistré).

---

## 11. Travailler sans jouer : rendu, bench, inventaire

### `namfoop render` : traiter un fichier

Même chaîne que `run`, sur un WAV :

```sh
namfoop render -m b7k --in di.wav -o b7k.wav
namfoop render -m "800d modern" -i "dg212 sm57" -o test.wav
```

- `--in` : WAV d'entrée (sans : signal de test fourni avec le moteur NAM) ;
- `-o`, `--out` : WAV de sortie (flottant 32 bits). Sans : affiche seulement les niveaux ;
- `--block` : taille de bloc (le son ne dépend pas de ce réglage, c'est vérifié par les tests).

Toutes les options de son du [§ 9](#9-régler-le-son-et-le-volume) s'appliquent.

### `namfoop bench` : coût CPU

```sh
namfoop bench --all-sizes                          # toute la bibliothèque, chaque taille
namfoop bench --block 48 b7k "800d modern"         # quelques modèles
```

Affiche le temps moyen et maximal par bloc, et la part du budget temps réel. `--block`,
`--seconds` (durée simulée) et `--library` sont réglables ; `--all-sizes` mesure chaque taille des
modèles slimmable. À lancer sur la machine de scène (ex. le Pi) pour savoir quels modèles tiennent.

### `namfoop models` et `namfoop ports`

`namfoop models` liste la bibliothèque ([§ 6](#6-la-bibliothèque-de-sons)). `namfoop ports` liste
les ports JACK audio et MIDI.

---

## 12. Mesurer la latence

Relie une **sortie** à une **entrée** avec un câble (par défaut sortie 1 -> entrée 2), gain de
l'entrée bas, bouton Mixer côté Playback, casque bas (la mesure émet des clics) :

```sh
namfoop latency
namfoop --backend alsa --device hw:iTwo --buffer 48 latency --count 20
```

`--out-ch` / `--in-ch` choisissent les canaux, `--count` le nombre de mesures. Le résultat est
l'aller-retour complet (conversion de la carte comprise).

---

## 13. Le fichier de configuration

`~/.config/namfoop/config.toml`, écrit par `setup` et `calibrate`, modifiable à la main :

```toml
trim_db = -18.0                      # trim d'entrée de l'instrument (dB)       — calibrate
level = "match"                      # "match", "normalize" ou "raw"           — calibrate
target_db = -18.0                    # cible du mode normalize
limiter = true                       # limiteur de sortie
calibration_wav = "calibration.wav"  # enregistrement de calibration (relatif à ce dossier)
default_ir = "dg212 sm57"            # optionnel : IR préférée (identifiant ou mots-clés), -i auto

[audio]                              # — setup
backend = "alsa"
device = "hw:iTwo"
buffer = 48
periods = 3
rate = 48000

[routing]                            # — setup
bass_in = 1
mic_in = 2                           # optionnel

[[routing.outputs]]
channel = 1
source = "processed"

[[routing.outputs]]
channel = 2
source = "di"
```

Les options de la ligne de commande priment toujours sur ce fichier.

---

## 14. Dépannage

**Le son est beaucoup plus saturé que la démo Tone3000.** Niveau d'entrée trop chaud : lance
`namfoop calibrate`, ou essaie `--input-db -12`.

**Le son traité est bien plus fort que le son clair.** Passe en `--level match` (après
calibration), ou baisse `--master-db`.

**`[CLIP]` sur l'entrée.** Le préampli de la carte sature avant namfoop : baisse le gain de
l'entrée de la carte (aucun réglage logiciel ne rattrape cette saturation).

**Craquements, xruns.** Relance `namfoop setup` (étape buffer), augmente `--buffer` (ALSA : 64),
utilise le backend ALSA, ferme les applications lourdes, ou choisis un modèle plus léger
(`--size 0`).

**Pas de son sur une sortie.** Vérifie la ligne `routage :` au démarrage de `run`, ou refais
l'étape sorties de `namfoop setup` avec le bip (`t`).

**« entrée N inexistante » / « sortie N inexistante ».** La config vise une autre carte que celle
ouverte : relance `namfoop setup`.

**Larsen ou son « en double », filtré.** Bouton Mixer de l'iTwo pas à fond côté Playback.

**« occupé par une autre application » en ALSA.** Une autre application (ou un autre namfoop)
tient la carte. Ferme-la, ou utilise le backend JACK.

**Plus de son PipeWire sur l'iTwo après un plantage dur.** Le profil est resté sur « off » :
`pactl set-card-profile <nom de la carte> pro-audio`.

**Clic ou coupure en changeant de son dans `browse`.** Ce n'est pas normal (le fondu est de 20 ms) :
note le modèle et l'IR concernés et ouvre une issue.

**« … est ambigu ».** Plusieurs fichiers correspondent aux mots-clés : namfoop affiche la liste
avec les identifiants. Ajoute un mot, ou donne l'identifiant (`-m m4jn`).

**« capture amp sans baffle : ajoute une IR ».** Le modèle ne contient pas de baffle : sans IR, le
son est très agressif. Ajoute `-i auto` ou une des IR conseillées.

**Le son de l'ordinateur n'arrive pas dans le casque de la table.** Voir le
[§ 2](#2-matériel-et-câblage) : le retour USB doit passer par une tranche qui alimente l'aux.

---

## 15. Limites actuelles

namfoop est un prototype. Pas encore disponibles : effets (gate, compresseur, EQ, crossover,
modulation, delay, réverbe), presets, changement de son par MIDI, traitement du micro, démarrage
automatique, version Raspberry Pi. Voir le [CHANGELOG](../CHANGELOG.md) pour suivre les versions.
