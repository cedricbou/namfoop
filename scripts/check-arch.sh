#!/usr/bin/env bash
# Sens des dépendances entre crates (docs/architecture.md). Appelé par scripts/check.sh.
# Lit `cargo metadata` et refuse toute arête interdite par la table ci-dessous.
#
# Table des règles, une par ligne :
#   <crate> : interdit <dép> <dép> ...   -> ces dépendances directes sont refusées
#   <crate> : seulement <dép> <dép> ...  -> toute autre dépendance directe est refusée
# Sont vérifiées les dépendances normales et de build ; les dev-dependencies (tests) sont libres.
# Chaque issue qui crée une crate ajoute ici ses règles.
#
# SEULES LES ARÊTES DIRECTES sont lues, et c'est l'intention : `engine` interdit `toml` alors que
# `engine -> model -> toml` existe, parce que `model` ne ré-exporte pas `toml` — `engine` ne peut
# donc pas s'en servir sans le déclarer, et ce serait refusé. Un contrôle transitif signalerait ici
# un non-problème (les types de `model` sont faits pour être partagés) et pousserait à retirer
# `toml` de `model`, qui en a besoin pour les lire.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

RULES='
nam-sys  : seulement thiserror cc
dsp      : interdit alsa clap ratatui libc namfoop audio-io midi-io engine model
audio-io : interdit clap ratatui dsp namfoop toml serde nam-sys engine midi-io
midi-io  : seulement alsa libc log rtrb thiserror
engine   : interdit clap ratatui namfoop toml alsa midi-io
model    : seulement serde toml thiserror
library  : interdit audio-io midi-io engine clap ratatui alsa
'

cargo metadata --format-version 1 --no-deps --offline 2>/dev/null >/dev/null \
  || cargo metadata --format-version 1 --no-deps >/dev/null # télécharge l'index si besoin
cargo metadata --format-version 1 --no-deps --offline 2>/dev/null | RULES=$RULES python3 -c '
import json, os, sys

meta = json.load(sys.stdin)
deps = {
    p["name"]: sorted({d["name"] for d in p["dependencies"] if d["kind"] in (None, "build")})
    for p in meta["packages"]
}
errors = []
for n, line in enumerate(os.environ["RULES"].strip().splitlines(), 1):
    crate, _, rule = (x.strip() for x in line.partition(":"))
    mode, *names = rule.split()
    if crate not in deps:
        errors.append(f"règle {n} : crate « {crate} » inconnue (renommée ? mettre la table à jour)")
        continue
    if mode == "interdit":
        bad = [d for d in deps[crate] if d in names]
    elif mode == "seulement":
        bad = [d for d in deps[crate] if d not in names]
    else:
        errors.append(f"règle {n} : mode « {mode} » inconnu (interdit | seulement)")
        continue
    for d in bad:
        errors.append(f"{crate} -> {d} : dépendance interdite ({line.strip()})")
if errors:
    print("\n".join("✗ " + e for e in errors), file=sys.stderr)
    print("Voir docs/architecture.md : les dépendances ne vont que vers le bas.", file=sys.stderr)
    sys.exit(1)
print(f"{len(deps)} crates, sens des dépendances respecté")
'

# Seule `engine` ouvre la carte : aucun audio_io::start / start_with ailleurs.
if hits=$(grep -rnE 'audio_io::(start|start_with)\b' crates --include='*.rs' \
  | grep -vE '^crates/(engine|audio-io)/'); then
  echo "✗ flux audio ouvert hors d'engine (passer par engine::Engine ou engine::raw) :" >&2
  echo "$hits" >&2
  echo "Voir docs/architecture.md." >&2
  exit 1
fi
echo "carte ouverte seulement par engine"

# Un port MIDI ne s'ouvre qu'à deux endroits : la traduction des options (cli_midi.rs), qui tient la
# règle « un port absent n'empêche jamais de démarrer », et `namfoop midi`, qui affiche l'erreur
# telle quelle. Partout ailleurs, on reçoit un `midi_io::Input` déjà ouvert.
if hits=$(grep -rnE 'midi_io::open\b' crates --include='*.rs' \
  | grep -vE '^crates/(midi-io/|namfoop/src/(cli_midi|midi_cmd)\.rs)'); then
  echo "✗ port MIDI ouvert ailleurs que dans cli_midi.rs ou midi_cmd.rs :" >&2
  echo "$hits" >&2
  echo "Passer par crate::cli_midi::MidiArgs::open, qui ne fait jamais échouer le démarrage." >&2
  exit 1
fi
echo "port MIDI ouvert seulement par les options et « namfoop midi »"

# `dsp` ne crée aucun thread : c'est l'appelant qui décide où le DSP tourne. Jusqu'ici la règle
# était écrite dans docs/architecture.md sans être vérifiée.
if hits=$(grep -rnE '\bstd::thread\b|\bthread::(spawn|Builder)\b' crates/dsp/src --include='*.rs'); then
  echo "✗ dsp crée un thread : c'est à l'appelant (engine) de décider où le DSP tourne :" >&2
  echo "$hits" >&2
  exit 1
fi
echo "dsp sans thread"

# Toutes les couleurs des écrans viennent de `namfoop::theme` : une teinte s'y change une fois, et
# elle change partout où elle a ce sens. `Color` reste un **type** utilisable (un paramètre, un
# champ) ; c'est nommer une teinte en dur qui est refusé.
if hits=$(grep -rnE 'Color::[A-Za-z]' crates/namfoop/src --include='*.rs' \
  | grep -v '^crates/namfoop/src/theme.rs:'); then
  echo "✗ couleur en dur hors de crates/namfoop/src/theme.rs :" >&2
  echo "$hits" >&2
  echo "Ajouter ou réutiliser une fonction de theme, nommée par ce que la couleur signifie." >&2
  exit 1
fi
echo "couleurs des écrans toutes dans theme"
