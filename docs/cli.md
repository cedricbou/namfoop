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
  models     Liste les modèles et IR avec leur identifiant, et l'IR conseillée pour chaque modèle
  calibrate  Mesure le niveau de ta basse (joue fort ~15 s) et règle le trim d'entrée + le volume
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
  namfoop run -m b7k
  namfoop run -m "800d modern" -i "dg212 sm57" --route 2=di
  namfoop render -m b7k -o /tmp/out.wav
  namfoop bench --all-sizes
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

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --input-db <INPUT_DB>
          Gain avant ce modèle (dB), en plus du trim : plus = plus de saturation
          
          [default: 0]

      --output-db <OUTPUT_DB>
          Gain après le modèle (dB)
          
          [default: 0]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --trim <TRIM>
          Trim d'entrée global de l'instrument (dB). Défaut : valeur de la config (`calibrate`)

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --level <LEVEL>
          Volume de sortie : match (= basse en direct), normalize (-18 dB), raw. Défaut : config

          Possible values:
          - match:     Volume traité aligné sur le volume de la basse en direct (nécessite `calibrate`)
          - normalize: Loudness du modèle ramenée à `target_db` (convention NAM)
          - raw:       Sortie brute du modèle

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --target-db <TARGET_DB>
          Cible du mode normalize (dB)

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

      --no-limiter
          Désactive le limiteur de sortie

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

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --input-db <INPUT_DB>
          Gain avant ce modèle (dB), en plus du trim : plus = plus de saturation
          
          [default: 0]

      --output-db <OUTPUT_DB>
          Gain après le modèle (dB)
          
          [default: 0]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]

      --trim <TRIM>
          Trim d'entrée global de l'instrument (dB). Défaut : valeur de la config (`calibrate`)

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

      --level <LEVEL>
          Volume de sortie : match (= basse en direct), normalize (-18 dB), raw. Défaut : config

          Possible values:
          - match:     Volume traité aligné sur le volume de la basse en direct (nécessite `calibrate`)
          - normalize: Loudness du modèle ramenée à `target_db` (convention NAM)
          - raw:       Sortie brute du modèle

      --config <CONFIG>
          Fichier de config [défaut : ~/.config/namfoop/config.toml]

      --target-db <TARGET_DB>
          Cible du mode normalize (dB)

      --no-config
          Ignore la config : réglages par défaut (tests reproductibles, comparaisons)

      --no-limiter
          Désactive le limiteur de sortie

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

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --channel <CHANNEL>
          Entrée de la carte où est branchée la basse [défaut : config (setup), sinon 1]

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

      --buffer <BUFFER>
          Taille de buffer / période en frames [défaut : config, sinon 64]

      --count <COUNT>
          Nombre de mesures
          
          [default: 10]

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
