#!/usr/bin/env bash
# Tests MATÉRIELS : namfoop en ALSA direct, le backend de scène.
# Câblage : sortie 1 -> entrée 2 de l'iTwo, bouton Mixer de l'iTwo à fond côté Playback.
# Le test prend la carte en exclusif quelques secondes (PipeWire la rend ensuite).
#
# Usage : scripts/test-backends.sh [buffer] [périodes]
# Variables : ALSA_DEV (hw:iTwo), SMOKE (secondes, 10), MODEL (b7k)
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
BUF=${1:-48}
PER=${2:-3}
BIN=./target/release/namfoop
ALSA_DEV=${ALSA_DEV:-hw:iTwo}
SMOKE=${SMOKE:-10}
MODEL=${MODEL:-b7k}

cargo build --release -q || exit 1
ALSA=("$BIN" --no-config --backend alsa --device "$ALSA_DEV" --buffer "$BUF" --periods "$PER")

fail=0
check() { # nom, commande... : échec si code retour != 0 ou xruns > 0
  local name=$1; shift
  local out; out=$("$@" 2>&1); local rc=$?
  local line; line=$(grep -E 'médiane [0-9]|arrêt|^total ' <<<"$out" | tail -1)
  if [[ $rc -ne 0 ]] || grep -qE 'xruns [1-9]|[1-9][0-9]* xrun' <<<"$line"; then
    echo "ÉCHEC  $name : ${line:-$(tail -1 <<<"$out")}"; fail=1
  else
    echo "OK     $name : ${line#*] }"
  fi
}

check "alsa latence      " "${ALSA[@]}" latency --count 10
# les chaînes de référence, dans le trajet puis à côté : latence, charge DSP et xruns par passe
check "alsa latence chaînes" "${ALSA[@]}" latency --chains --count 5
check "alsa passthrough  " "${ALSA[@]}" run -q --master-db -120 --seconds "$SMOKE"
check "alsa NAM $MODEL    " "${ALSA[@]}" run -q -m "$MODEL" --route 2=di --master-db -120 --seconds "$SMOKE"
exit $fail
