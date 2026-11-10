# Référence des commandes namfoop

> Généré par `scripts/gen-cli-doc.sh` depuis l'aide intégrée (`namfoop <commande> --help`).
> Ne pas éditer à la main : modifier les textes d'aide dans le code puis régénérer.
> Pour les explications et les cas d'usage, voir le [guide utilisateur](guide.md).

Les options globales (`--backend`, `--device`, `--buffer`...) se placent **avant** la
commande : `namfoop --backend alsa --buffer 48 run -m b7k`.

## namfoop

```text
Multi-effet basse temps réel (NAM + IR) — JACK, ALSA direct ou carte simulée

Usage: namfoop [OPTIONS] <COMMAND>

Commands:
  run        Joue en temps réel : entrée basse -> [NAM -> IR] -> sorties selon leur rôle
  render     Traite un WAV hors ligne avec la même chaîne que `run`
  bench      Mesure le coût CPU des modèles NAM (fichiers, dossiers ou mots-clés ; défaut : bibliothèque)
  import     Importe des packs (zip, dossier, .nam, .wav) : rangement, vérification, fichiers identiques, type de capture
  meta       Affiche ou corrige les métadonnées d'un modèle (type de capture, entrée, IR préférée…)
  models     Liste les modèles et IR avec leur identifiant, et l'IR conseillée pour chaque modèle
  calibrate  Mesure le niveau de ta basse (joue fort ~15 s) et règle le trim d'entrée + le volume
  browse     Navigateur plein écran qui joue en direct : modèles, IR conseillées, mélanges d'IR à l'oreille
  setup      Assistant plein écran : backend, carte, entrées (vumètres), sorties (bip), buffer
  ports      Liste les ports JACK (audio + MIDI)
  latency    Mesure la latence aller-retour (câble sortie -> entrée nécessaire)
  help       Print this message or the help of the given subcommand(s)

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

Exemples :
  namfoop setup              (une fois : carte, entrées, sorties, buffer)
  namfoop calibrate          (une fois : règle le niveau de ta basse)
  namfoop browse             (choisir modèle / IR / mélange en jouant)
  namfoop run -m b7k
  namfoop run -m "800d modern" -i "dg212 sm57" --route 2=di
  namfoop render -m b7k -o /tmp/out.wav
  namfoop bench --all-sizes
  namfoop import ~/Téléchargements/pack.zip
  namfoop models

Réglages enregistrés dans ~/.config/namfoop/config.toml ; les options priment.
```

## namfoop run

```text
Joue en temps réel : entrée basse -> [NAM -> IR] -> sorties selon leur rôle

Usage: namfoop run [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

  -m, --model <MODEL>
          Modèle NAM : identifiant (`namfoop models`), chemin, ou mots-clés (ex: "b7k")

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

  -i, --ir <IR>
          IR de baffle (.wav) ou mélange (.cab.toml) : identifiant (`namfoop models`), chemin, mots-clés (ex: "dg212 sm57") ou « auto » (la mieux conseillée pour le modèle)

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --input-db <INPUT_DB>
          Gain avant ce modèle (dB), en plus du trim : plus = plus de saturation
          
          [default: 0]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --output-db <OUTPUT_DB>
          Gain après le modèle (dB)
          
          [default: 0]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --trim <TRIM>
          Trim d'entrée global de l'instrument (dB). Défaut : valeur de la config (`calibrate`)

      --level <LEVEL>
          Volume de sortie : match (= basse en direct), normalize (-18 dB), raw. Défaut : config

          Possible values:
          - match:     Volume traité aligné sur le volume de la basse en direct (nécessite `calibrate`)
          - normalize: Loudness du modèle ramenée à `target_db` (convention NAM)
          - raw:       Sortie brute du modèle

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --target-db <TARGET_DB>
          Cible du mode normalize (dB)

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-limiter
          Désactive le limiteur de sortie

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

      --size <SIZE>
          Taille des modèles slimmable : 0 = le plus léger, 1 = complet
          
          [default: 1]

      --ir-ms <IR_MS>
          Longueur max de l'IR (ms)
          
          [default: 200]

      --master-db <MASTER_DB>
          Volume final (dB)
          
          [default: 0]

      --library <LIBRARY>
          Dossier de la bibliothèque (modèles et IR)
          
          [default: library]

      --bass-in <BASS_IN>
          Entrée de la carte où est branchée la basse [défaut : config, sinon 1]

      --mic-in <MIC_IN>
          Entrée du micro [défaut : config]

      --route <N=SOURCE>
          Rôle d'une sortie, N=source avec source = processed, di, mic ou mute (répétable, ex: --route 1=processed --route 2=di) [défaut : config, sinon 1 et 2 = processed]

      --seconds <SECONDS>
          Arrêt automatique après N secondes (tests)

  -q, --quiet
          Pas de vumètre (logs seulement)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop render

```text
Traite un WAV hors ligne avec la même chaîne que `run`

Usage: namfoop render [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

  -m, --model <MODEL>
          Modèle NAM : identifiant (`namfoop models`), chemin, ou mots-clés (ex: "b7k")

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

  -i, --ir <IR>
          IR de baffle (.wav) ou mélange (.cab.toml) : identifiant (`namfoop models`), chemin, mots-clés (ex: "dg212 sm57") ou « auto » (la mieux conseillée pour le modèle)

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --input-db <INPUT_DB>
          Gain avant ce modèle (dB), en plus du trim : plus = plus de saturation
          
          [default: 0]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --output-db <OUTPUT_DB>
          Gain après le modèle (dB)
          
          [default: 0]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --trim <TRIM>
          Trim d'entrée global de l'instrument (dB). Défaut : valeur de la config (`calibrate`)

      --level <LEVEL>
          Volume de sortie : match (= basse en direct), normalize (-18 dB), raw. Défaut : config

          Possible values:
          - match:     Volume traité aligné sur le volume de la basse en direct (nécessite `calibrate`)
          - normalize: Loudness du modèle ramenée à `target_db` (convention NAM)
          - raw:       Sortie brute du modèle

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --target-db <TARGET_DB>
          Cible du mode normalize (dB)

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-limiter
          Désactive le limiteur de sortie

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

      --size <SIZE>
          Taille des modèles slimmable : 0 = le plus léger, 1 = complet
          
          [default: 1]

      --ir-ms <IR_MS>
          Longueur max de l'IR (ms)
          
          [default: 200]

      --master-db <MASTER_DB>
          Volume final (dB)
          
          [default: 0]

      --library <LIBRARY>
          Dossier de la bibliothèque (modèles et IR)
          
          [default: library]

      --in <INPUT>
          WAV d'entrée (DI). Par défaut : signal de test de NeuralAmpModelerCore

  -o, --out <OUTPUT>
          WAV de sortie (flottant 32 bits). Sans : stats seulement

      --block <BLOCK>
          Taille de bloc
          
          [default: 64]

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop bench

```text
Mesure le coût CPU des modèles NAM (fichiers, dossiers ou mots-clés ; défaut : bibliothèque)

Usage: namfoop bench [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...
          Fichiers .nam, dossiers ou mots-clés (défaut : toute la bibliothèque)

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --block <BLOCK>
          Taille de bloc
          
          [default: 64]

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --seconds <SECONDS>
          Secondes d'audio simulées par modèle
          
          [default: 5]

      --all-sizes
          Mesure chaque taille des modèles slimmable

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --library <LIBRARY>
          Dossier de la bibliothèque
          
          [default: library]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop import

```text
Importe des packs (zip, dossier, .nam, .wav) : rangement, vérification, fichiers identiques, type de capture

Usage: namfoop import [OPTIONS] <SOURCES>...

Arguments:
  <SOURCES>...
          Zips, dossiers ou fichiers .nam / .wav à importer (un dossier de zips importe chaque zip)

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --pack <PACK>
          Nom du pack (dossier dans la bibliothèque) [défaut : nom du zip ou du dossier]

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --dry-run
          Vérifie et affiche le résultat sans rien écrire

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --skip-duplicates
          N'importe pas un fichier identique (octet par octet) à un fichier déjà dans la bibliothèque ou plus haut dans l'import. Par défaut il est importé quand même, pour que chaque pack reste complet, et signalé

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --review
          Ouvre ensuite le navigateur sur les modèles importés, éditeur ouvert : écoute, type de capture, gain d'entrée, IR préférée (prend la carte son)

      --library <LIBRARY>
          Dossier de la bibliothèque
          
          [default: library]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop meta

```text
Affiche ou corrige les métadonnées d'un modèle (type de capture, entrée, IR préférée…)

Usage: namfoop meta [OPTIONS] <MODEL>

Arguments:
  <MODEL>
          Modèle : identifiant, chemin ou mots-clés

Options:
      --all
          Applique à tous les modèles qui contiennent les mots-clés (ex: "fullrig")

      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --type <GEAR_TYPE>
          Type de capture (décide s'il faut une IR)

          Possible values:
          - amp:      Ampli seul, sans baffle (DI) : une IR est nécessaire
          - amp-cab:  Ampli + baffle (micro compris) : pas d'IR
          - full-rig: Chaîne complète avec baffle : pas d'IR
          - pedal:    Pédale : une IR est nécessaire
          - preamp:   Préampli : une IR est nécessaire

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --make <MAKE>
          Marque de l'ampli (aide au choix des IR conseillées)

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --name <NAME>
          Nom affiché

      --input-db <INPUT_DB>
          Gain d'entrée propre au modèle (dB), ajouté à --input-db ; 0 pour l'annuler

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --ir <IR>
          IR préférée du modèle (identifiant, chemin ou mots-clés) : en tête des conseils et de -i auto

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --reset
          Efface toutes les corrections de ce(s) modèle(s)

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --library <LIBRARY>
          Dossier de la bibliothèque
          
          [default: library]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop models

```text
Liste les modèles et IR avec leur identifiant, et l'IR conseillée pour chaque modèle

Usage: namfoop models [OPTIONS] [LIBRARY]

Arguments:
  [LIBRARY]
          Dossier de la bibliothèque
          
          [default: library]

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop calibrate

```text
Mesure le niveau de ta basse (joue fort ~15 s) et règle le trim d'entrée + le volume

Usage: namfoop calibrate [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --seconds <SECONDS>
          Durée d'enregistrement (s)
          
          [default: 15]

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --target <TARGET>
          Crête robuste visée à l'entrée des modèles (dBFS)
          
          [default: -30]

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --channel <CHANNEL>
          Entrée de la carte où est branchée la basse [défaut : config (setup), sinon 1]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --dry-run
          Analyse seulement, ne sauve rien

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop browse

```text
Navigateur plein écran qui joue en direct : modèles, IR conseillées, mélanges d'IR à l'oreille

Usage: namfoop browse [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --library <LIBRARY>
          Dossier de la bibliothèque
          
          [default: library]

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --size <SIZE>
          Taille des modèles slimmable : 0 = le plus léger, 1 = complet
          
          [default: 1]

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --filter <FILTER>
          Filtre initial de la liste (ex: "PopPunk")

      --bass-in <BASS_IN>
          Entrée de la carte où est branchée la basse [défaut : config, sinon 1]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --mic-in <MIC_IN>
          Entrée du micro [défaut : config]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --route <N=SOURCE>
          Rôle d'une sortie, N=source avec source = processed, di, mic ou mute (répétable, ex: --route 1=processed --route 2=di) [défaut : config, sinon 1 et 2 = processed]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop setup

```text
Assistant plein écran : backend, carte, entrées (vumètres), sorties (bip), buffer

Usage: namfoop setup [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --library <LIBRARY>
          Dossier de la bibliothèque (un modèle sert de charge réaliste au test des buffers)
          
          [default: library]

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop ports

```text
Liste les ports JACK (audio + MIDI)

Usage: namfoop ports [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop latency

```text
Mesure la latence aller-retour (câble sortie -> entrée nécessaire)

Usage: namfoop latency [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio [défaut : config, sinon jack]

          Possible values:
          - jack: JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
          - alsa: ALSA direct : latence minimale et stable, accès exclusif à la carte
          - sim:  Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel

      --out-ch <OUT_CH>
          Canal de sortie qui émet l'impulsion
          
          [default: 1]

      --device <DEVICE>
          Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]

      --in-ch <IN_CH>
          Canal d'entrée où revient le signal
          
          [default: 2]

      --capture-device <CAPTURE_DEVICE>
          Device ALSA de capture, si l'entrée est sur une autre carte ou un autre device que la sortie (ex: "hw:1,0" pour un HAT) [défaut : config, sinon le même que --device]

      --count <COUNT>
          Nombre de mesures
          
          [default: 10]

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

  -h, --help
          Print help (see a summary with '-h')
```
