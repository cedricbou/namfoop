#!/usr/bin/env bash
# Importe des zips Tone3000 (ou autres) dans la bibliothèque :
#   *.nam -> library/nam/<nom du zip>/   *.wav -> library/ir/<nom du zip>/
# Usage : scripts/import-tone3000.sh [dossier des zips, défaut nam/] [bibliothèque, défaut library/]
# Les chemins d'archive absolus ou contenant ".." sont refusés.
set -euo pipefail
SRC=${1:-nam}
LIB=${2:-library}
shopt -s nullglob
for z in "$SRC"/*.zip; do
  stem=$(basename "$z" .zip)
  # liste lue une fois (grep -q + pipefail + SIGPIPE sur unzip = faux négatifs)
  list=$(unzip -Z1 "$z")
  if grep -qE '(^/|\.\./)' <<<"$list"; then
    echo "IGNORÉ (chemins suspects) : $z"; continue
  fi
  for ext in nam wav; do
    sub=$([[ $ext == wav ]] && echo ir || echo nam)
    if grep -qiE "\.$ext$" <<<"$list"; then
      mkdir -p "$LIB/$sub/$stem"
      unzip -qjo "$z" "*.$ext" "*.${ext^^}" -d "$LIB/$sub/$stem" 2>/dev/null || true
    fi
  done
  echo "importé : $stem"
done
echo "$(find "$LIB" -name '*.nam' | wc -l) modèles, $(find "$LIB" -iname '*.wav' | wc -l) IR dans $LIB/"
