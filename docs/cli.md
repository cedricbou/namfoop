# Référence des commandes namfoop

> Généré par `scripts/gen-cli-doc.sh` depuis l'aide intégrée (`namfoop <commande> --help`).
> Ne pas éditer à la main : modifier les textes d'aide dans le code puis régénérer.
> Pour les explications et les cas d'usage, voir le [guide utilisateur](guide.md).

Les options globales (`--backend`, `--device`, `--buffer`...) se placent **avant** la
commande : `namfoop --backend alsa --buffer 48 run -m b7k`.

## namfoop

```text
Multi-effet basse temps réel (NAM + IR) — JACK ou ALSA direct

Usage: namfoop [OPTIONS] <COMMAND>

Commands:
  run        Joue en temps réel : entrée 1 -> [NAM -> IR] -> sorties
  render     Traite un WAV hors ligne avec la même chaîne que `run`
  bench      Mesure le coût CPU des modèles NAM (fichiers, dossiers ou mots-clés ; défaut : bibliothèque)
  models     Liste les modèles et IR de la bibliothèque
  calibrate  Mesure le niveau de ta basse (joue fort ~15 s) et règle le trim d'entrée + le volume
  ports      Liste les ports JACK (audio + MIDI)
  latency    Mesure la latence aller-retour (câble sortie -> entrée nécessaire)
  help       Print this message or the help of the given subcommand(s)

Options:
      --backend <BACKEND>
          Backend audio

          Possible values:
          - jack: JACK API (pipewire-jack ou jackd)
          - alsa: ALSA direct, accès exclusif à la carte
          
          [default: jack]

      --device <DEVICE>
          Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo"). Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)

      --buffer <BUFFER>
          Taille de buffer / période en frames
          
          [default: 64]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
          
          [default: 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
          
          [default: 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

Exemples :
  pw-jack namfoop run -m b7k -i "svt 8x10"
  namfoop --backend alsa --buffer 48 run -m "svt classic"
  namfoop calibrate          (une fois : règle le niveau de ta basse)
  namfoop render -m b7k -o /tmp/out.wav
  namfoop bench --all-sizes
  namfoop models
```

## namfoop run

```text
Joue en temps réel : entrée 1 -> [NAM -> IR] -> sorties

Usage: namfoop run [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio

          Possible values:
          - jack: JACK API (pipewire-jack ou jackd)
          - alsa: ALSA direct, accès exclusif à la carte
          
          [default: jack]

  -m, --model <MODEL>
          Modèle NAM : chemin, ou mots-clés cherchés dans la bibliothèque (ex: "b7k")

      --device <DEVICE>
          Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo"). Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)

  -i, --ir <IR>
          IR de baffle (.wav) : chemin ou mots-clés (ex: "dg212 sm57")

      --buffer <BUFFER>
          Taille de buffer / période en frames
          
          [default: 64]

      --input-db <INPUT_DB>
          Gain avant ce modèle (dB), en plus du trim : plus = plus de saturation
          
          [default: 0]

      --output-db <OUTPUT_DB>
          Gain après le modèle (dB)
          
          [default: 0]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
          
          [default: 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
          
          [default: 48000]

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

      --target-db <TARGET_DB>
          Cible du mode normalize (dB)

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

      --config <CONFIG>
          Fichier de config (défaut : ~/.config/namfoop/config.toml)

      --no-config
          Ignore la config (réglages par défaut : tests reproductibles)

      --out2 <OUT2>
          Contenu de la sortie 2

          Possible values:
          - same: Même signal traité que la sortie 1
          - di:   Signal direct de la basse (DI) : pratique pour envoyer un DI propre au sondier
          - mute: Silence
          
          [default: same]

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
          Backend audio

          Possible values:
          - jack: JACK API (pipewire-jack ou jackd)
          - alsa: ALSA direct, accès exclusif à la carte
          
          [default: jack]

  -m, --model <MODEL>
          Modèle NAM : chemin, ou mots-clés cherchés dans la bibliothèque (ex: "b7k")

      --device <DEVICE>
          Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo"). Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)

  -i, --ir <IR>
          IR de baffle (.wav) : chemin ou mots-clés (ex: "dg212 sm57")

      --buffer <BUFFER>
          Taille de buffer / période en frames
          
          [default: 64]

      --input-db <INPUT_DB>
          Gain avant ce modèle (dB), en plus du trim : plus = plus de saturation
          
          [default: 0]

      --output-db <OUTPUT_DB>
          Gain après le modèle (dB)
          
          [default: 0]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
          
          [default: 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
          
          [default: 48000]

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

      --target-db <TARGET_DB>
          Cible du mode normalize (dB)

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

      --config <CONFIG>
          Fichier de config (défaut : ~/.config/namfoop/config.toml)

      --no-config
          Ignore la config (réglages par défaut : tests reproductibles)

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
          Backend audio

          Possible values:
          - jack: JACK API (pipewire-jack ou jackd)
          - alsa: ALSA direct, accès exclusif à la carte
          
          [default: jack]

      --block <BLOCK>
          Taille de bloc
          
          [default: 64]

      --device <DEVICE>
          Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo"). Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)

      --seconds <SECONDS>
          Secondes d'audio simulées par modèle
          
          [default: 5]

      --all-sizes
          Mesure chaque taille des modèles slimmable

      --buffer <BUFFER>
          Taille de buffer / période en frames
          
          [default: 64]

      --library <LIBRARY>
          Dossier de la bibliothèque
          
          [default: library]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
          
          [default: 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
          
          [default: 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop models

```text
Liste les modèles et IR de la bibliothèque

Usage: namfoop models [OPTIONS] [LIBRARY]

Arguments:
  [LIBRARY]
          Dossier de la bibliothèque
          
          [default: library]

Options:
      --backend <BACKEND>
          Backend audio

          Possible values:
          - jack: JACK API (pipewire-jack ou jackd)
          - alsa: ALSA direct, accès exclusif à la carte
          
          [default: jack]

      --device <DEVICE>
          Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo"). Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)

      --buffer <BUFFER>
          Taille de buffer / période en frames
          
          [default: 64]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
          
          [default: 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
          
          [default: 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop calibrate

```text
Mesure le niveau de ta basse (joue fort ~15 s) et règle le trim d'entrée + le volume

Usage: namfoop calibrate [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio

          Possible values:
          - jack: JACK API (pipewire-jack ou jackd)
          - alsa: ALSA direct, accès exclusif à la carte
          
          [default: jack]

      --seconds <SECONDS>
          Durée d'enregistrement (s)
          
          [default: 15]

      --device <DEVICE>
          Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo"). Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)

      --target <TARGET>
          Crête robuste visée à l'entrée des modèles (dBFS)
          
          [default: -30]

      --buffer <BUFFER>
          Taille de buffer / période en frames
          
          [default: 64]

      --channel <CHANNEL>
          Entrée de la carte où est branchée la basse
          
          [default: 1]

      --dry-run
          Analyse seulement, ne sauve rien

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
          
          [default: 3]

      --config <CONFIG>
          Fichier de config à écrire (défaut : ~/.config/namfoop/config.toml)

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
          
          [default: 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop ports

```text
Liste les ports JACK (audio + MIDI)

Usage: namfoop ports [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio

          Possible values:
          - jack: JACK API (pipewire-jack ou jackd)
          - alsa: ALSA direct, accès exclusif à la carte
          
          [default: jack]

      --device <DEVICE>
          Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo"). Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)

      --buffer <BUFFER>
          Taille de buffer / période en frames
          
          [default: 64]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
          
          [default: 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
          
          [default: 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

  -h, --help
          Print help (see a summary with '-h')
```

## namfoop latency

```text
Mesure la latence aller-retour (câble sortie -> entrée nécessaire)

Usage: namfoop latency [OPTIONS]

Options:
      --backend <BACKEND>
          Backend audio

          Possible values:
          - jack: JACK API (pipewire-jack ou jackd)
          - alsa: ALSA direct, accès exclusif à la carte
          
          [default: jack]

      --out-ch <OUT_CH>
          Canal de sortie qui émet l'impulsion (1 ou 2)
          
          [default: 1]

      --device <DEVICE>
          Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo"). Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)

      --in-ch <IN_CH>
          Canal d'entrée où revient le signal (1 ou 2)
          
          [default: 2]

      --buffer <BUFFER>
          Taille de buffer / période en frames
          
          [default: 64]

      --count <COUNT>
          Nombre de mesures
          
          [default: 10]

      --periods <PERIODS>
          Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
          
          [default: 3]

      --rate <RATE>
          Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
          
          [default: 48000]

      --client-name <CLIENT_NAME>
          Nom du client JACK
          
          [default: namfoop]

  -h, --help
          Print help (see a summary with '-h')
```
