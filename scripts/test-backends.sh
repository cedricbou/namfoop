#!/usr/bin/env bash
# Tests MATÉRIELS : namfoop en ALSA direct, le backend de scène.
# Le test prend la carte en exclusif quelques secondes (PipeWire la rend ensuite).
#
# Câblage : la sortie OUT_CH doit revenir dans l'entrée IN_CH — c'est ce que mesure `latency`, et
# sans ce câble il n'y a pas d'essai. Sur l'iTwo (les valeurs par défaut) : sortie 1 -> entrée 2,
# bouton Mixer à fond côté Playback.
#
# Usage : scripts/test-backends.sh [buffer] [périodes]
#
# Variables, toutes aux valeurs de l'iTwo par défaut :
#   ALSA_DEV  carte ALSA                                           hw:iTwo
#   OUT_CH    sortie qui émet l'impulsion, et porte le son traité  1
#   IN_CH     entrée où l'impulsion revient                        2
#   BASS_IN   entrée où est branchée la basse                      1
#   ROUTES    rôles de sortie, séparés par une espace ; VIDE =      2=di
#             aucun, les sorties gardent leur rôle par défaut
#   SMOKE     durée d'un essai de flux, en secondes                10
#   MODEL     modèle NAM de l'essai chargé                         b7k
#   NAMFOOP_BIN  binaire à lancer               target/release/namfoop
#
# Une autre carte se donne donc en entier sur la ligne de commande. Soundcraft Signature 12 MTK,
# basse sur l'entrée 3, sortie 2 (douze sorties : le DI va sur une sortie libre) :
#
#   ALSA_DEV=hw:MTK OUT_CH=2 IN_CH=3 BASS_IN=3 ROUTES="2=processed 3=di" scripts/test-backends.sh
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
BUF=${1:-48}
PER=${2:-3}
# NAMFOOP_BIN comme pour les essais de bout en bout : de quoi pointer un autre binaire, et de quoi
# vérifier les commandes assemblées sans prendre la carte
BIN=${NAMFOOP_BIN:-./target/release/namfoop}
ALSA_DEV=${ALSA_DEV:-hw:iTwo}
OUT_CH=${OUT_CH:-1}
IN_CH=${IN_CH:-2}
BASS_IN=${BASS_IN:-1}
# `-` et non `:-` : ROUTES= (vide) veut dire « aucune route », pas « celle de l'iTwo »
ROUTES=${ROUTES-2=di}
SMOKE=${SMOKE:-10}
MODEL=${MODEL:-b7k}

cargo build --release -q || exit 1
ALSA=("$BIN" --no-config --backend alsa --device "$ALSA_DEV" --buffer "$BUF" --periods "$PER")
LOOP=(--out-ch "$OUT_CH" --in-ch "$IN_CH")
routes=()
for r in $ROUTES; do routes+=(--route "$r"); done

# **Le câblage s'affiche avant les résultats** : un « OK » sans la carte ni les canaux qui l'ont
# produit ne se compare à rien, et c'est justement ce qu'on vient coller dans une issue.
echo "carte $ALSA_DEV · buffer $BUF × $PER · basse sur l'entrée $BASS_IN · bouclage sortie" \
     "$OUT_CH -> entrée $IN_CH · routage ${ROUTES:-par défaut}"

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

check "alsa latence      " "${ALSA[@]}" latency "${LOOP[@]}" --count 10
# les chaînes de référence, dans le trajet puis à côté : latence, charge DSP et xruns par passe
check "alsa latence chaînes" "${ALSA[@]}" latency "${LOOP[@]}" --chains --count 5
check "alsa passthrough  " "${ALSA[@]}" run -q --bass-in "$BASS_IN" --master-db -120 --seconds "$SMOKE"
check "alsa NAM $MODEL    " "${ALSA[@]}" run -q -m "$MODEL" --bass-in "$BASS_IN" "${routes[@]}" \
      --master-db -120 --seconds "$SMOKE"
exit $fail
