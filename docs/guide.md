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
namfoop browse        # choisir modèle, IR, mélange et effets en jouant
namfoop run -m b7k    # jouer (avec effets : --pre / --post, voir § 10)
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
10. [Effets](#10-effets)
11. [Backends, carte et routage en ligne de commande](#11-backends-carte-et-routage-en-ligne-de-commande)
12. [Travailler sans jouer : rendu, bench, inventaire](#12-travailler-sans-jouer--rendu-bench-inventaire)
13. [Mesurer la latence](#13-mesurer-la-latence)
14. [Le fichier de configuration](#14-le-fichier-de-configuration)
15. [Dépannage](#15-dépannage)
16. [Limites actuelles](#16-limites-actuelles)

---

## 1. Principe

```
            ┌─────────────────────────────────── namfoop ────────────────────────────────────┐
basse ─▶ entrée basse ─▶ trim ─▶ pré ─▶ NAM ─▶ IR ─▶ post ─▶ niveau ─▶ master ─▶ limiteur ─┬─▶ sorties « traité »
            │                │                                                             └─▶ (sondier, in-ear)
            │                └────────────────────────────────────────────────────────────────▶ sorties « DI »
micro ─▶ entrée micro ────────────────────────────────────────────────────────────────────────▶ sorties « micro »
            └────────────────────────────────────────────────────────────────────────────────┘
```

| Étage | Rôle | Réglages |
|---|---|---|
| trim | adapte le niveau de **ta** basse au niveau attendu par les modèles | `calibrate`, `--trim` |
| pré | effets avant le modèle : gate, compresseur, EQ ([§ 10](#10-effets)) | `--pre`, espace Chaîne de `browse` |
| NAM | le modèle d'ampli / pédale / rig | `-m`, `--input-db`, `--output-db`, `--size` |
| IR | le baffle (seulement si le modèle n'en contient pas) | `-i`, `--ir-ms` |
| post | effets après le baffle : EQ, chorus, delay, réverbe ([§ 10](#10-effets)) | `--post`, espace Chaîne de `browse` |
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

Prérequis : un compilateur C++20, `pkg-config`, `git`, `rustup`, et les fichiers de développement
d'ALSA et de JACK. Les paquets à installer sur Arch, Ubuntu / Debian et Fedora sont listés dans le
[README](../README.md#prérequis-de-compilation).

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

Sur scène, un seul buffer en retard s'entend. Quatre réglages du système comptent ; namfoop
les vérifie au démarrage et avertit (il ne modifie jamais le système lui-même) :

- **Priorité** (le réglage qui compte le plus) : le thread audio ALSA passe en SCHED_FIFO 80 si tu
  en as le droit, sinon il la demande à rtkit, qui plafonne à 20 et coupe un thread qui tourne
  plus de 200 ms d'affilée. Mesuré sur PC : à 20, un buffer sur 1 000 prend 3 à 6 fois le temps
  habituel et quelques-uns débordent chaque minute ; à 80, plus aucun ne dépasse la moitié du
  temps disponible. La voie normale : installer `realtime-privileges` et s'ajouter au groupe
  `realtime` (ou `audio` selon la distribution), puis se reconnecter. namfoop avertit s'il n'a que
  la priorité de rtkit. Sur le Pi : `LimitRTPRIO` dans le service systemd.

  ```sh
  sudo pacman -S realtime-privileges && sudo usermod -aG realtime $USER   # puis se reconnecter
  ```

- **Mémoire verrouillée** : avec une limite `memlock` illimitée, namfoop verrouille sa mémoire
  (`mlockall`) : plus de défaut de page en jouant. Le groupe `realtime` l'accorde ; sinon, une
  ligne `@realtime - memlock unlimited` dans `/etc/security/limits.d/`. Avec une limite finie,
  rien n'est verrouillé (« mémoire non verrouillée »).
- **Gouverneur du processeur** : en `powersave` (défaut de beaucoup de distributions), la
  fréquence redescend entre deux buffers et la charge fait des pics. Pour jouer :

  ```sh
  sudo cpupower frequency-set -g performance   # jusqu'au prochain redémarrage
  ```

- **Cœur dédié** : `--cpu N` (ou `[audio] cpu = N`) épingle le thread audio sur un cœur (ALSA,
  carte simulée ; avec JACK, le thread appartient au serveur). Utile surtout avec `isolcpus` sur
  le Pi, qui réserve ce cœur à namfoop.

```sh
namfoop --cpu 3 run -m b7k
```

Le calcul se fait en mode *flush-to-zero* (thread audio, `render`, `bench`) : les nombres
minuscules d'un son qui s'éteint (queue d'IR, de delay, de réverbe) sont ramenés à zéro au lieu de
ralentir le processeur pendant les silences ; c'est inaudible. Une ligne « dénormalisés » au
démarrage le confirme.

Chaque chaîne est aussi préchauffée avant d'entrer en jeu (bruit puis silence, au buffer réel),
pour que le premier passage du modèle et de l'IR ne tombe pas sur le thread audio.

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
   ([§ 14](#14-le-fichier-de-configuration)). Le reste (trim, niveau...) n'est pas touché.

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

**Dans l'éditeur (`e`)**, le grain est remesuré à chaque réglage de l'entrée : tu vois le modèle
passer de « clair » à « crunch 2 % » pendant que tu l'entends. « casse à +6 dB » se lit alors à
partir du réglage en cours.

**Le grain affiché aide** : « clair · casse à +9 dB » veut dire qu'il faut environ +9 dB d'entrée
pour que ce modèle commence à saturer avec ta basse ; un modèle « saturé 18 % » qui devrait être
clair (d'après son nom) est trop poussé, essaie -6 dB ([profil des
modèles](#profil-sonore-et-grain-des-modèles)).

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

### Profil sonore et grain des modèles

`namfoop models` (et l'onglet Modèles de `browse`) décrit aussi chaque modèle, **mesuré avec ta
basse** : l'enregistrement de `calibrate` (les 4 s les plus jouées), joué au niveau que le modèle
reçoit vraiment (trim + entrée propre du modèle).

```
    mrzw   PPB_DeepAndClean_DI  — amp | WaveNet 3ch + WaveNet 8ch | -12.1 dB
           ██▇▅▃▃▁▄▄▄▁▁  clair · casse à +15 dB · sombre, médium creusé  (avec AMPG_SVT410_Mix1)
    mway   ProCo Rat 2 - D_F_V - 9_9_9  — pedal rock | …
           ▆██▆▆▇▅▆▆▃▁▁  très saturé 28 % · compressé · brillant, sub +4  (avec …)
```

- **Grain** (absolu) : distorsion harmonique d'un la (110 Hz) joué à ta crête de jeu.
  - **clair** (< 2 %), **crunch** (2–8 %), **saturé** (8–20 %), **très saturé** (≥ 20 %) ;
  - **compressé** : le modèle tasse tes attaques (facteur de crête réduit de 5 dB ou plus) ;
  - **casse à +9 dB** : pour un son clair, le gain d'entrée à ajouter pour qu'il commence à
    saturer (3 %). C'est le repère pour régler l'entrée propre du modèle (`namfoop meta
    --input-db`, ou `e` dans `browse`) ; « ne casse pas » : pas de saturation jusqu'à +18 dB.

  Repères mesurés : un Bassman 800 « presque clair » donne 1 %, une Rat passe de 0,2 % (drive
  1,5) à 6 % (drive 6) puis à 28 % (drive 9).
- **Courbe et étiquettes** : comme pour les IR, mini-spectre et écarts au reste du pack. C'est la
  sortie du modèle divisée par ta basse, soit la « couleur » de l'ampli. Pour un modèle sans
  baffle, elle inclut l'**IR préférée** (ou la mieux conseillée), indiquée entre parenthèses :
  c'est ce que tu entends.
- `namfoop meta <id>` détaille la mesure (distorsion en %, compression en dB).

La mesure est faite une fois (≈ 0,1 s par modèle et par cœur), à l'import ou au premier `models`
ou `browse`, puis gardée dans `library/.cache/model-profiles.json`. Elle est **refaite
automatiquement** pour un modèle dont tu changes l'entrée, et pour tous après un nouveau
`calibrate`. Sans calibration, un signal de synthèse (notes de basse à -30 dBFS) sert de repli,
et namfoop le signale.

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
([§ 14](#14-le-fichier-de-configuration)). Pour un seul modèle, utilise `namfoop meta <id> --ir …`
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

**Deux espaces**, parce que ce sont deux activités : la **Bibliothèque** (`1`), où l'on
cherche un son, et la **Chaîne** (`2`), où l'on le règle.

| | **Bibliothèque** (magenta) | **Chaîne** (cyan) |
|---|---|---|
| Contenu | onglets **Modèles**, **IR**, **Mélange** | le **synoptique** de la chaîne, l'éditeur, les réglages |
| Ce qu'on entend | le **banc d'essai** : `basse › modèle › IR`, **sans aucun effet** | la **chaîne** telle qu'elle est éditée |

`1` et `2` changent d'espace depuis n'importe où (sauf pendant une saisie ou un choix en cours) et
**l'écoute suit**, avec le fondu habituel. Chaque espace garde son état : le curseur et le son du
banc d'un côté, la chaîne et son curseur de l'autre. Dans la Bibliothèque, **Tab** passe à l'onglet
suivant (en boucle), **Maj+Tab** au précédent, **→ / ←** vont de proche en proche — ils ne mènent
plus à la Chaîne, et → sur Mélange le dit. L'en-tête rappelle les deux espaces et le nombre de
blocs de la chaîne (`2 Chaîne ● 5 blocs`).

Trois règles tiennent l'ensemble :

1. **parcourir n'écrit jamais dans la chaîne** — écrire est un geste explicite (Entrée) ;
2. **on sait toujours ce qu'on écoute** : le bandeau du cadre et la ligne `♪` sous lui le disent
   (`♪ banc : basse › …`, avec la chaîne rappelée en gris ; ou `♪ chaîne : basse › …`) ;
3. **on ne quitte pas la chaîne pour remplir un bloc** : le choix se fait sur place (Entrée sur un
   bloc `nam` ou `ir`, [ci-dessous](#choisir-le-modèle-ou-lir-dun-bloc-sur-place)).

- **Onglet Modèles** : identifiant, badge « IR incluse » ou « IR requise », type de capture.
  ↑↓ **écoute** le modèle sous le curseur au banc d'essai, après 150 ms d'arrêt (on peut défiler
  vite sans tout recharger) — et **rien d'autre** : la chaîne n'est pas touchée. `◀ écouté` marque
  ce que joue le banc ; un **`●` vert** devant un modèle (ou une IR) rappelle que la chaîne
  l'utilise. **Entrée** met le son du banc dans la chaîne (ci-dessous). Pour un modèle sans baffle, la meilleure IR conseillée est posée d'office. Pour une
  capture qui inclut son baffle, l'IR est retirée. La première ligne, **son direct (DI, sans
  modèle)**, joue ta basse telle quelle : choisis-la puis une IR dans l'onglet IR pour **écouter
  une IR seule** (comparer les micros d'un pack, préparer un mélange). L'IR en cours est gardée
  quand tu passes sur cette ligne. Elle disparaît quand un filtre est actif.
- **Onglet IR** (Tab) : « aucune IR », puis les IR conseillées pour le modèle (★), puis toutes les
  autres. Dans un pack, le début commun des noms est masqué : on lit « 121 Lower - Cap » au lieu de
  « 1970 Bassman Cabinet CTS - 121 Lower - Cap ». Chaque IR affiche son mini-spectre et ses
  étiquettes ([profil sonore](#profil-sonore-des-ir)). **Espace** ajoute ou retire l'IR du mélange.
- **Panneau Spectre** (onglets IR et Mélange, terminal d'au moins **128 colonnes**) : à droite de
  la liste, la courbe de l'IR sous le curseur. Dans l'onglet Mélange, s'y ajoute la courbe du
  **mélange** résultant, où se voient les annulations entre micros. **`c` épingle** l'IR sous le
  curseur comme **référence** (◆, courbe jaune) ; sous le graphique, les écarts par zone entre
  l'IR et la référence (« sub +4 · … · air −2 »). `c` sur la référence la retire.
- **Panneau Bandes** (`z`) : le même propos que la courbe, mais lisible d'un regard — un
  **égaliseur graphique** des sept zones, qui dit ce que la chaîne écoutée (modèle + IR) fait à ta
  basse :

  ```
  ┌ Bandes — ce que la chaîne fait à ta basse ─┐
  │ +12 ┤                                      │
  │  +6 ┤                      ██  ██          │
  │   0 ┼──────────────██──██──██──██──        │
  │  -6 ┤  ██  ██  ██                          │
  │ -12 ┤  ▼▼  ██                              │
  │      sub  gr  bm  md  hm  pr air           │
  │      -26 -19  -9  -3  +3 +11 +19   dB      │
  └────────────────────────────────────────────┘
  ```

  Les zones sont celles des étiquettes : sub (40–80 Hz), grave (80–200), bas-médium (200–500),
  médium (0,5–1,5 kHz), haut-médium (1,5–3,5 kHz), présence (3,5–7 kHz), air (7–14 kHz). L'échelle
  va de -12 à +12 dB, un demi-bloc valant 3 dB ; `▲` / `▼` signalent une zone qui sort de
  l'échelle. Vert dans ±3 dB, jaune jusqu'à ±8, rouge au-delà. Le zéro n'est pas un volume : c'est
  la moyenne 80 Hz – 5 kHz de la chaîne elle-même, donc les colonnes montrent sa **couleur**, pas
  son niveau.

  L'exemple ci-dessus est une **pédale de fuzz** : elle perd 26 dB de sub et 19 dB de grave, et
  monte de 19 dB dans l'air. C'est ce qui la fait paraître bien plus forte qu'un ampli alors que
  [l'alignement de niveau](#volume-de-sortie--level) les met au même RMS — l'oreille entend surtout
  la zone où le fuzz a mis son énergie.

  Quand une **référence est épinglée** (`c`), les colonnes deviennent l'**écart** entre les deux
  sons et le titre le dit (« …FAT CLASSIC ROCK vs ◆ …REGGAE ») : le début commun des deux noms est
  retiré pour qu'on voie ce qui les distingue. `r` retourne la comparaison, et les colonnes avec
  elle.

  La mesure est **figée** : elle vient du profil déjà calculé avec ta basse, donc les colonnes
  changent dès que tu bouges le curseur, sans rien coûter au son. À partir de 128 colonnes le
  panneau se glisse sous la courbe du Spectre et ne prend aucune ligne de liste : il est alors
  affiché d'office. Sur un terminal plus étroit il s'empile sous la liste et attend `z`.
- **Onglet Modèles** : chaque modèle affiche son mini-spectre, son **grain** (vert clair, jaune
  crunch, rouge saturé) et ses étiquettes ([profil des modèles](#profil-sonore-et-grain-des-modèles)).
  Le panneau Spectre y montre la courbe du modèle sous le curseur (avec son IR), et une ligne
  « grain : … · ◆ … » compare au modèle de référence.
- **A/B entre modèles** : dans l'onglet Modèles, **`c`** épingle le modèle sous le curseur (◆).
  Choisis un autre modèle (A), puis **`r`** pour basculer entre les deux. Le modèle B garde l'IR en
  cours s'il en a besoin, ou joue sans IR si son baffle est inclus. L'en-tête affiche « A/B
  modèles ». Épingler une IR (onglet IR) remplace la référence modèle, et inversement : un seul A/B
  à la fois.
- **Écoute A/B**, dans tous les onglets et dans l'éditeur :
  - **`b`** coupe ou remet l'IR, pour entendre le modèle (ou ta basse) avec et sans baffle ;
  - **`r`** bascule entre ton choix (A) et la référence épinglée (B), sans perdre la sélection.

  Un **indicateur A/B** reste affiché en haut à droite, à côté des onglets : `A <ton choix>` et
  `B ◆ <référence>`. Celui qu'on entend est surligné (A en cyan, B en jaune), et `IR COUPÉE` est
  en rouge. Dans la liste, `◀ écoutée (A)` ou `◀ écoutée (B)` marque l'IR jouée.

  Pas à pas : dans l'onglet IR, place-toi sur une IR et `c` (elle devient B). Choisis ensuite une
  **autre** IR avec ↑↓ (elle devient A), puis `r` pour basculer. Si tu épingles l'IR en cours de
  lecture, A et B sont la même : l'indicateur le dit, et `r` ne fait rien tant que tu n'as pas
  choisi une autre IR. Il en va de même pour deux copies identiques d'un même fichier (voir
  l'import). En mode `match`, le volume est réaligné à chaque bascule : la comparaison se fait
  **à volume égal**, l'oreille juge le timbre et non le niveau. Choisir une autre IR ou un autre
  modèle met fin à l'écoute de B (retour à A).
- **Onglet Mélange** : `+`/`-` règle le gain de l'IR choisie (par pas de 1 dB), `p` inverse sa
  polarité, `a` active ou coupe l'alignement automatique, `x` la retire. Chaque changement
  s'écoute aussitôt. `s` sauve le mélange dans `library/cabs/` (Entrée valide le nom proposé).
- **Espace Chaîne** (`2`) : le **synoptique** de la chaîne,
  qui est aussi l'**éditeur**, puis les **réglages** de ce qui est sous le curseur (barre, valeur,
  unité, aide et plage) et la **ligne de commande équivalente**, à copier dans `run`.

  Le synoptique montre la chaîne de l'entrée à la sortie, avec un **vumètre au-dessus de chaque
  bloc** :

  ```
  ▁▃▅   ▁▃▅▇█    ▁▃▅▇    ▁▃
  IN ─ gate -60 ─ nam·b7k ─ ir·sm57 ─ delay ─ OUT
  ```

  En volume aligné sur le DI (`level = "match"`, après `calibrate`), un dernier étage **grisé**
  apparaît avant `OUT` avec sa valeur :

  ```
  IN ─ nam·b7k ─ ir·sm57 ─ niveau DI -11.1 dB ─ OUT
  ```

  Ce n'est pas un bloc : on ne le choisit pas, on ne le règle pas, on ne le déplace pas. C'est
  l'alignement qui ramène la chaîne au volume de ta basse en direct — mesuré à chaque
  reconstruction, donc il bouge quand tu règles un bloc. Il explique une surprise classique : un
  bloc `gain` dans la chaîne **ne change pas le niveau de sortie**, parce que cet étage le
  compense exactement (voir [`--level`](#volume-de-sortie--level)). `browse` le dit aussi quand tu
  ajoutes un `gain`, et sous ses réglages.

  Les barres vont de -60 à 0 dBFS, en huitièmes de caractère : vert, **jaune** au-dessus de -9 dB,
  **rouge** au-dessus de -1 dB (là, le limiteur travaille). Elles montent tout de suite et
  descendent de 28 dB par seconde, comme un vumètre d'enceinte. Deux barres se lisent autrement :
  celle d'un `comp` se remplit **en orange depuis la droite** avec sa réduction de gain (toute la
  barre = 20 dB), et celle d'un `gate` **fermé** est pleine et rouge sombre. Un bloc coupé (`b`) est
  barré et sa barre s'éteint. **`v`** masque les vumètres (utile pour lire la chaîne au calme). Le
  titre du cadre compte les modèles actifs et prévient si la charge risque de monter (plusieurs
  modèles NAM dans la même chaîne).

  **Se déplacer** : **←→** suit la chaîne de bloc en bloc, **↑↓** passe d'une branche à l'autre
  dans un parallèle. Le curseur peut être sur un bloc, sur le `◆` d'un parallèle, sur l'**en-tête**
  d'une branche (son étiquette et son gain, choisis ensemble), ou sur la **sortie** du parallèle
  (`◇`, à son bord droit, là où les branches se somment).

  **Construire la chaîne** :

  | Touche | Effet |
  |---|---|
  | **`a`** / **`i`** | ouvre le **catalogue** (modèle, IR, effets) et ajoute **après** / **avant** le curseur, dans sa branche. Tape pour filtrer, ↑↓, Entrée ajoute, Échap annule |
  | **`s`** | met le bloc **en parallèle**. Sur un `nam` suivi de son `ir`, les deux partent ensemble et le parallèle est un **crossover à 150 Hz** : grave = son direct, aigu = l'ampli — le blend grave clair, en une touche. Sinon une copie (B1 = son direct, B2 = le bloc) |
  | **`+`** | une **branche** de plus (4 au maximum ; en bandes, une coupure de plus) |
  | **`x`** | retire le bloc ; sur un en-tête, retire la branche (à deux branches, le parallèle est défait) ; sur le `◆`, défait le parallèle en gardant la branche de l'ampli |
  | **`[` / `]`** | déplace le bloc dans la chaîne. À côté d'un parallèle, il y **entre** (branche 1) ; au bout d'une branche, il en **sort**. Sur le `◆`, déplace tout le parallèle |
  | **`{` / `}`** | passe le bloc dans la branche du dessus / dessous |
  | **`◇`** | la **sortie** du parallèle, à son bord droit : `→` depuis le bout d'une branche (ou `←` depuis le bloc qui suit) t'y amène. **`a` y ajoute après le parallèle** — c'est la façon d'ajouter un bloc derrière un parallèle en bout de chaîne —, `i` à la fin de la première branche. `↑` remonte au `◆`, d'où le parallèle se règle, se déplace et se défait |
  | **`b`** | coupe ou remet le bloc (il reste visible, barré) |
  | **`m` / `o` / `p`** | **muet** / **solo** / **polarité** de la branche. Le solo est une aide d'écoute : il ne s'enregistre jamais |
  | **Entrée** | les **réglages** de ce qui est sous le curseur : les paramètres d'un effet, le mode et les coupures d'un parallèle, le gain et la phase d'une branche. Sur un bloc `nam` ou `ir`, ouvre le **sélecteur sur place** (ci-dessous) |
  | **`v`** | masque / affiche les vumètres |

  <a id="choisir-le-modèle-ou-lir-dun-bloc-sur-place"></a>
  **Choisir le modèle ou l'IR d'un bloc, sur place.** Quand le curseur est sur un bloc `nam` ou
  `ir`, le panneau sous le synoptique montre son contenu **en entier** (nom complet, variante,
  pack, identifiant, badge IR, mini-spectre, grain) — ce que l'étiquette du synoptique, coupée à 16
  caractères, ne peut pas faire. **Entrée** y ouvre la **liste de la bibliothèque** (cadre jaune) :

  | Touche | Effet |
  |---|---|
  | **↑↓** | **essaie dans la chaîne**, avec ses effets, 150 ms après la dernière touche. Le synoptique montre l'essai ; la liste marque `● actuel` ce qui était là et `◀ à l'essai` ce qu'on entend |
  | **Entrée** | **garde** ce qu'on entend |
  | **Échap** | remet l'**actuel** : la chaîne redevient exactement celle d'avant |
  | **`c`** | bascule l'écoute entre l'**actuel** et l'**essai**, sans bouger le curseur : l'A/B en contexte. La ligne marque `● actuel ◀ écouté (c)`, et Entrée pendant qu'on écoute l'actuel garde l'actuel. En mode `match`, le volume est réaligné à chaque bascule |
  | **`o`** | n'entend que **ce bloc et son voisin immédiat** (`basse › nam › ir`), comme au banc d'essai ; le reste du synoptique s'assombrit. `o` de nouveau revient à la chaîne |
  | **`r`** | avec / sans effets |
  | **`/`** | filtre la liste, comme dans les onglets |

  Tant qu'on n'a pas validé, **rien n'est modifié** : un essai ne compte pas comme un changement
  (`q` ne demande rien de plus), et `q` est d'ailleurs refusé tant qu'un choix est en cours — valide
  (Entrée) ou annule (Échap) d'abord. Pour une IR, les **mélanges enregistrés** (`library/cabs/`)
  sont en tête de la liste et les IR **conseillées** pour le modèle voisin portent un `★` ; il n'y a
  pas de ligne « aucune IR » (on retire le bloc avec `x`). Calibrer (`e`) reste réservé aux onglets
  Modèles et IR. Sur un terminal assez large (128 colonnes), le **panneau Spectre** s'ouvre à
  droite de la liste : la courbe de la ligne sous le curseur, l'**actuel** superposé en référence
  (`●`), les écarts par zone et, pour un modèle, la ligne `grain`. Choisir `nam` ou `ir` dans le **catalogue** (`a` / `i`) pose un bloc
  `nam·(à choisir)` et ouvre aussitôt le sélecteur : Échap retire le bloc, rien n'est ajouté.

  Dans les réglages : ↑↓ choisit, ←→ change d'un pas, **Maj + ←→** d'un pas fin (un dixième),
  Échap ou Entrée revient à la chaîne. Les coupures d'un crossover se règlent par **pas
  multiplicatif** (×1,06, ou ×1,01 en pas fin) et restent écartées d'au moins ×1,25 l'une de l'autre.

  Un parallèle a **2 à 4 branches** et deux modes (réglage `mode`) : **copie** (chaque branche reçoit
  tout le signal) ou **bandes** (crossover LR4, une bande par branche). Une branche **vide** est le
  son direct, mise automatiquement au niveau de la branche qui contient l'ampli. Il n'y a qu'**un
  seul niveau** : un parallèle dans une branche est refusé (le message dit quoi faire).

  **`r`** bascule **avec / sans effets** (A/B) ; la ligne de commande garde la chaîne réglée. Chaque
  changement s'écoute 150 ms après la dernière touche, avec le fondu habituel. Pas, plages et unités
  viennent de la table de chaque effet ([Effets](#10-effets)). En prototype,
  chaque réglage reconstruit la chaîne : les queues de delay et de réverbe sont coupées. Tant que la
  chaîne s'exprime en options (`-m`, `-i`, `--pre`, `--post`), le panneau du bas donne la ligne de
  commande ; dès qu'il y a un parallèle, il faut un **preset** — l'enregistrement viendra avec
  eux (#6).
- **Mettre le son du banc dans la chaîne : Entrée.** Quand la chaîne est **vide**, elle devient
  **ce que tu entends** — `nam › ir`, le modèle seul si son baffle est inclus (ou si `b` coupe
  l'IR), l'IR seule si le banc ne joue pas de modèle —, et depuis n'importe lequel des trois
  onglets : c'est le parcours « j'ouvre `browse`, je choisis un son » (un modèle, puis son baffle),
  et le message rappelle que `2` sert ensuite à l'habiller d'effets.

  Dès qu'elle n'est pas vide, une **fenêtre** demande où va le son — même s'il n'y a qu'un bloc du
  bon type, parce qu'il faut aussi décider du sort de l'IR :

  ```
  ┌ Mettre « SVT Classic · clean » dans la chaîne ─────────────────────────┐
  │ ▶ remplacer  nam·Fuzz Face (Fuzz Face · bass) (branche fuzz)           │
  │   remplacer  nam·SVT Classic (SVT Classic · clean) (branche ampli)     │
  │   ajouter un bloc nam…  (tu choisis l'endroit dans la chaîne)          │
  │                                                                        │
  │  [x] avec son IR (8x10 SM57) dans le bloc ir voisin                    │
  │                                                                        │
  │  Entrée valider · Espace cocher · Échap annuler                        │
  └────────────────────────────────────────────────────────────────────────┘
  ```

  Une ligne `remplacer` par bloc `nam` (ou `ir`) de la chaîne, dans l'ordre du synoptique, avec le
  **nom complet** du modèle en place et sa branche ; `ajouter` en dernier. ↑↓ choisit, **Espace**
  coche, Entrée valide, Échap annule (rien n'est changé). La case **« avec son IR »** n'apparaît
  que pour un modèle qui a besoin d'une IR quand le banc en joue une ; cochée, l'IR du banc va dans
  le bloc `ir` qui suit le `nam` visé — et si ce bloc n'existe pas, il est ajouté juste après.

  - **Remplacer** : on reste dans la Bibliothèque, le message dit ce qui a changé (`nam·B7K Ultra
    (branche > 150 Hz) remplacé par nam·SVT Classic › ir·8x10 SM57 · 2 pour voir la chaîne`).
  - **Ajouter** : on passe dans la Chaîne en **mode pose**. Le cadre demande `♪ CHAÎNE · où poser
    nam·… › ir·… ?`, les flèches déplacent le curseur comme d'habitude, **Entrée pose après le
    curseur** (mêmes règles que `a`), Échap annule et ramène à la Bibliothèque. Aucune autre touche
    d'édition ne passe en mode pose. Après la pose on reste dans la Chaîne, `[ ]` déplace le bloc.

  Depuis l'onglet **Mélange**, Entrée envoie le mélange ; s'il n'est pas encore enregistré, il l'est
  d'abord sous le nom proposé. Sur « son direct » (ou « aucune IR »), un message dit qu'il n'y a
  rien à envoyer.
- **Enregistrer et retrouver ses chaînes : les presets.** **`S`** enregistre la chaîne en cours
  dans `library/presets/<nom>.preset.toml` (un nom est proposé, Entrée valide, Échap annule) ;
  **`P`** ouvre la **banque** : la liste des presets avec leur identifiant court, leur chaîne en une
  ligne, le filtre `/`, et Entrée pour en **charger** un. En haut à droite, l'en-tête dit toujours
  où tu en es : `preset : Chill verse ✓`, ou `● non enregistré (S)` dès que la chaîne diffère de ce
  qui est enregistré (c'est l'**état** qui est comparé : défaire ton changement efface l'alerte).

  Charger un preset alors que la chaîne n'est pas enregistrée demande d'abord, **une fois** : `S`
  enregistrer puis charger, Entrée charger quand même, Échap rester. La fenêtre de `q` propose la
  même chose (`S` enregistrer puis quitter). Un preset dont un son a disparu de la bibliothèque
  reste listé, en rouge, avec la raison : le charger est refusé, mais il est réparable à la main
  (le fichier est du TOML lisible, et l'identifiant court y sert de filet si tu as juste déplacé
  un `.nam`).

  **Repartir de zéro : `n`.** Une chaîne **vierge**, détachée du preset courant : la chaîne se vide
  (la basse passe en son direct le temps que tu ajoutes le premier bloc avec `a`), l'en-tête
  n'annonce plus de preset, et `S` proposera donc un **nom neuf** au lieu de réenregistrer par-dessus
  celui que tu viens d'écouter. C'est la façon de construire une chaîne de plus sans toucher aux
  presets existants — avant, il fallait partir d'une chaîne en place, la vider bloc par bloc et
  bien penser à changer son nom en l'enregistrant. `n` marche depuis les deux espaces et depuis la
  banque (qu'il referme) ; si la chaîne en cours n'est pas enregistrée, il demande d'abord, une
  fois (`S` enregistrer puis repartir, Entrée repartir quand même, Échap rester).

  **Passer d'un preset à l'autre comme sur un pédalier** : dans l'espace Chaîne, **PgDn** va au
  preset suivant et **PgUp** au précédent, dans l'ordre de la banque et **en boucle** (du dernier
  on revient au premier). L'en-tête donne la place : `preset 2/7 : Chill verse ✓`. Si la chaîne en
  cours n'est pas enregistrée, on te demande d'abord — une fois. Dans la Bibliothèque, PgUp et PgDn
  continuent de **défiler les listes** : le preset, c'est la chaîne, on en change depuis la Chaîne.

  La bascule est **immédiate** : les chaînes de tous tes presets sont bâties et préchauffées
  **dès l'ouverture de `browse`**, en tâche de fond (la barre du bas affiche `banque 3/7` tant
  qu'il en reste), si bien que changer de preset ne charge plus rien. Mesuré sur PC, de la touche
  au son installé : **20 ms**, le temps du fondu — y compris pour un preset à deux modèles en
  parallèle, qui demandait 204 ms avant. De quoi tomber sur un temps. Le son ne se coupe jamais :
  la chaîne précédente joue jusqu'au fondu.

  Un preset dont la chaîne n'est pas encore préchargée (juste après l'ouverture, ou juste après
  l'avoir enregistré) se charge comme avant, en ~80 ms : c'est un retard, pas une panne. Même chose
  pour un preset qu'on **rejoue dans la seconde qui suit son départ** : sa chaîne finit encore de
  sonner (voir la queue, juste en dessous), donc elle est rebâtie.

  **La queue : la réverbe du preset quitté finit de sonner.** Quand tu changes de preset, la chaîne
  que tu quittes n'est pas coupée : son entrée se ferme (le fondu de 20 ms) mais sa **réverbe** et
  ses **répétitions de delay** continuent leur décroissance pendant que le nouveau son attaque,
  comme sur un pédalier à « trails ». Chaque effet **dit** combien de temps il peut encore sonner,
  et la queue est gardée exactement ce temps-là — mesuré : **23 ms** pour une chaîne sans effet à
  queue (le fondu seul : rien ne sonne, rien n'est gardé), **1,1 s** pour une réverbe aux réglages
  par défaut, 2 s au maximum pour un delay à longue réinjection.

  Le prix : pendant ce temps, **deux chaînes tournent** (la queue et la nouvelle), donc jusqu'à deux
  fois la charge DSP — pour b7k, 3,4 % du budget d'un bloc qui passent à 6,8 %. C'est sans
  conséquence sur un PC ; si tu entends des décrochages (`xruns` qui montent dans la barre du bas)
  sur un Raspberry Pi ou avec un preset à deux modèles en parallèle, coupe la queue :

  ```sh
  namfoop browse --no-trails     # ou namfoop run --no-trails
  ```

  La réverbe du preset quitté s'arrête alors net au changement, comme avant, et une seule chaîne
  tourne. À savoir : la queue ne coûte plus rien quand rien ne sonne (une chaîne `nam › ir` part en
  23 ms au lieu d'un demi-tiers de seconde), et un delay lent n'est plus coupé entre deux
  répétitions — la durée est demandée à la chaîne, non devinée en écoutant le silence.

  **`t`** déplie le TOML du preset sous le synoptique (replié par défaut : `▸ preset TOML (t)`) —
  pratique pour vérifier ce qui sera écrit, ou pour copier un bloc d'un preset à l'autre.

  ```sh
  namfoop browse          # S enregistrer, P la banque, n chaîne vierge, t le TOML
  ```

  Les presets sont rangés dans la bibliothèque, comme tes mélanges d'IR : ils suivent donc la même
  sauvegarde et la même copie vers le Pi. Ils contiennent la chaîne **entière** (blocs, parallèles,
  branches, blocs coupés) avec **tous** les paramètres des effets, pour qu'un preset sonne pareil
  dans un an ; le gain d'entrée propre à un modèle reste, lui, dans `library/overrides.toml`.
- **`h` (ou `?`) ouvre l'aide** : toutes les touches de l'écran où tu es — Bibliothèque (selon
  l'onglet), Chaîne, sélecteur d'un bloc, catalogue, fenêtre « mettre dans la chaîne », mode pose,
  éditeur `e`. N'importe quelle touche la referme. La barre du bas, elle, reste affichée en
  permanence et finit **toujours** par `h aide`, l'autre espace (`1` / `2`) et `q quitter` : sur un
  terminal étroit, les touches du milieu sautent (un `…` le dit) mais jamais celles-là.
- **`/` filtre** la liste : tape des mots (`121 lower`), Entrée pour valider. Hors filtre, les
  lettres sont des commandes.
- **`e` modifie et calibre** le modèle sous le curseur, sans couper le son (voir ci-dessous). Un
  modèle corrigé est marqué ✎.
- En bas : le son en cours, les vumètres, la réduction du limiteur (`lim`), la charge DSP et les
  xruns.
- **Quitter : `q`.** Échap ne quitte jamais : il revient d'un niveau (il sort du filtre, annule la
  saisie d'un nom, ferme l'éditeur `e`, revient des réglages d'un effet à la chaîne puis au
  catalogue) et, quand il n'y a plus rien à remonter, il le dit. Si quelque chose n'est pas
  enregistré — chaîne d'effets changée depuis le lancement, mélange d'IR pas sauvé, brouillon de
  l'éditeur `e` — `q` ouvre une fenêtre qui liste ce qui serait perdu : **`Q`** (majuscule, un geste
  volontaire) quitte sans enregistrer, **Échap ou n'importe quelle autre touche** fait rester. Il
  n'y a rien à confirmer si tu as défait tes changements ou tout enregistré : c'est l'état qui est
  comparé, pas un drapeau. Parcourir la Bibliothèque n'ajoute donc **jamais** rien à confirmer.

  Un cas à part : **la chaîne est vide alors que le banc joue un son**. `q` le signale toujours,
  même s'il n'y a rien d'autre à enregistrer, parce que la commande affichée en quittant ne jouerait
  pas ce son : **Entrée** le met dans la chaîne et on reste (`2` pour la voir), **`Q`** quitte quand
  même, **Échap** reste. Banc sur « son direct » sans IR : rien à signaler.

En quittant, le navigateur affiche la commande pour rejouer le même son, effets compris, par
exemple `namfoop run -m m4jn -i c7k2 --pre gate --post delay:time=300`. Options : `--library`, `--size` (modèles légers), `--filter`
(filtre de départ, ex. `--filter PopPunk`), et le routage
`--bass-in`, `--mic-in`, `--route` ([§ 11](#11-backends-carte-et-routage-en-ligne-de-commande)).
Sans matériel : `namfoop --backend sim --no-config browse`.

#### Modifier et calibrer un modèle (`e`)

L'éditeur s'ouvre sur le modèle sous le curseur, qui continue de jouer. ↑↓ choisit un champ,
←→ le change, **et le changement s'entend tout de suite** :

| Champ | ←→ | Pour quoi faire |
|---|---|---|
| Type de capture | `amp`, `amp_cab`, `full_rig`, `pedal`, `preamp` | dire si le baffle est inclus (IR ou non) |
| Entrée | ±1 dB | régler la saturation propre à ce modèle ; s'ajoute à `--input-db`. Le **grain** est remesuré avec ta basse 0,3 s après le dernier changement (« grain : crunch 6 % · casse à +3 dB ») |
| IR préférée | aucune, puis les conseillées, jouées au fur et à mesure | l'IR par défaut du modèle (`-i auto`, conseils) ; Entrée prend l'IR choisie dans l'onglet IR, `x` l'efface |
| Marque, Nom | Entrée pour saisir | conseils d'IR de la même marque, affichage |

La valeur lue dans le fichier est rappelée en regard (« fichier : amp »). **`s` enregistre** dans
`library/overrides.toml`, comme `namfoop meta` ; le `.nam` n'est jamais modifié. **`a` applique**
ce que tu as changé (le type et/ou l'entrée) à **tous les modèles visibles** (ceux du filtre) :
filtre `/fullrig`, `e`, type `amp_cab`, `a`, et tout le pack est corrigé ; les champs que tu n'as
pas changés restent ceux de chaque modèle. Échap ferme sans enregistrer et revient au réglage
enregistré ; `q` veut quitter `browse`, et prévient alors que le brouillon n'est pas enregistré. Pas à pas : [tutoriel d'import](#tutoriel--dun-zip-acheté-à-un-son-prêt-pour-la-scène).

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
`namfoop bench --all-sizes` ([§ 12](#12-travailler-sans-jouer--rendu-bench-inventaire)).

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
   `~/.config/namfoop/` ([§ 14](#14-le-fichier-de-configuration)). Toutes les commandes `run` et
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
maximale, les xruns et les éventuels écrêtages, et la répartition de la charge par buffer :
médiane, p99 (1 buffer sur 100 au-dessus), p99,9, et le nombre de buffers au-delà de 50 %, 80 %
et 100 % du temps disponible. Au-delà de 100 %, le buffer a pris du retard : la marge des
périodes ALSA l'absorbe, jusqu'au xrun.

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

### Rejouer un preset : `namfoop presets`, `run --preset`

Les presets enregistrés dans `browse` (touche `S`) sont des fichiers de la bibliothèque : on peut
donc les jouer **sans écran**, ce qui est le cas d'usage du Pi et des scripts.

```sh
namfoop presets                       # la banque : identifiant, nom, chaîne
namfoop run --preset "chill verse"    # par mots-clés
namfoop run --preset p3k8             # ou par identifiant
namfoop render --preset p3k8 --in di.wav --out sortie.wav
```

`--preset` accepte un **identifiant court** (`p3k8`), un nom ou des **mots-clés**, comme `-m` et
`-i` : si plusieurs presets correspondent, le message les liste. Un preset décrit **toute** la
chaîne : il ne se mélange pas avec `-m`, `-i`, `--pre`, `--post` ni `--crossover` (le message le
dit). Les réglages qui ne sont pas dans la chaîne — trim, niveau (`--level`), master, limiteur,
`--input-db` — restent les tiens : ils viennent de la config et des options, comme d'habitude.

Le rendu est **identique au bit près** à celui des options équivalentes : un preset ne change rien
au son, il le décrit.

### Options de `run`

| Option | Rôle |
|---|---|
| `-m`, `--model` | modèle NAM ([§ 6](#6-la-bibliothèque-de-sons)) |
| `-i`, `--ir` | IR de baffle |
| `--bass-in`, `--mic-in`, `--route` | routage pour cette session ([§ 11](#11-backends-carte-et-routage-en-ligne-de-commande)) |
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
| `match` | (après `calibrate`) le son traité a **le même volume que ta basse en direct**. Au chargement, ton enregistrement de calibration passe dans la chaîne et un gain compense l'écart de **loudness** (pondération K d'ITU-R BS.1770, la norme de la diffusion : l'oreille n'entend pas le fondamental d'une basse comme ses harmoniques). Pas de saut de volume d'un modèle à l'autre ni entre clair et traité |
| `normalize` | (défaut sans calibration) la loudness annoncée par le modèle est ramenée à `--target-db` (-18 dB, la convention NAM) |
| `raw` | sortie brute du modèle, sans correction |

`--master-db` règle le volume final.

**En `match`, aucun gain posé dans la chaîne ne change le volume de sortie.** L'alignement se
mesure sur la chaîne entière et son gain est ajouté **après** tout le reste : il compense donc
exactement ce que tu viens d'ajouter. Cela vaut pour `--output-db`, pour `--post gain:db=…` et pour
un bloc `gain` de l'éditeur de `browse` — mis en dernier juste avant `OUT`, il ne fait rien du
tout. Mesuré : avec un bloc à -40 dB l'alignement vaut +40,9 dB, avec le même à +24 dB il vaut
-23,1 dB, et la sortie est identique dans les deux cas.

Ce qui marche quand même :

| Ce que tu veux | Comment |
|---|---|
| plus ou moins de **saturation** | un `gain` (ou `--input-db`) **avant** le modèle : l'alignement rattrape le niveau derrière, il reste le grain |
| **baisser ou monter la sortie** | `--master-db` (après l'alignement), ou `--level normalize` si tu renonces à l'alignement |
| **équilibrer deux presets** entre eux | le **volume du preset** : `,` et `.` dans l'espace Chaîne de `browse` (voir ci-dessous). Il s'applique **après** l'alignement, c'est le seul gain du preset qu'il n'annule pas |

**Pondération, et ses limites.** Jusqu'en novembre 2026 l'alignement comparait des RMS larges ; il
compare maintenant des loudness pondérées K. L'effet est réel mais modeste : sur la bibliothèque,
une pédale de fuzz est désormais alignée **1,2 dB plus bas** qu'avant, un ampli à 0,1 dB près comme
avant — soit **1,3 dB** de correction relative entre les deux. Ça ne suffit pas à faire tenir un
fuzz à côté d'un clair : pour ça, c'est le [volume du preset](#volume-du-preset---et-) qui tranche.
Une pondération **A** a été mesurée et écartée : elle ignore tellement le grave qu'elle aligne le
fuzz **plus haut** encore que le RMS large (l'ampli, riche en grave, se retrouve baissé à sa place).

`browse` montre cet étage en bout de synoptique (« niveau DI -11.1 dB ») et prévient quand tu poses
un `gain`. Le grain d'un modèle se règle par l'entrée (`--input-db`, ou l'entrée propre au modèle :
[tutoriel d'import](#tutoriel--dun-zip-acheté-à-un-son-prêt-pour-la-scène)).

### Volume du preset : `,` et `.`

Chaque preset porte son propre volume, enregistré dans son `.preset.toml` et appliqué **après**
l'alignement de niveau, juste avant le master. C'est ce qui permet qu'un preset saturé ne saute pas
à l'oreille à côté d'un clair, là où un bloc `gain` ne servirait à rien ([ci-dessus](#volume-de-sortie--level)).

- dans `browse`, espace Chaîne : **`.`** monte d'1 dB, **`,`** baisse d'1 dB, de -24 à +12 dB ;
- le réglage **s'entend tout de suite** : c'est le seul gain de la chaîne qui se change sans la
  rebâtir (#187). Avant, chaque appui rechargeait les modèles et remesurait l'alignement — 143 ms
  pour un modèle, près de 400 ms pour un parallèle à deux modèles, de quoi croire que le réglage ne
  faisait rien. Un saut d'1 dB est lissé sur 20 ms, comme un changement de preset ;
- la valeur se lit dans l'en-tête (« preset 2/5 : Couplet · volume +3 dB ») et en bout de
  synoptique, en gris, **après** l'étage « niveau DI » — l'ordre du signal ;
- elle compte comme une modification : l'en-tête passe à « ● non enregistré », `S` l'enregistre ;
- `namfoop presets` la montre en fin de chaîne, et `run --preset` la joue — le preset sonne donc
  comme tu l'as réglé à l'oreille ;
- le **master** (`--master-db`, ou `master_db` dans la config) reste à part : c'est le volume de la
  scène, pas celui d'un preset.

```sh
# le preset tel qu'il a été réglé dans browse, volume compris
namfoop run --preset "couplet"
```

### IR : `--ir-ms`

Les IR sont tronquées à 200 ms par défaut, avec un fondu : un baffle tient en ~100 ms, la suite est
de la pièce. Elles sont rééchantillonnées automatiquement si besoin (44,1 / 96 kHz) et leur niveau
est normalisé sur 80 Hz – 5 kHz.

### Limiteur

Actif par défaut à -1 dBFS, sans latence ajoutée : il empêche l'écrêtage numérique dur quand un
modèle sort trop fort. `--no-limiter` le désactive (la ligne d'état signale alors `[CLIP SORTIE]`).

---

## 10. Effets

Des effets entourent le modèle et l'IR : avant (**pré**), ils changent ce que le modèle reçoit,
donc sa saturation, comme une pédale devant un ampli ; après (**post**), ils travaillent le son
final sans toucher au grain.

```
trim › effets pré › modèle NAM › IR › effets post › niveau › master › limiteur
```

| Effet | Pour quoi faire | Place habituelle |
|---|---|---|
| `gate` | couper le souffle et les bruits entre les notes | pré, en premier |
| `comp` | égaliser les notes (slap, attaques) | pré (tasse avant la saturation) ou post |
| `hpf`, `lpf`, `peak`, `lowshelf`, `highshelf` | EQ : nettoyer l'infra, creuser le « carton », adoucir le fizz | post (ou pré pour changer ce qui sature) |
| `--crossover` | garder le grave clair du DI sous le son saturé | autour du modèle (option dédiée) |
| `chorus` | ondulation dans le médium, grave sec | post |
| `delay` | écho | post, vers la fin |
| `reverb` | un peu d'espace, surtout dans les in-ears | post, en dernier |
| `gain` | monter ou baisser le niveau à un point de la chaîne | où il faut — mais **avant** le modèle en volume aligné sur le DI, où il ne change que la saturation ([`--level`](#volume-de-sortie--level)) |
| parallèle | 2 à 4 branches (copie ou bandes) : ce n'est pas un effet mais un **bloc de la chaîne**, décrit dans un preset (#6) ; `--crossover` en ligne de commande | — |

Ordre conseillé : **pré** `gate › comp`, **post** `EQ › chorus › delay › reverb`. Le gate vient en
premier pour ne pas couper les queues des effets ; la réverbe en dernier pour réverbérer les échos.

### Parcours type : régler en jouant, puis jouer

1. **Choisir le son de base** : `namfoop browse`, espace **Bibliothèque** : ↑↓ écoute au banc
   d'essai, **Entrée** met le son dans la chaîne
   ([§ 6](#choisir-son-son-en-jouant--namfoop-browse)).
2. **Ajouter les effets** : `2` pour l'espace **Chaîne** (12 effets au catalogue). `a` ouvre le
   catalogue et ajoute **après** le curseur, `i` **avant** ; Entrée sur un effet ouvre ses réglages :
   ↑↓ choisit le paramètre, ←→ change la valeur (Maj + ←→ : pas fin). Chaque changement s'entend
   150 ms après la dernière touche.
3. **Changer le modèle ou l'IR d'un bloc** sans quitter la chaîne : curseur dessus, **Entrée**, puis
   ↑↓ pour essayer dans la chaîne, `c` pour comparer avec l'actuel, Entrée pour garder (ou Échap
   pour revenir à l'ancien).
4. **Comparer** : `r` bascule avec et sans effets ; `b` coupe seulement le bloc choisi.
5. **Ordonner** : dans la chaîne, `[` et `]` déplacent le bloc (il passe de pré à post au bord du
   modèle) ; `x` le retire.
6. **Récupérer la commande** : le cadre « à copier dans namfoop run » montre les options des
   effets ; en quittant (`q`), le navigateur affiche la commande complète, modèle, IR et effets
   compris. Si la chaîne est restée vide alors que le banc d'essai joue un son, `q` le signale et
   propose de mettre ce son dans la chaîne (Entrée) avant de quitter.
7. **Jouer** : coller cette commande.

```sh
namfoop browse
namfoop run -m b7k -i "dg212 sm57" --pre gate --pre comp:threshold=-24 --post peak:freq=300,gain=-4 --post delay:time=375,mix=20 --post reverb:mix=12
```

Sans écran plein, la même chaîne se règle en ligne de commande : `namfoop fx` liste les effets,
`namfoop fx delay` donne les paramètres d'un effet, et `render` permet de comparer deux réglages
dans des fichiers avant de jouer.

```sh
namfoop fx
namfoop fx reverb
namfoop render -m b7k -i "dg212 sm57" --post reverb:decay=2,mix=20 -o reverb.wav
```

Pour l'instant (prototype) :
- **pas encore de presets** : garde la commande (dans un fichier texte ou un script) ; les presets
  TOML et le changement de son par MIDI viendront ensuite ;
- changer un réglage **reconstruit la chaîne** : les répétitions du delay et la queue de la
  réverbe en cours sont coupées.

### En ligne de commande : `--pre`, `--post`, `namfoop fx`

- `--pre FX` : avant le modèle (après le trim). Le modèle reçoit le son déjà traité : un gain, un
  compresseur ou un EQ placé ici **change sa saturation**, comme une pédale devant un ampli.
- `--post FX` : après le modèle et l'IR, sur le son final (avant le niveau, le master et le
  limiteur) : réglages de couleur et de volume sans toucher au grain.
- Les deux options se répètent ; les effets sont joués dans l'ordre donné.
- Syntaxe : `nom:param=valeur,param=valeur` (ex. `gain:db=-6`) ; un paramètre absent prend sa
  valeur par défaut, une valeur hors plage est refusée avec la plage permise.
- `namfoop fx` liste les effets ; `namfoop fx <nom>` donne leurs paramètres (unité, plage, défaut,
  aide). `namfoop bench --fx` mesure le coût de chaque effet.
- En niveau « match », l'alignement sur le DI se calcule avec les effets : un `--post` qui baisse le
  volume est compensé. Pour baisser vraiment, utiliser `--master-db`.

```sh
namfoop fx gain
namfoop render -m b7k -i "dg212 sm57" --pre gain:db=3 -o plus-sature.wav
namfoop run -m b7k -i "dg212 sm57" --post gain:db=-6
namfoop bench --fx --block 48
```

Au chargement, une ligne `chaîne :` rappelle l'ordre réel des étages.

### `gate` : silence entre les notes

Un modèle saturé amplifie le souffle de la basse et des micros : entre deux notes, ça chuinte.
Le gate coupe quand le signal passe sous `threshold`, et rouvre dès qu'on rejoue.

- Réglage de départ : `gate:threshold=-50`, puis monter le seuil jusqu'à ce que le souffle
  disparaisse sans manger les notes jouées doucement.
- `hysteresis` : il se referme quelques dB sous le seuil d'ouverture (pas de battement sur une note
  qui s'éteint) ; `hold` le garde ouvert un instant ; `release` long (100–300 ms) préserve les fins
  de notes ; `range` : de combien il baisse une fois fermé (-80 : silence).
- Piège : placé **avant** le modèle (`--pre`), il agit sur le DI, propre, et coupe le souffle que le
  modèle aurait amplifié ; placé après, il doit composer avec la saturation.

```sh
namfoop run -m b7k -i "dg212 sm57" --pre gate:threshold=-50,release=200
```

### `comp` : tenir le niveau

Égalise les notes (slap, attaques au médiator, jeu aux doigts irrégulier). Au-delà de `threshold`,
le niveau monte `ratio` fois moins vite ; `makeup` remonte ensuite le tout.

- Réglage de départ : `comp:threshold=-24,ratio=4,attack=10,release=100` ; un `attack` plus long
  (20–30 ms) laisse passer le claquement du slap, un `release` court fait « pomper ».
- `mix` < 100 : compression parallèle, le son direct garde ses attaques sous le son tassé.
- Piège : avant le modèle, il change la saturation (les notes fortes saturent moins) ; après, il
  agit sur le son final, comme sur une console. Détecteur de crête, sans lookahead (aucune latence).

```sh
namfoop render -m b7k -i "dg212 sm57" --post comp:threshold=-24,ratio=4,makeup=4 -o comp.wav
```

### Filtres et EQ : `hpf`, `lpf`, `peak`, `lowshelf`, `highshelf`

Une bande par effet : on en enchaîne plusieurs avec des `--post` successifs. Après le modèle, ils
sculptent le son sans toucher au grain ; avant, ils changent ce que le modèle sature.

- `hpf` (passe-haut) : `freq`, `q`, `slope` 12 ou 24 dB/oct. Nettoie l'infra et le souffle des HP
  sans perdre le grave utile : 30–40 Hz pour une basse 4 cordes, 25 Hz pour un si grave.
- `lpf` (passe-bas) : adoucit le « fizz » d'un modèle saturé ou les claquements de cordes :
  5–8 kHz. À 24 dB/oct, il atténue de 6 dB à `freq` (deux étages, type Linkwitz-Riley).
- `peak` (cloche) : `freq`, `gain`, `q`. Un creux de 3–4 dB vers 250–400 Hz enlève le « carton » ;
  une bosse vers 700–900 Hz fait ressortir la basse dans le mix ; 2–4 kHz : attaque et présence.
- `lowshelf` / `highshelf` (plateaux) : plus ou moins de grave sous `freq`, ou d'aigu au-dessus ;
  la moitié du gain est atteinte à `freq`.
- Piège : un `q` élevé avec beaucoup de gain sonne nasal, et un passe-haut résonant (`q` > 1) fait
  une bosse juste au-dessus de la coupure.

```sh
namfoop run -m b7k -i "dg212 sm57" --post hpf:freq=35 --post peak:freq=300,gain=-4 --post highshelf:freq=3000,gain=2
namfoop fx peak
```

### Tempo et divisions rythmiques : `--tempo`, `division`

Sur scène, ce qu'on veut d'un écho ce n'est pas « 344 ms », c'est **une croche pointée au tempo du
morceau**. Les effets de temps acceptent donc leur temps de deux façons :

- **en millisecondes**, comme avant : `delay:time=375` ;
- **en division rythmique** : `delay:division=1/8.` — le temps est alors calculé au tempo courant,
  et `time` est ignoré (mais conservé : y revenir le retrouve tel quel).

Les neuf valeurs, du plus court au plus long, dans l'ordre où `←` / `→` les parcourent :

| valeur | la note | en temps | à 120 BPM |
|---|---|---|---|
| `libre` | — | c'est `time` qui agit | `time` |
| `1/16` | double-croche | un quart de temps | 125 ms |
| `1/8T` | triolet de croche | un tiers de temps | 167 ms |
| `1/8` | croche | un demi-temps | 250 ms |
| `1/8.` | croche pointée | trois quarts de temps | 375 ms |
| **`1/4`** | **noire** | **un temps** | **500 ms** |
| `1/4.` | noire pointée | un temps et demi | 750 ms |
| `1/2` | blanche | deux temps | 1 s |
| `1/1` | ronde | quatre temps | 2 s |

**Attention au piège de la fraction.** `1/4` ne veut **pas** dire « un quart de temps » : c'est une
**noire**, donc **un temps** — les fractions sont relatives à la *ronde*, pas au temps. C'est la
convention de tous les pédaliers et de tous les séquenceurs, sans exception : à 120 BPM, `1/4` vaut
500 ms partout. Et un quart de temps à 120 BPM, c'est 125 ms, que tout le monde appelle une
**double-croche**, `1/16` — exactement ce que fait namfoop. La colonne « la note » ci-dessus est là
pour que la question ne se pose pas, et le panneau de réglages de `browse` la rappelle sous le
curseur : `1/4 = une noire, soit un temps (500 ms à 120 BPM)`.

`libre` est la valeur par défaut : sans rien changer, un delay se règle en millisecondes comme il
l'a toujours fait. `1/8.` (la croche pointée) est le réglage de delay le plus répandu ; `1/4` suit
simplement la pulsation.

**Les divisions courtes ne s'entendent pas comme un rythme**, et ce n'est pas un défaut : à 120 BPM,
`1/16` place sa répétition 125 ms après la note, au même niveau que n'importe quelle autre division
(mesuré : -15,8 dB sur un pincement de basse, identique de `1/16` à `1/4`). Mais 125 ms sous une
note qui sonne encore, l'oreille ne les sépare pas : ça **épaissit** au lieu de répéter. Pour
entendre des double-croches, il faut monter `mix` et `feedback`.

**D'où vient le tempo.** Le tempo appartient au **preset** : changer de preset change le tempo. Il
s'écrit `tempo_bpm` dans le `.preset.toml`, entre 30 et 300 BPM, et vaut 120 quand le preset n'en
porte pas. En ligne de commande, `--tempo` le donne directement — indispensable pour comparer deux
réglages avec `render`, où rien n'est laissé au hasard.

**Taper le tempo : `*`.** Dans `browse`, espace **Chaîne** (`2`), la touche `*` (à droite de `ù` sur
un clavier AZERTY, sans Maj) tape le tempo. Elle n'agit pas dans la Bibliothèque : le banc d'essai
n'est pas un preset, il n'a ni volume ni tempo propres. **Deux frappes suffisent** ; au-delà, c'est la moyenne des quatre derniers
intervalles, donc taper quatre à cinq fois donne une valeur plus sûre. Une frappe trop écartée des
précédentes (une hésitation, ou un autre tempo) **repart** une séquence au lieu de polluer la
moyenne, et trois secondes sans frappe la terminent.

L'en-tête affiche en permanence le tempo **et d'où il vient** : `♩ 104 (tap)`, `♩ 92 (preset)`.
Sans cette mention, on ne saurait pas pourquoi le delay ne suit pas ce qu'on croit avoir réglé.

Un tempo tapé **modifie le preset**, comme n'importe quel autre réglage : l'étoile « non
enregistré » apparaît, `S` l'enregistre, et passer au preset suivant restaure le tempo de celui-là.
Autrement dit, un tap qu'on ne garde pas est perdu — c'est voulu, et c'est la même règle que pour
tout le reste de `browse`.

Le tempo est un **réglage vivant** : taper pendant que le delay sonne ne reconstruit rien et ne
coupe aucune répétition. Le passage d'un temps à l'autre fond en 50 ms, sans glissement de hauteur.

```sh
# une croche pointée à 92 BPM : 652 ms / 0,75 = 489 ms, sans calculer
namfoop run -m b7k -i "dg212 sm57" --tempo 92 --post delay:division=1/8.,feedback=40,mix=25
```

**Parcours type.** Dans `browse`, espace Chaîne, curseur sur le bloc `delay`, `Entrée` ouvre ses
réglages : `division` se parcourt à `←` / `→`. Dès qu'une note est choisie, la ligne `time` passe
**entre parenthèses** avec sa valeur calculée — c'est le signe qu'elle ne se règle plus. `S`
enregistre le preset, tempo compris.

### Le clic du métronome : `k`, `run --click`

Un clic au tempo, **dans les in-ears seulement**. Utile quand tu joues seul, ou quand le clic envoyé
dans le mix global ne te convient pas.

**Où il va.** Dans les sorties que `namfoop setup` déclare **« in-ear »**, et nulle part ailleurs —
jamais dans le sondier. C'est la même information que tu donnes déjà à l'assistant, donc il n'y a
rien de plus à régler : une sortie in-ear reçoit le clic *si* tu l'allumes. Pour l'envoyer ailleurs,
`click = true` sur une sortie de `[routing]` dans la config (mais `setup` reprendra la règle des
in-ears à la prochaine exécution).

**Comment on l'allume.** `k` dans `browse`, `--click` pour `run`. Il est **éteint au démarrage**,
toujours : c'est une aide d'écoute, pas un son, et il ne s'enregistre donc pas dans un preset.
Allumé, l'en-tête l'affiche à côté du tempo : `♩ 104 (tap) ·clic`.

```sh
namfoop run --preset "chill verse" --click      # le clic dans les in-ears, au tempo du preset
```

**Son réglage** vit dans `[metronome]` de la config :

| | |
|---|---|
| `level_db` | niveau du clic (défaut **-18**) |
| `beats_per_bar` | temps par mesure (défaut **4**) ; le premier est plus aigu — c'est l'accent. `1` : aucun accent |

Le clic suit le **même tempo** que les effets de temps : il suit donc le tap (`*`), le tempo du
preset, et l'horloge MIDI quand elle arrivera. Taper recale le **temps 1** : on tape sur le premier
temps. Changer de tempo ne redémarre pas la mesure — seul l'écart entre deux temps change.

**Si `k` ne fait rien**, c'est qu'aucune sortie ne reçoit le clic : le message le dit, et la réponse
est `namfoop setup` (déclarer une sortie « in-ear »). `run --click` refuse de démarrer dans ce cas
plutôt que de jouer sans clic en silence.

**Un piège à connaître** : le clic est ajouté **après** le limiteur, puisqu'il n'a rien à voir avec
le son de la basse. Sur une sortie déjà au plafond, la somme peut donc écrêter. Si tu entends un
grésillement quand le clic tombe sur une note forte, baisse `level_db` (ou `--master-db`) : le
limiteur ne protège que la basse.

**À savoir.**

- Le tampon du delay fait **2 secondes**. Une division plus longue est **écrêtée** et le dit
  (`(2000 ms, plafond)`) : à 60 BPM, une ronde demanderait 4 s. En pratique, cela ne concerne que
  les tempos lents avec `1/2` ou `1/1`.
- Un changement de tempo **fond** entre deux têtes de lecture en 50 ms : pas de clic, et pas de
  glissement de hauteur comme sur certaines pédales.
- `namfoop fx delay` liste les valeurs acceptées, et un nom inconnu les rappelle au lieu de dire
  « n'est pas un nombre ».
- Le tempo pourra aussi venir d'une **horloge MIDI** (à venir) : elle aura alors la priorité sur le
  preset, un `*` reprendra la main dessus, et l'en-tête dira laquelle des deux parle.

### `delay` : écho

Répète le son toutes les `time` ms. La réinjection est **filtrée** : chaque répétition passe par un
passe-haut (`lowcut`) et un passe-bas (`highcut`), elle devient plus fine et plus sombre que la
précédente et ne brouille pas le grave.

- `time` : 1 à 2000 ms. Un écho court (80–150 ms) épaissit, un écho long (300–500 ms) se cale sur
  le tempo — mais plutôt que de calculer `60000 / BPM`, mieux vaut régler `division` (ci-dessus) et
  laisser le tempo faire le calcul.
- `division` : le temps **au tempo** (`1/8.`, `1/4`…) ; `libre` (le défaut) rend la main à `time`.
- `feedback` : nombre de répétitions, de 0 (une seule) à 90 %. Plafonné à 90 % : la queue est alors
  très longue (une minute) mais ne s'emballe jamais.
- `mix` : niveau des répétitions ; le son direct reste toujours à 100 %.
- `lowcut` (80 Hz par défaut) : à monter vers 200–300 Hz pour que les répétitions laissent la place
  à la fondamentale.
- Piège : changer un paramètre reconstruit la chaîne et coupe les répétitions en cours (prototype ;
  le maintien des queues viendra avec les presets). Le **tempo**, lui, est déjà vivant : en changer
  ne rebâtit rien et ne coupe aucune répétition.

```sh
namfoop run -m b7k -i "dg212 sm57" --post delay:time=375,feedback=40,lowcut=200,mix=25
```

### `chorus` : ondulation, grave sec

Mélange au son une copie légèrement retardée dont le retard ondule (`rate`, `depth`). Sous
`lowcut`, le son reste **sec** : la fondamentale garde son assise, seul le médium-aigu ondule.

- `rate` : vitesse de l'ondulation (0,5–1 Hz : large et lent ; 3–5 Hz : vibrato).
- `depth` : amplitude de la modulation du retard, en ms (plus : plus désaccordé).
- `delay` : retard minimal de la copie (5–10 ms : chorus ; 1–3 ms : proche d'un flanger, sans
  réinjection).
- `lowcut` : 150–300 Hz pour une basse ; `mix` : part de la copie au-dessus de `lowcut` (100 % : que
  la copie, vibrato pur dans le médium).

```sh
namfoop run -m b7k -i "dg212 sm57" --post chorus:rate=0.8,depth=2,lowcut=200,mix=50
```

### `reverb` : un peu d'espace

Une réverbe (réseau de 8 lignes à retard) : surtout utile dans les **in-ears**, où le son sec paraît
enfermé. À la basse, elle reste discrète et **hors du grave** : sous `lowcut`, la part réverbérée
est coupée (24 dB/oct), la fondamentale reste nette.

- `decay` : durée de la queue (RT60, temps pour perdre 60 dB), de 0,2 à 10 s ; 0,8–1,5 s suffisent.
- `predelay` : attente avant la réverbe ; 10–30 ms gardent l'attaque de la note lisible.
- `damping` : amortissement des aigus dans la queue (plus : plus sombre, plus naturel).
- `lowcut` : 150–300 Hz pour une basse ; `mix` : niveau de la réverbe, **bas** (10–20 %), le son
  direct reste à 100 %.
- Piège : changer un paramètre reconstruit la chaîne et coupe la queue (prototype).

```sh
namfoop run -m b7k -i "dg212 sm57" --post reverb:decay=1.2,predelay=20,lowcut=250,mix=15
```

### Blend grave clair : `--crossover`

Un ampli saturé « mange » le grave : la fondamentale se brouille et perd son assise. Le blend garde,
sous une fréquence, le **grave clair du DI**, et ne laisse au modèle que le reste.

```
            ┌─ passe-bas ─ grave clair (DI) ─ retard ─┐
DI ─ pre ───┤                                          ├─ somme ─ post ─ niveau ─ limiteur
            └─ passe-haut ─ modèle NAM ─ IR ───────────┘
```

- `--crossover HZ` : fréquence de coupure (filtres Linkwitz-Riley 24 dB/oct : la somme des deux
  branches est plate). Départ : 100–150 Hz ; plus haut (200 Hz), plus de rondeur claire, moins de
  grain dans le bas-médium.
- La branche grave est **alignée automatiquement** sur le niveau du modèle, mesuré sur ton
  enregistrement de calibration (niveau « match ») ; `--crossover-low-db` et `--crossover-high-db`
  corrigent ensuite chaque branche.
- Le retard du modèle et de l'IR (l'attaque d'une IR arrive souvent après 0,5 à 2 ms) est mesuré et
  compensé sur la branche claire : sans cela, les deux branches s'annuleraient en partie autour de la
  coupure (creux de plusieurs dB). La ligne `◆ bandes` du log donne les branches, l'alignement et
  les retards compensés.
- Piège : sans calibration, l'alignement se fait sur une note synthétique à -30 dBFS ; avec un
  autre niveau de jeu, ajuster `--crossover-low-db` à l'oreille.

```sh
namfoop run -m b7k -i auto --crossover 150
namfoop run -m b7k -i auto --crossover 120 --crossover-low-db -3
```

#### La chaîne est une suite de blocs

`--crossover` est un **raccourci**. Depuis l'[ADR 0006](adr/0006-chaine-en-blocs-et-parallele.md), la
chaîne est une suite de **blocs** entre l'entrée et la sortie : un bloc est un effet, un **modèle
NAM**, une **IR**, ou un **parallèle**. Le modèle n'est donc plus coincé entre `pre` et `post` : il
se place où tu veux, y compris dans une branche, et tu peux en mettre **plusieurs** (une fuzz
capturée devant un ampli, par exemple ; leur coût processeur est mesuré, #111).

Un parallèle a **2 à 4 branches** et deux modes :

- **copie** : chaque branche reçoit tout le signal (compression parallèle, deux amplis mélangés) ;
- **bandes** : 1 à 3 coupures croissantes (crossover Linkwitz-Riley 24 dB/oct, somme plate) ; une
  seule coupure, c'est exactement `--crossover`.

Chaque branche a un gain, une polarité et un muet ; une branche **vide** est le son direct, mise
automatiquement au niveau de la première branche qui contient un modèle. Il n'y a qu'**un seul
niveau** : pas de parallèle dans une branche (plusieurs parallèles peuvent se suivre en série).

Les branches ne se décrivent **pas en ligne de commande** (ce serait illisible) : `--pre split` et
`--post split` sont refusés — le parallèle n'est pas un effet, il n'est pas au catalogue de
`namfoop fx` ni de `browse`. Pour autre chose que `--crossover`, ça passe par l'éditeur de chaîne de
`browse` et les presets (#6).

---

## 11. Backends, carte et routage en ligne de commande

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
- `--capture-device` (ALSA) : device de capture, si l'entrée n'est pas sur le même device que la
  sortie (certains HAT I2S exposent la capture sur un autre device, ou une carte sert d'entrée et
  une autre de sortie). Par défaut, le même que `--device`. Avec deux cartes différentes, leurs
  horloges ne sont pas liées : namfoop prévient, et une dérive (xruns réguliers après quelques
  minutes) est possible. Préférer une seule carte sur scène.
- `--buffer` : taille de période en frames. JACK n'accepte que des puissances de 2 (32, 64...).
  Avec l'iTwo : 48 en ALSA, 64 en JACK ; 32 et en dessous provoquent des xruns.
- `--periods` : nombre de périodes côté ALSA (3 par défaut, meilleur compromis mesuré).
- `--rate` : fréquence d'échantillonnage (48000 ; les modèles sont entraînés à 48 kHz). En JACK,
  c'est le serveur qui décide : namfoop refuse de démarrer s'il tourne à une autre fréquence.
- `--client-name` : nom du client JACK (pour lancer plusieurs instances).

**Formats ALSA.** namfoop essaie dans l'ordre `S32_LE`, `S24_3LE`, `S24_LE` puis `S16_LE`, pour la
capture et la lecture séparément, et garde le premier accepté par la carte (ligne « format ALSA »
au démarrage). L'iTwo reste en `S32_LE` ; beaucoup de cartes USB Behringer n'acceptent que
`S24_3LE`, et les HAT I2S souvent `S16_LE` ou `S24_LE`. Si aucun ne passe, namfoop le dit
(« aucun format supporté par la carte ») : essayer `plughw:` au lieu de `hw:` (conversion par ALSA,
un peu plus de latence).

```sh
namfoop --backend alsa --device hw:1,0 --capture-device hw:1,1 --buffer 48 latency
```

**En ALSA, si PipeWire utilise la carte**, namfoop coupe son profil le temps de la session et le
remet à l'arrêt, y compris en cas de plantage. Seul un `kill -9` peut laisser le profil sur « off »
([§ 15](#15-dépannage)).

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

### Profils de carte

**Pourquoi.** Ce qui change d'une carte son à l'autre (device, buffer et périodes qui tiennent sans
xrun, entrée de la basse, contrôles du mixer à régler, latence mesurée) est rangé dans un petit
fichier TOML : changer de carte (iTwo, UMC404HD, HAT I2S sur le Pi) ne demande ni option ni code.

Un profil est livré pour l'iTwo (`profiles/itwo.toml`). Les tiens vont dans
`~/.config/namfoop/profiles/<id>.toml` (à côté de la config) et priment à identifiant égal.
`setup` (backend ALSA) écrit `[audio] profile = "<id>"` : il reprend le profil qui reconnaît la
carte choisie, ou en crée un nouveau avec tes choix, à compléter.

```toml
# ~/.config/namfoop/profiles/hat.toml
name = "HAT I2S du Pi"
match = "sndrpihifiberry"  # nom ALSA de la carte (/proc/asound/<id>, aplay -l)
device = "hw:sndrpihifiberry"
buffer = 64
periods = 3
bass_in = 1
measured_latency_ms = 4.8  # mesuré avec namfoop latency, pour mémoire
[[mixer]]                  # réglé à l'ouverture (ALSA) : gain du préampli, sélection d'entrée...
control = "Capture"        # nom d'après amixer -c <carte> scontrols
value = "80%"              # « 80% », « -6dB », « on » / « off », entier brut ou nom d'un choix
```

```sh
namfoop --backend alsa latency      # profil reconnu : ni --device ni --buffer à donner
```

Ordre de priorité : option de la ligne de commande > config > profil > défaut. Le profil choisi
est `[audio] profile` ; sans lui, celui dont `match` apparaît dans le nom d'une carte branchée.

Pièges :
- le profil ne sert qu'en **ALSA direct** : JACK a ses propres noms de carte et n'accepte que des
  buffers en puissance de 2 ; avec `--no-config` et en carte simulée, aucun profil n'est lu ;
- un contrôle `[[mixer]]` absent ou une valeur refusée donne un avertissement, la carte s'ouvre
  quand même : vérifier le nom avec `amixer -c <carte> scontrols` ;
- un profil inconnu dans `[audio] profile` est une erreur qui liste les profils trouvés.

---

## 12. Travailler sans jouer : rendu, bench, inventaire

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

## 13. Mesurer la latence

Relie une **sortie** à une **entrée** avec un câble (par défaut sortie 1 -> entrée 2), gain de
l'entrée bas, bouton Mixer côté Playback, casque bas (la mesure émet des clics) :

```sh
namfoop latency
namfoop --backend alsa --device hw:iTwo --buffer 48 latency --count 20
```

`--out-ch` / `--in-ch` choisissent les canaux, `--count` le nombre de mesures. Le résultat est
l'aller-retour complet (conversion de la carte comprise). Chaque ligne donne aussi le **niveau du
retour** et le **bruit de fond** : c'est ce qui permet de voir tout de suite si le câble, le gain ou
le bouton Mixer ne vont pas.

Le retour est cherché au **maximum** du signal dans les 250 ms qui suivent l'impulsion, comparé au
bruit de fond mesuré juste avant — pas avec un seuil fixe. Il se mesure donc à n'importe quel
niveau, même quand PipeWire atténue et étale l'impulsion (la sortie à 85 % ne donnait aucune mesure
avant, #70). Deux pièges qui restent :

- **ne joue pas pendant la mesure** : une note plus forte que le retour serait prise pour lui. Les
  colonnes `min` / `max` et l'avertissement « mesures instables » le montrent.
- si rien ne sort du bruit, le message dit quoi vérifier (câble, bouton Mixer, volume de la sortie)
  et propose `--backend alsa`, qui court-circuite PipeWire.

---

## 14. Le fichier de configuration

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
profile = "itwo"                     # optionnel : profil de carte (sinon reconnu d'après la carte)
device = "hw:iTwo"
# capture_device = "hw:1,1"          # optionnel (ALSA) : capture sur un autre device
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

## 15. Dépannage

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

## 16. Limites actuelles

namfoop est un prototype. Les effets sont là (gate, compresseur, EQ, crossover, chorus, delay,
réverbe : [§ 10](#10-effets)), mais changer un réglage coupe les queues de delay et de réverbe.
Pas encore disponibles : presets (une chaîne se garde sous forme de commande), changement de son
par MIDI, traitement du micro, démarrage automatique, version Raspberry Pi. Voir le [CHANGELOG](../CHANGELOG.md) pour suivre les versions.
