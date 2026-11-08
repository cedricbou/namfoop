#!/usr/bin/env bash
# Installe les hooks git qui imposent le cycle issue -> branche -> PR -> merge
# (la protection de branche GitHub n'est pas disponible sur ce dépôt privé) :
#   pre-commit  refuse un commit sur main ; lance scripts/check.sh --quick
#   commit-msg  ajoute « Refs #n » (n° d'issue de la branche) si le message n'en cite aucune
#   pre-push    refuse de pousser sur main (passer par scripts/feature.sh merge)
# Contourner exceptionnellement : NAMFOOP_ALLOW_MAIN=1 git ... ou git commit --no-verify
set -euo pipefail
cd "$(dirname "$0")/.."
dir=$(git rev-parse --git-path hooks)
mkdir -p "$dir"

cat > "$dir/pre-commit" <<'HOOK'
#!/usr/bin/env bash
b=$(git branch --show-current)
if [[ ($b == main || $b == master) && -z ${NAMFOOP_ALLOW_MAIN:-} ]]; then
  echo "✗ pas de commit direct sur $b : scripts/feature.sh start <issue> pour ouvrir une branche" >&2
  exit 1
fi
exec scripts/check.sh --quick
HOOK

cat > "$dir/commit-msg" <<'HOOK'
#!/usr/bin/env bash
msg=$1
b=$(git branch --show-current)
if [[ $b =~ ^(feat|fix|docs)/([0-9]+)- ]] && ! grep -qE '#[0-9]+' "$msg"; then
  n=${BASH_REMATCH[2]}
  # avant les trailers éventuels (Co-Authored-By...), sinon à la fin
  if grep -qE '^[A-Za-z-]+: ' "$msg"; then
    awk -v r="Refs #$n" 'BEGIN{d=0} /^[A-Za-z-]+: /&&!d{print r; print ""; d=1} {print}' "$msg" > "$msg.tmp" && mv "$msg.tmp" "$msg"
  else
    printf '\nRefs #%s\n' "$n" >> "$msg"
  fi
fi
HOOK

cat > "$dir/pre-push" <<'HOOK'
#!/usr/bin/env bash
while read -r _local_ref _local_sha remote_ref _remote_sha; do
  if [[ $remote_ref == refs/heads/main || $remote_ref == refs/heads/master ]] && [[ -z ${NAMFOOP_ALLOW_MAIN:-} ]]; then
    echo "✗ pas de push direct sur main : PR puis scripts/feature.sh merge" >&2
    exit 1
  fi
done
HOOK

chmod +x "$dir/pre-commit" "$dir/commit-msg" "$dir/pre-push"
echo "hooks installés dans $dir : pre-commit, commit-msg, pre-push"
