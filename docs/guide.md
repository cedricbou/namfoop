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
library/nam/<pack>/…/*.nam   modèles
library/ir/<pack>/…/*.wav    réponses impulsionnelles de baffle
library/docs/<pack>/…        notices des packs (README…)
library/cabs/*.cab.toml      tes mélanges d'IR
library/overrides.toml       tes corrections de métadonnées
```

Ce dossier n'est jamais versionné : les droits des modèles et IR (Tone3000, packs achetés)
appartiennent à leurs auteurs.

### Importer des sons : `namfoop import`

Donne-lui des zips, des dossiers ou des fichiers `.nam` / `.wav`, tels que téléchargés :

```sh
namfoop import ~/Téléchargements/PopPunkBass_NAMA2.zip
namfoop import nam/                      # chaque zip du dossier devient un pack
namfoop import --dry-run nam/komposition101
namfoop import --pack "Mes IR" cab1.wav cab2.wav
```

Pour chaque fichier, `import` :

1. **range** : modèles dans `library/nam/<pack>/`, IR dans `library/ir/<pack>/`, notices dans
   `library/docs/<pack>/`. Le pack porte le nom du zip ou du dossier (ou `--pack`). Les
   sous-dossiers du pack sont gardés (`DI/`, `Full-Rig/`…), et le dossier racine du zip est retiré.
   Les fichiers parasites (`desktop.ini`, `__MACOSX`…) sont ignorés ;
2. **repère les fichiers identiques** : un fichier dont le **contenu** est exactement celui d'un
   fichier déjà dans la bibliothèque (ou ailleurs dans l'import) est **importé quand même** et
   signalé, avec le fichier dont il est la copie. Chaque pack reste ainsi complet : une série de
   micros d'un baffle ne perd pas un de ses fichiers parce qu'un autre pack contient le même.
   `--skip-duplicates` ne les importe pas (bibliothèque sans copie, mais packs incomplets) ;
3. **vérifie** : chaque modèle est chargé et joué (refusé s'il est illisible, muet ou s'il produit
   des valeurs invalides). Chaque IR est lue, et refusée si elle est vide, muette ou dure plus de
   10 s (une démo, pas une IR) ;
4. **détermine le type de capture** (baffle inclus ou non, voir ci-dessous) et corrige les
   métadonnées fausses ;
5. **retient l'IR du pack** : si le pack fournit des IR, la première (un mélange « Mix » de
   préférence) devient l'**IR préférée** de ses modèles sans baffle ;
6. **résume** : identifiant, type, IR conseillée, fichiers identiques, refus et points à vérifier.

Relancer un import ne change rien : chaque fichier déjà à sa place avec le même contenu est compté
« déjà présent » et laissé tel quel. Un fichier différent qui porterait le même nom est refusé
(rien n'est jamais écrasé). `--dry-run` fait toutes les
vérifications sans rien écrire. `--review` ouvre ensuite le navigateur sur les nouveaux modèles,
éditeur ouvert, pour les écouter et les corriger ou les calibrer en jouant
([`browse`, touche `e`](#modifier-et-calibrer-un-modèle-e)). Il prend la carte son.

**Comment deux fichiers sont reconnus identiques.** namfoop calcule une empreinte de chaque fichier
(hash de **tous ses octets**) pour trouver les candidats, puis compare les deux fichiers **octet
par octet**. « Identique » veut donc dire exactement le même fichier, quel que soit son nom ou son
pack : `AMPG_SVT410_Mix1.wav` d'un pack et `AMPG 410 Mix 1.wav` d'un autre sont reconnus. À
l'inverse, la même IR réexportée (autre format, 24 bits au lieu de 32, métadonnées ajoutées) n'est
**pas** considérée identique : les deux sont importées sans remarque, et c'est à l'écoute de
trancher. Des fichiers identiques au sein d'un même pack existent aussi (erreur de l'éditeur :
dans *Massive Bass Pack 2*, `Cab8_Mix2` est la copie exacte de `Cab10_Mix3`) : l'import le signale.

Pour les modèles, une copie identique partage les corrections de l'original (`library/overrides.toml`
est indexé par contenu) : corriger l'un corrige l'autre.

**Type de capture.** Les métadonnées des fichiers sont souvent incomplètes ou fausses. Certains packs
achetés déclarent `amp` (sans baffle) même pour leurs captures FullRig, ou laissent des valeurs
de remplissage (`T3K-Null`, `tz-make`), que namfoop traite comme absentes. `import` décide dans
cet ordre :
- le **nom du fichier** : `DI`, `FullRig`, `Full Rig`, `amp cab`, `no cab`… ;
- puis les **dossiers** : `Full-Rig Profiles/`, `DI/`… ;
- puis les **métadonnées**.

Quand le résultat contredit les métadonnées, la correction est enregistrée et marquée « → corrigé ».
Quand rien ne permet de trancher, le modèle est listé « à vérifier » avec la commande pour le
corriger. Le plus simple est de l'écouter et de le corriger à l'écran : `namfoop browse`, touche `e`.

Les fichiers d'origine ne sont **jamais modifiés** : les corrections vont dans
`library/overrides.toml`, rangées sous l'**empreinte du contenu** de chaque modèle. Elles
survivent à un renommage, à un déplacement ou à un réimport du pack.

### Corriger un modèle : `namfoop meta`

`namfoop meta` affiche ce que namfoop sait d'un modèle (valeur du fichier entre parenthèses quand
elle a été corrigée), et le corrige :

```sh
namfoop meta <id>                               # afficher
namfoop meta <id> --type amp-cab                # baffle inclus : pas d'IR
namfoop meta fullrig --all --type amp-cab       # tous les modèles dont le chemin contient « fullrig »
namfoop meta <id> --input-db -6                 # ce modèle sature trop : -6 dB à son entrée
namfoop meta <id> --ir "ampg 410 mix 1"         # son IR préférée
namfoop meta <id> --reset                       # oublier les corrections
```

- `--type` : `amp` (ampli seul), `amp-cab` (ampli + baffle), `full-rig`, `pedal`, `preamp`.
  Décide si une IR est nécessaire ;
- `--make` : la marque de l'ampli, utilisée pour conseiller les IR de la même marque ; `--name` :
  le nom affiché ;
- `--input-db` : un gain d'entrée **propre au modèle**, ajouté à `--input-db` de `run` / `browse`.
  Pratique pour aligner des packs calibrés différemment ; `--input-db 0` le retire ;
- `--ir` : l'**IR préférée** du modèle, en tête des conseils et choisie par `-i auto` ;
- `--all` : applique la correction à **tous** les modèles qui contiennent les mots-clés (sans
  `--all`, les mots-clés doivent désigner un seul modèle) ;
- `--reset` : efface les corrections du ou des modèles.

`namfoop models` signale les modèles corrigés (« corrigé : type, IR »).

### Tutoriel : d'un zip acheté à un son prêt pour la scène

Exemple suivi : les packs komposition101 rangés dans `nam/komposition101/` (deux packs de modèles
avec versions DI et FullRig, un pack d'IR). La méthode vaut pour tout pack, Tone3000 compris.

**Avant de commencer** : `namfoop setup` et `namfoop calibrate` sont faits
([§ 5](#5-configurer-la-carte--lassistant), [§ 7](#7-calibrer-sa-basse)). La calibration sert de
référence à tous les réglages d'entrée ci-dessous.

#### Import ou revue : deux étapes distinctes

| | `namfoop import` | la revue (`browse`, touche `e`) |
|---|---|---|
| Rôle | ranger et **vérifier automatiquement** les fichiers | **écouter** et ajuster à l'oreille |
| Carte son | non utilisée | prise (le son joue) |
| Ce qu'il décide | rangement, fichiers identiques signalés, fichiers refusés, type de capture d'après le nom, IR fournie avec le pack | type de capture douteux, gain d'entrée propre au modèle, IR préférée, nom, marque |
| Quand | une fois par pack | après l'import, ou n'importe quand plus tard |
| Obligatoire ? | oui | non : sans revue, les modèles sont jouables avec les réglages par défaut |

`namfoop import --review` enchaîne les deux : il importe, puis ouvre le navigateur sur les
nouveaux modèles, éditeur ouvert. C'est un raccourci, pas une autre façon d'importer. La revue se
refait ensuite à tout moment avec `namfoop browse` (touche `e`) ou `namfoop meta`.

#### Étape 1 — Voir ce que l'import va faire

```sh
namfoop import --dry-run nam/komposition101
```

Rien n'est écrit. Lis le résumé, pack par pack :

```
[PopPunkBass_NAMA2] modèles : 52 nouveaux · IR : 13 nouvelles · docs : 2 · déjà présents : 0 · refusés : 0
      -      DI/PPB_ChunkyBass_DI.nam  — sans baffle (nom « di »)
      -      Full-Rig/PPB_ChunkyBass_FullRig.nam  — baffle inclus (nom « fullrig ») → corrigé
    13 fichier(s) identique(s) octet par octet à un autre, importé(s) pour garder le pack complet :
      IR Bass_Variety_Starter_Pack/…/AMPG_SVT410_Mix1.wav  =  ir/EverythingBassIRBundle/K101_AMPG_SVT4x10/Mixes/AMPG 410 Mix 1.wav
```

- **sans baffle / baffle inclus** et entre parenthèses **d'où vient la décision** (nom du fichier,
  dossier, ou métadonnées) ;
- **→ corrigé** : les métadonnées du fichier disaient autre chose (ici `amp` pour un FullRig) ;
  la correction sera enregistrée ;
- **⚠ à vérifier** : rien ne permet de trancher, il faudra écouter (étape 3) ;
- **identiques** : copies exactes d'un autre fichier (souvent les mêmes IR livrées dans plusieurs
  packs), importées pour que le pack reste complet, avec le fichier d'origine en regard ;
- **déjà présents** : fichiers déjà importés à cette place (réimport) ;
- **refusés** : fichiers illisibles ou muets, avec la raison.

#### Étape 2 — Importer

```sh
namfoop import nam/komposition101
```

Le même résumé s'affiche, avec les **identifiants** (`m…`) et l'IR conseillée de chaque modèle
sans baffle. Les notices des packs sont copiées dans `library/docs/<pack>/` : lis-les, elles
donnent parfois le niveau de calibration ou les intentions des sons.

Le pack est utilisable dès maintenant : `namfoop run -m <id> -i auto`. La suite sert à affiner.

#### Étape 3 — Vérifier le type de capture (si l'import en a signalé)

Ouvre le navigateur filtré sur le pack, puis l'éditeur :

```sh
namfoop browse --filter "PopPunk"
```

Place-toi sur le modèle, `e`, descends sur **IR préférée** et fais défiler avec ←→ : « aucune »,
puis les IR conseillées, qui jouent au fur et à mesure.
- Le son est **fini sans IR** et devient **sourd, étouffé** avec une IR : le baffle est inclus.
  Remonte sur **Type de capture**, choisis `amp_cab`.
- Le son est **criard, bourdonnant, « râpeux » sans IR** et naturel avec : il n'y a pas de baffle.
  Choisis `amp`.

`s` enregistre. Le modèle porte alors la marque ✎ dans la liste.

#### Étape 4 — Trouver le bon gain d'entrée

Le **gain d'entrée** décide du **grain** (clair, crunch, saturé). Il s'additionne en trois
couches, de la plus globale à la plus fine :

```
basse ─▶ trim (calibrate)  ─▶ --input-db (session)  ─▶ entrée du modèle (e / meta)  ─▶ modèle NAM ─▶ IR
         toute la config       une commande run         ce modèle, enregistrée
```

| Réglage | Portée | Où il est rangé | Sert à |
|---|---|---|---|
| trim | ta basse + ta carte, tous les modèles | config (`calibrate`) | ramener ta basse au niveau de référence (crêtes à -30 dBFS à l'entrée des modèles) |
| `--input-db` | une session | rien (option) | essayer, ou pousser un morceau précis |
| entrée du modèle | un modèle, ou un pack avec `a` / `--all` | `library/overrides.toml` | corriger un modèle calibré différemment des autres |

**0 dB dans l'éditeur veut dire « ta basse arrive au niveau de la calibration ».** C'est le bon
point de départ : avec les captures Tone3000, il redonne le son des démos (le Bassman 800 reste
presque clair). Les packs vendus ne disent pas toujours à quel niveau ils ont été capturés : c'est
là que l'oreille décide.

**Méthode**, modèle par modèle (ou un modèle représentatif du pack) :
1. Pars de l'**intention du son** : son nom (`DeepAndClean` doit rester clair, `BrightAndHighGain`
   doit saturer), la notice, ou la démo du vendeur si elle existe.
2. Dans l'éditeur, sur **Entrée**, joue ton passage **le plus appuyé**. Un son clair qui casse :
   ← (moins). Un son saturé qui manque de mordant : → (plus). Par pas de 1 dB, ça s'entend
   immédiatement.
3. Vérifie en jouant **doucement** : le grain doit suivre ton attaque comme sur un vrai ampli.
4. `s` enregistre.

Repères : les corrections utiles tiennent en général entre -12 et +6 dB. Au-delà, le problème est
ailleurs : recalibre (`namfoop calibrate`) ou vérifie le gain de la carte. Si **tous** les modèles
te semblent trop saturés, ne corrige pas modèle par modèle : relance `calibrate`, ou baisse
`--target` de calibration.

#### Et le volume de sortie ?

**Tu n'as rien à régler par modèle.** En mode `level = "match"` (celui que `calibrate` active),
namfoop recalcule le volume **à chaque chargement**. Ton enregistrement de calibration passe dans
la chaîne complète (entrée, modèle, IR), et un gain ramène le résultat au volume de ta basse en
direct. Conséquences :

- changer l'entrée change le **grain**, pas le volume ;
- `--output-db` est sans effet en mode `match`, puisque l'alignement vient après lui. Il ne sert
  qu'en `normalize` et `raw` ;
- les conseils du type « monte la sortie de 8 à 10 dB » des notices (komposition101 par exemple)
  visent les lecteurs sans alignement : ignore-les ici ;
- le volume général se règle avec `--master-db`, et `lim` en bas du navigateur montre si le
  limiteur travaille (s'il est souvent actif, baisse le master, pas l'entrée).

Limite actuelle : l'alignement égalise l'énergie (RMS). À énergie égale, un son clair peut
paraître un peu moins fort qu'un son saturé. Le temps qu'un volume propre à chaque modèle arrive
([issue #25](https://github.com/cedricbou/namfoop/issues/25)), compense avec `--master-db`.

#### Étape 5 — Corriger en masse

**À l'écran.** Le filtre (`/`) choisit les modèles ; **ce qui est visible est ce qui sera
modifié**. Ouvre l'éditeur sur l'un d'eux (`e`), change le type **ou** l'entrée, puis `a` :
seuls les champs que tu as changés sont recopiés sur tous les modèles visibles. Les autres champs
de chaque modèle (son type, son IR…) restent les siens. Exemples :
- tout un pack un peu trop chaud : `/` `poppunk`, `e`, Entrée à -3, `a` ;
- les deux versions d'un même son (même ampli, même réglage d'entrée en général) :
  `/` `chunkybass`, `e`, réglage, `a` ;
- des FullRig mal déclarés : `/` `fullrig`, `e`, type `amp_cab`, `a`.

Attention aux mots courts : le filtre cherche des **morceaux** de mots, et `di` trouve aussi
« **Di**rty ». Regarde la liste avant d'appuyer sur `a`.

**En ligne de commande**, avec `namfoop meta … --all` : les mots-clés désignent tous les modèles
dont le **chemin** les contient. Lance d'abord la commande **sans option** pour voir la liste
concernée, puis avec la correction :

```sh
namfoop meta "poppunk" --all                     # liste ce qui sera touché
namfoop meta "poppunk" --all --input-db -3       # applique
namfoop meta "zeroamps full-rig" --all --type amp-cab
```

Contrairement à `a` à l'écran, `meta --all` applique **toutes** les options données.

#### Étape 6 — Choisir l'IR des modèles sans baffle

Pour t'orienter dans un pack d'IR numérotées, lis leurs **étiquettes**
([profil sonore](#profil-sonore-des-ir)) : `namfoop models`, ou l'onglet IR de `browse`. Commence
par les extrêmes (la plus « sub + », la plus « brillant », la plus « sombre ») pour situer le pack,
puis affine à l'oreille entre les IR voisines. Dans `browse`, épingle ta favorite du moment avec
`c` et compare chaque candidate avec `r` (A/B à volume égal) en regardant les écarts sous le
spectre. Pour écouter les IR seules, sur le son direct de la
basse, choisis la ligne « son direct (DI) » de l'onglet Modèles.

L'import a déjà retenu l'IR fournie avec le pack comme **IR préférée** (`-i auto` la prend).
Pour en changer : dans l'éditeur, champ **IR préférée**, ←→ parmi les conseillées. Ou choisis
n'importe quelle IR dans l'onglet IR (y compris un de tes mélanges), reviens sur le modèle, `e`,
Entrée sur **IR préférée**. En ligne de commande : `namfoop meta <id> --ir "<mots-clés>"`.

#### Étape 7 — Vérifier et jouer

```sh
namfoop meta <id>                 # ce qui est retenu : type, entrée, IR (valeur du fichier entre parenthèses)
namfoop run -m <id> -i auto
```

Au démarrage, `run` affiche ce qu'il applique : `entrée propre au modèle -3.0 dB`, puis la ligne
`NAM …` avec l'entrée **totale** (trim + `--input-db` + entrée du modèle), puis
`niveau aligné sur le DI : +x dB`.

Pour repartir de zéro sur un modèle : `namfoop meta <id> --reset`. Tes corrections ne touchent
jamais aux fichiers achetés, et survivent à un réimport.

### Inventaire : `namfoop models`

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
disponibles, la loudness, et si une **IR** est nécessaire. Pour chaque IR : son identifiant, puis
son **profil sonore** (ci-dessous). Les fichiers sont classés dans l'ordre naturel (Mix 2 avant
Mix 10).

### Profil sonore des IR

Dans un pack, les IR portent souvent de simples numéros (« AMPG 410 Mix 1 » à « Mix 11 »).
namfoop mesure la réponse en fréquence de chacune, **telle qu'elle est jouée** (tronquée et
normalisée comme à l'écoute), et la résume :

```
    i3j9   AMPG 410 Mix 11  ▆███▇▆▃▅▄▃▁▁  air +4, haut-médium −2
    ijep   AMPG 410 Mix 3   ▅███▆▅▃▅▄▃▁▁  présence −3, sub −2, aigus coupés à 3,9 kHz
    i0xe   AMPG 410 Mix 9   ▇██▇▆▅▃▅▄▄▂▁  sub +4
```

- Le **mini-spectre** a 12 colonnes, du grave (40 Hz) à gauche à l'aigu (13 kHz) à droite. Chaque
  colonne représente deux tiers d'octave. Les baffles basse ont tous une bosse dans le grave et
  une chute dans l'aigu : c'est la forme générale qui se lit d'un coup d'œil.
- Les **étiquettes** disent ce qui distingue l'IR **des autres IR de son pack**, par écart à leur
  médiane. Un pack de moins de 3 IR est comparé à toute la bibliothèque. Les zones : sub
  (40–80 Hz), grave (80–200), bas-médium (200–500), médium (0,5–1,5 kHz), haut-médium
  (1,5–3,5 kHz), présence (3,5–7 kHz), air (7–14 kHz). Un écart s'affiche à partir de 1,5 dB.
  « brillant » et « sombre » résument présence et air ensemble ; « médium en avant » et « médium
  creusé » résument le médium. « aigus coupés à 3,9 kHz » ou « aigus jusqu'à 7,4 kHz » indiquent
  une étendue dans l'aigu (point à -12 dB) qui diffère d'un tiers d'octave ou plus de celle du pack.
  « dans la moyenne » : rien ne ressort.

Ce sont des **repères pour choisir quoi écouter**, pas un jugement : deux IR proches sur le papier
peuvent sonner différemment (phase, résonances fines). Le profil est calculé à l'import et gardé
dans `library/.cache/ir-profiles.json`, rangé par empreinte du contenu. Les IR déjà présentes
sont profilées au premier `models` ou `browse` (environ 1 s pour 250 IR), puis tout est lu dans le
cache. Ce fichier peut être supprimé sans risque : il se reconstruit.

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
trois meilleures suggestions. Pour imposer ta préférée partout, mets `default_ir` dans la config
([§ 13](#13-le-fichier-de-configuration)). Pour un seul modèle, utilise `namfoop meta <id> --ir …`
(l'IR préférée du modèle passe avant tout le reste).

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
  capture qui inclut son baffle, l'IR est retirée. La première ligne, **son direct (DI, sans
  modèle)**, joue ta basse telle quelle : choisis-la puis une IR dans l'onglet IR pour **écouter
  une IR seule** (comparer les micros d'un pack, préparer un mélange). L'IR en cours est gardée
  quand tu passes sur cette ligne. Elle disparaît quand un filtre est actif.
- **Onglet IR** (Tab) : « aucune IR », puis les IR conseillées pour le modèle (★), puis toutes les
  autres. Dans un pack, le début commun des noms est masqué : on lit « 121 Lower - Cap » au lieu de
  « 1970 Bassman Cabinet CTS - 121 Lower - Cap ». Chaque IR affiche son mini-spectre et ses
  étiquettes ([profil sonore](#profil-sonore-des-ir)). **Espace** ajoute ou retire l'IR du mélange.
- **Panneau Spectre** (onglets IR et Mélange, terminal d'au moins ~130 colonnes) : à droite de
  la liste, la courbe de l'IR sous le curseur. Dans l'onglet Mélange, s'y ajoute la courbe du
  **mélange** résultant, où se voient les annulations entre micros. **`c` épingle** l'IR sous le
  curseur comme **référence** (◆, courbe jaune) ; sous le graphique, les écarts par zone entre
  l'IR et la référence (« sub +4 · … · air −2 »). `c` sur la référence la retire.
- **Écoute A/B**, dans tous les onglets et dans l'éditeur :
  - **`b`** coupe ou remet l'IR, pour entendre le modèle (ou ta basse) avec et sans baffle ;
  - **`r`** bascule entre ton choix (A) et la référence épinglée (B), sans perdre la sélection.

  L'en-tête affiche « [A/B : IR coupée — b] » ou « [A/B : B = ◆ … — r] ». En mode `match`, le
  volume est réaligné à chaque bascule : la comparaison se fait **à volume égal**, l'oreille juge
  le timbre et non le niveau. Choisir une autre IR ou un autre modèle met fin à l'A/B.
- **Onglet Mélange** : `+`/`-` règle le gain de l'IR choisie (par pas de 1 dB), `p` inverse sa
  polarité, `a` active ou coupe l'alignement automatique, `x` la retire. Chaque changement
  s'écoute aussitôt. `s` sauve le mélange dans `library/cabs/` (Entrée valide le nom proposé).
- **`/` filtre** la liste : tape des mots (`121 lower`), Entrée pour valider. Hors filtre, les
  lettres sont des commandes.
- **`e` modifie et calibre** le modèle sous le curseur, sans couper le son (voir ci-dessous). Un
  modèle corrigé est marqué ✎.
- En bas : le son en cours, les vumètres, la réduction du limiteur (`lim`), la charge DSP et les
  xruns. `q` ou Échap quitte.

En quittant, le navigateur affiche la commande pour rejouer le même son, par exemple
`namfoop run -m m4jn -i c7k2`. Options : `--library`, `--size` (modèles légers), `--filter`
(filtre de départ, ex. `--filter PopPunk`), et le routage
`--bass-in`, `--mic-in`, `--route` ([§ 10](#10-backends-carte-et-routage-en-ligne-de-commande)).
Sans matériel : `namfoop --backend sim --no-config browse`.

#### Modifier et calibrer un modèle (`e`)

L'éditeur s'ouvre sur le modèle sous le curseur, qui continue de jouer. ↑↓ choisit un champ,
←→ le change, **et le changement s'entend tout de suite** :

| Champ | ←→ | Pour quoi faire |
|---|---|---|
| Type de capture | `amp`, `amp_cab`, `full_rig`, `pedal`, `preamp` | dire si le baffle est inclus (IR ou non) |
| Entrée | ±1 dB | régler la saturation propre à ce modèle ; s'ajoute à `--input-db` |
| IR préférée | aucune, puis les conseillées, jouées au fur et à mesure | l'IR par défaut du modèle (`-i auto`, conseils) ; Entrée prend l'IR choisie dans l'onglet IR, `x` l'efface |
| Marque, Nom | Entrée pour saisir | conseils d'IR de la même marque, affichage |

La valeur lue dans le fichier est rappelée en regard (« fichier : amp »). **`s` enregistre** dans
`library/overrides.toml`, comme `namfoop meta` ; le `.nam` n'est jamais modifié. **`a` applique**
ce que tu as changé (le type et/ou l'entrée) à **tous les modèles visibles** (ceux du filtre) :
filtre `/fullrig`, `e`, type `amp_cab`, `a`, et tout le pack est corrigé ; les champs que tu n'as
pas changés restent ceux de chaque modèle. Échap ferme sans enregistrer et revient au réglage
enregistré. Pas à pas : [tutoriel d'import](#tutoriel--dun-zip-acheté-à-un-son-prêt-pour-la-scène).

**Calibrer à l'oreille.** Les packs ne sont pas calibrés au même niveau d'entrée (Tone3000,
packs achetés…). Joue comme sur scène et règle l'entrée jusqu'au grain voulu, en surveillant `out`
et `lim` en bas. Pour comparer avec un autre modèle, ferme l'éditeur, passe au modèle suivant et
reviens. Le volume, lui, est aligné par `--level`.

**Baffle inclus ou pas ?** Appuie sur **`b`** pour couper et remettre l'IR (ou passe l'IR préférée
sur « aucune », puis sur une IR conseillée). Une
capture **avec** baffle sonne finie sans IR, et sourde avec (deux baffles). Une capture **sans**
baffle est criarde et bourdonnante sans IR. Règle le type en conséquence. Le son seul ne permet
pas de le deviner automatiquement : les baffles basse ont souvent un tweeter.

**Après un import** : `namfoop import --review <zip>` importe puis ouvre directement le navigateur
sur les nouveaux modèles, éditeur ouvert.

```sh
namfoop import --review ~/Téléchargements/PopPunkBass_NAMA2.zip
namfoop browse --filter "PopPunk"
```

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
  in   -9.3 dB (calib. -8.5) → modèle -31 dB  out  -10.2 dB  lim -1.2 dB  dsp   8.5 %  xruns 0
```

| Champ | Signification | Que faire |
|---|---|---|
| `in` | crête **brute** de l'entrée basse de la carte, avant tout traitement | voir « Lire les vumètres » ci-dessous. `[CLIP ×n]` : baisse le gain de la carte |
| `(calib. …)` | crête attendue d'après ta calibration (cible − trim) | en jouant fort, `in` doit l'atteindre |
| `→ modèle` | ce que reçoit vraiment le modèle : `in` + trim + entrée (`--input-db`, entrée propre au modèle) | autour de -30 dB en jouant fort, après `calibrate` |
| `out` | crête de la sortie traitée, après le limiteur | ne dépasse jamais -1 dB avec le limiteur |
| `lim` | réduction de gain du limiteur | s'il travaille souvent, baisse `--master-db` |
| `dsp` | part du temps disponible utilisée par le traitement (pic) | au-delà de 70 %, risque de craquements |
| `xruns` | décrochages audio depuis le démarrage | doit rester à 0 |
| `[CLIP SORTIE]` | échantillons écrêtés en sortie (limiteur désactivé) | baisse `--master-db` |

Au démarrage, une ligne `routage :` rappelle l'entrée de la basse et le rôle de chaque sortie, et
une ligne `calibration :` la crête attendue sur `in`. À l'arrêt, un résumé donne la charge DSP
maximale, les xruns et les éventuels écrêtages.

### Lire les vumètres

Les deux vumètres (ligne d'état de `run`, bas de l'écran de `browse`) ne mesurent pas le même
endroit de la chaîne :

```
carte ──▶ [in] ──▶ trim ──▶ entrée ──▶ modèle ──▶ IR ──▶ niveau (match) ──▶ master ──▶ limiteur ──▶ [out]
                    (calibrate)  (--input-db, e)
```

**`in` : ta basse telle que la donne la carte**, avant le trim. Ce n'est **pas** ce que reçoit le
modèle. Après `calibrate`, le repère `┃` sur la barre de `browse` (ou `(calib. …)` dans `run`)
marque la crête mesurée pendant la calibration. Par exemple, avec un trim de -21,5 dB et la cible
de -30 dBFS, le repère est à -8,5 dBFS.
- En jouant **fort**, la barre atteint le repère, souvent en jaune (au-dessus de -12) : **c'est
  normal**. Le trim ramène ensuite ta basse à environ -30 dBFS à l'entrée du modèle, ce
  qu'affiche `→ modèle`.
- En jouant doucement, la barre descend bien en dessous (-25 à -35 dBFS).
- **Rouge ou `[CLIP]`** : le préampli de la carte sature, et aucun réglage logiciel ne rattrape ça.
  Baisse le gain de la carte, **puis relance `calibrate`**.
- **Toujours loin du repère** en jouant comme d'habitude (5 dB ou plus en dessous) : le gain de la
  carte ou la basse ont changé depuis la calibration. Relance `calibrate`, sinon les modèles
  saturent moins que prévu (ou plus, dans le cas inverse).

**`out` : ce qui part vers les sorties « traité »**, après toute la chaîne.
- En mode `match`, le volume est aligné sur ta basse en direct : `out` reste **du même ordre que
  `in`**. Il est souvent un peu plus bas sur un son saturé, plus compressé, ou avec une IR sombre.
  Ça ne veut pas dire que le son est plus faible à l'oreille.
- Il ne dépasse jamais -1 dBFS : le limiteur l'en empêche, et `lim` montre de combien il réduit.
  Si `lim` travaille en permanence, baisse `--master-db`. Ne baisse pas l'entrée : elle change le
  grain, pas le volume.

Échelle : de -60 à 0 dBFS. Vert en dessous de -12, jaune de -12 à -3, rouge au-dessus de -3. La
barre garde la crête et retombe en douceur. Sans calibration, il n'y a pas de repère.

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
s'ajoute au trim de calibration et à l'entrée propre du modèle (enregistrée avec `namfoop meta`
ou l'éditeur de `browse`), pour cette session seulement.

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

`--master-db` règle le volume final. `--output-db` ajoute un gain juste après le modèle : utile
en `normalize` et `raw`, mais **sans effet en `match`**, dont l'alignement vient après. Le grain
d'un modèle se règle par l'entrée (`--input-db`, ou l'entrée propre au modèle :
[tutoriel d'import](#tutoriel--dun-zip-acheté-à-un-son-prêt-pour-la-scène)).

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

`namfoop models` liste la bibliothèque ([§ 6](#6-la-bibliothèque-de-sons)) ; `namfoop import` et
`namfoop meta` la remplissent et la corrigent. `namfoop ports` liste
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
calibration_target_dbfs = -30.0     # crête visée lors de calibrate (repère des vumètres)
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
son est très agressif. Ajoute `-i auto` ou une des IR conseillées. Si le modèle contient en fait
son baffle (métadonnées fausses), corrige-le : `namfoop meta <id> --type amp-cab`.

**Un pack acheté sonne plus fort ou plus saturé que les autres.** Les packs sont calibrés
différemment. Le volume est déjà aligné par `--level` ; pour la saturation, donne au modèle (ou
au pack, avec `--all`) un gain d'entrée propre : `namfoop meta <mots-clés> --all --input-db -6`.

**`import` refuse un modèle « sortie invalide » ou « illisible ».** Le fichier est corrompu, ou son
architecture n'est pas reconnue par la version de NeuralAmpModelerCore de namfoop. Signale-le dans
une issue avec le nom du pack.

**Le son de l'ordinateur n'arrive pas dans le casque de la table.** Voir le
[§ 2](#2-matériel-et-câblage) : le retour USB doit passer par une tranche qui alimente l'aux.

---

## 15. Limites actuelles

namfoop est un prototype. Pas encore disponibles : effets (gate, compresseur, EQ, crossover,
modulation, delay, réverbe), presets, changement de son par MIDI, traitement du micro, démarrage
automatique, version Raspberry Pi. Voir le [CHANGELOG](../CHANGELOG.md) pour suivre les versions.
