#!/usr/bin/env bash
# Vérifie que la documentation suit le code (lancé par scripts/check.sh) :
#  1. docs/cli.md est identique à l'aide actuelle de la CLI (sinon : scripts/gen-cli-doc.sh)
#  2. chaque sous-commande est présentée dans docs/guide.md et citée dans README.md
#  3. chaque option longue de la CLI est expliquée dans docs/guide.md
#  4. chaque exemple « namfoop ... » des blocs ```sh des docs est une ligne de commande valide
#     (et ses mots-clés -m / -i désignent un fichier unique si library/ existe)
#  5. aucun modèle / IR / zip n'est versionné (droits incertains)
#  6. rappel CHANGELOG.md si du code a changé sans entrée de changelog (avertissement)
#  7. chaque crate a son CLAUDE.md, citée dans la carte du CLAUDE.md racine et dans
#     docs/architecture.md
#  8. taille des fichiers .rs (hors tests/ et vendor/) : avertissement > 600 lignes, échec > 1000
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
PROFILE=${NAMFOOP_PROFILE:-release}
BIN=./target/$PROFILE/namfoop
GUIDE=${GUIDE:-docs/guide.md}
fail=0
err() { echo "  ✗ $*"; fail=1; }

cargo build -q --profile "$PROFILE" || exit 1
[[ -f $GUIDE ]] || { echo "  ✗ $GUIDE absent"; exit 1; }

# 1. référence générée à jour
tmp=$(mktemp)
scripts/gen-cli-doc.sh "$tmp" >/dev/null
if ! diff -q "$tmp" docs/cli.md >/dev/null 2>&1; then
  err "docs/cli.md périmé : lancer scripts/gen-cli-doc.sh"
  diff "$tmp" docs/cli.md | head -20 | sed 's/^/      /'
fi

# 2. sous-commandes dans le guide et le README
subs=$("$BIN" --help | sed -n '/^Commands:/,/^$/p' | awk 'NR>1 && $1 != "help" && NF {print $1}')
for c in $subs; do
  grep -qE "namfoop([^\`]* )?$c( |\`|$)" "$GUIDE" || err "sous-commande « $c » absente de $GUIDE"
  grep -qE "namfoop([^\`]* )?$c( |\`|$)" README.md || err "sous-commande « $c » absente de README.md"
done

# 3. options longues expliquées dans le guide
opts=$(grep -oE -- '--[a-z][a-z0-9-]+' "$tmp" | sort -u | grep -vxE -- '--(help|version)')
for o in $opts; do
  grep -qE -- "$o([^a-z0-9-]|$)" "$GUIDE" || err "option $o absente de $GUIDE"
done
rm -f "$tmp"

# 4. exemples exécutables (analyse seule, rien n'est lancé)
check_examples() {
  local file=$1 in_code=0 n=0 line rest
  while IFS= read -r line; do
    if [[ $line =~ ^[[:space:]]*\`\`\`(.*)$ ]]; then
      if (( in_code )); then in_code=0
      elif [[ ${BASH_REMATCH[1]} =~ ^(sh|bash|shell)$ ]]; then in_code=1
      else in_code=2; fi                           # bloc non shell : ignoré
      continue
    fi
    (( in_code == 1 )) || continue
    [[ $line =~ ^[[:space:]]*# ]] && continue
    [[ $line =~ (^|[[:space:]/])namfoop[[:space:]] ]] || continue
    [[ $line == *"<"*">"* ]] && continue          # gabarits <...>
    rest=${line#*namfoop }                        # retire ./target/release/, VAR=...
    rest=${rest%%  #*}                            # commentaire en fin de ligne
    rest=${rest%% #*}
    n=$((n + 1))
    local -a args=()
    if ! eval "args=($rest)" 2>/dev/null; then err "$file : exemple illisible : $line"; continue; fi
    local out
    if ! out=$(NAMFOOP_CHECK_ARGS=1 RUST_LOG=off "$BIN" "${args[@]}" 2>&1); then
      err "$file : exemple invalide : namfoop $rest"
      echo "$out" | grep -vE '^\s*$' | head -4 | sed 's/^/      /'
    fi
  done < "$file"
  echo "  $file : $n exemples vérifiés"
}
check_examples README.md
check_examples "$GUIDE"

# 5. pas de sons versionnés
if git rev-parse --git-dir >/dev/null 2>&1; then
  tracked=$(git ls-files | grep -iE '\.(nam|wav|zip|aif|aiff|flac)$' || true)
  [[ -z $tracked ]] || err "fichiers audio/modèles versionnés (droits !) : $tracked"
fi

# 6. changelog
if git rev-parse --verify -q HEAD >/dev/null; then
  changed=$(git diff --name-only HEAD -- crates/ 2>/dev/null | grep -v '/tests/' || true)
  if [[ -n $changed ]] && ! git diff --name-only HEAD -- CHANGELOG.md | grep -q .; then
    echo "  ⚠ du code a changé sans entrée dans CHANGELOG.md (section « Non publié »)"
  fi
fi

# 7. un CLAUDE.md par crate, citée dans la carte racine et dans docs/architecture.md
for dir in crates/*/; do
  c=$(basename "$dir")
  [[ -f $dir/CLAUDE.md ]] || err "crates/$c/CLAUDE.md absent (rubriques : responsabilité, API, interdits, règles, faits mesurés, tests)"
  grep -q "crates/$c/" CLAUDE.md || err "crate « $c » absente de la carte du CLAUDE.md racine"
  grep -q "\`$c\`" docs/architecture.md || err "crate « $c » absente de docs/architecture.md"
done

# 8. taille des fichiers : au-delà de ~600 lignes, un agent lit en morceaux et rate des invariants
while IFS= read -r -d '' f; do
  n=$(wc -l < "$f")
  if (( n > 1000 )); then
    err "$f : $n lignes (> 1000) : découper par responsabilité"
  elif (( n > 600 )); then
    echo "  ⚠ $f : $n lignes (> 600) : penser à découper"
  fi
done < <(find crates -name '*.rs' -not -path '*/tests/*' -not -path '*/vendor/*' -not -path '*/target/*' -print0)

if (( fail )); then echo "docs : ÉCHEC"; exit 1; fi
echo "docs : OK"
