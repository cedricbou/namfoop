#!/usr/bin/env bash
# Tests MATÉRIELS : namfoop sur les deux backends (JACK via PipeWire, ALSA direct).
# Câblage : sortie 1 -> entrée 2 de l'iTwo, bouton Mixer de l'iTwo à fond côté Playback.
# Le test ALSA prend la carte en exclusif quelques secondes (PipeWire la rend ensuite).
#
# Usage : scripts/test-backends.sh [buffer] [périodes ALSA]
#   JACK n'accepte que des puissances de 2 (48 -> 64).
# Variables : JACK_DEV (iTwo), ALSA_DEV (hw:iTwo), SMOKE (secondes, 10), MODEL (b7k)
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
BUF=${1:-48}
PER=${2:-3}
BIN=./target/release/namfoop
JACK_DEV=${JACK_DEV:-iTwo}
ALSA_DEV=${ALSA_DEV:-hw:iTwo}
SMOKE=${SMOKE:-10}
MODEL=${MODEL:-b7k}

cargo build --release -q || exit 1
JACK=("$BIN" --no-config --backend jack --device "$JACK_DEV" --buffer "$BUF")
ALSA=("$BIN" --no-config --backend alsa --device "$ALSA_DEV" --buffer "$BUF" --periods "$PER")

fail=0
check() { # nom, commande... : échec si code retour != 0 ou xruns > 0
  local name=$1; shift
  local out; out=$("$@" 2>&1); local rc=$?
  local line; line=$(grep -E 'médiane|arrêt' <<<"$out" | tail -1)
  if [[ $rc -ne 0 ]] || grep -qE 'xruns [1-9]|[1-9][0-9]* xrun' <<<"$line"; then
    echo "ÉCHEC  $name : ${line:-$(tail -1 <<<"$out")}"; fail=1
  else
    echo "OK     $name : ${line#*] }"
  fi
}

for b in jack alsa; do
  [[ $b == jack ]] && cmd=("${JACK[@]}") || cmd=("${ALSA[@]}")
  check "$b latence      " "${cmd[@]}" latency --count 10
  check "$b passthrough  " "${cmd[@]}" run -q --master-db -120 --seconds "$SMOKE"
  check "$b NAM $MODEL    " "${cmd[@]}" run -q -m "$MODEL" --route 2=di --master-db -120 --seconds "$SMOKE"
done
exit $fail
