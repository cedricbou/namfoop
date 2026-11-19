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
#  8. taille des fichiers .rs (hors vendor/), tests compris : avertissement > 600, échec > 1000
#  9. aucun numéro d'issue ou de PR dans un commentaire ou un rustdoc (règle 2 de style-doc),
#     et le compte à part des mêmes numéros dans une chaîne littérale (certains s'affichent)
# 10. aucun mot-signal temporel dans les commentaires, docs/ et CLAUDE.md (règle 1)
# 11. tout chemin de fichier cité dans un CLAUDE.md ou docs/ existe à HEAD (« referential rot »)
# 12. taille des fichiers de documentation : avertissement > 600
#
# Les contrôles 9 à 11 sont **provisoirement des avertissements** : le dépôt ne les passe pas
# encore. Ils deviennent bloquants quand leurs compteurs atteignent zéro (issue de durcissement
# #291) ; le compte affiché est ce qui mesure l'avancement, et ce paragraphe disparaît avec eux.
# Référence des règles : docs/style-doc.md.
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
PROFILE=${NAMFOOP_PROFILE:-release}
BIN=./target/$PROFILE/namfoop
GUIDE=${GUIDE:-docs/guide.md}
fail=0
err() { echo "  ✗ $*"; fail=1; }
warn() { echo "  ⚠ $*"; }

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

# 8. taille des fichiers : au-delà de ~600 lignes, un agent lit en morceaux et rate des invariants.
# Les fichiers de test comptent aussi : la règle porte sur ce qu'on arrive à lire d'un bloc, pas sur
# la nature du code.
while IFS= read -r -d '' f; do
  n=$(wc -l < "$f")
  if (( n > 1000 )); then
    err "$f : $n lignes (> 1000) : découper par responsabilité"
  elif (( n > 600 )); then
    echo "  ⚠ $f : $n lignes (> 600) : penser à découper"
  fi
done < <(find crates -name '*.rs' -not -path '*/vendor/*' -not -path '*/target/*' -print0)


# --- Les règles de docs/style-doc.md qui se mécanisent -------------------------------------------

# Les fichiers de référence : un CLAUDE.md par crate, le racine, docs/ (hors cli.md, engendré).
docs_rediges() {
  find docs -name '*.md' -not -name 'cli.md'
  find . -name 'CLAUDE.md' -not -path './target/*' -not -path './vendor/*' | sed 's|^\./||'
}
sources_rs() { find crates -name '*.rs' -not -path '*/vendor/*' -not -path '*/target/*'; }

# L'une ou l'autre moitié de chaque ligne de Rust, en « fichier:ligne:texte » : la part commentaire
# (part = 1) ou la part code (part = 2). Le « // » d'une URL (précédé de « : ») n'ouvre pas un
# commentaire ; une chaîne littérale qui en contiendrait un serait un faux positif, le dépôt n'en a
# pas.
moities_rs() {
  sources_rs | tr '\n' '\0' | xargs -0 awk -v part="$1" '
    { coupe = 0
      for (i = 1; i < length($0); i++)
        if (substr($0, i, 2) == "//" && substr($0, i - 1, 1) != ":") { coupe = i; break }
      texte = (part == 1) ? (coupe ? substr($0, coupe) : "") : (coupe ? substr($0, 1, coupe - 1) : $0)
      if (texte != "") printf "%s:%d:%s\n", FILENAME, FNR, texte
    }'
}
commentaires_rs() { moities_rs 1; }
code_rs() { moities_rs 2; }

# 9. pas de numéro d'issue ni de PR dans les commentaires (règle 2 : l'historique est dans git et
# le CHANGELOG). Les renvois « ADR 000x » ne portent pas de « # » : ils passent par construction.
tags=$(commentaires_rs | grep -E '#[0-9]+' || true)
if [[ -n $tags ]]; then
  n=$(echo "$tags" | grep -oE '#[0-9]+' | wc -l)
  warn "$(echo "$tags" | wc -l) commentaires portent un numéro d'issue ($n occurrences) : à retirer"
  echo "$tags" | cut -d: -f1 | sort | uniq -c | sort -rn | head -5 | sed 's/^/      /'
fi
# Les mêmes numéros dans une **chaîne littérale** : comptés à part, parce qu'une partie d'entre eux
# est montrée à l'utilisateur (le champ `what` de la table des touches va dans la fenêtre d'aide,
# un message d'erreur de `FxSpec` va au terminal, la ligne de tempo va à l'écran Set).
chaines=$(code_rs | grep -E '#[0-9]+' || true)
if [[ -n $chaines ]]; then
  warn "$(echo "$chaines" | wc -l) chaînes de caractères portent un numéro d'issue (dont des textes montrés à l'utilisateur) :"
  echo "$chaines" | sed 's/^/      /' | head -10
fi

# 10. mots-signaux temporels (règle 1). La liste est le sous-ensemble **mécanisable** de la règle :
# « l'ancien X » et « ne … plus » sont du vocabulaire du domaine ici (l'ancienne chaîne d'un fondu,
# « un son qui a disparu de la bibliothèque »), donc hors de ce contrôle et laissés à la relecture.
# Exemptés : CHANGELOG.md (de l'historique par nature), docs/adr/ (un ADR dit ce qui était le cas
# pour justifier la décision, et ne se réécrit pas : il se supersède), et docs/style-doc.md, qui
# doit nommer les mots qu'il proscrit.
TEMPOREL="désormais|auparavant|anciennement|c'était|ont disparu|(a|ont) été (retiré|remplacé|écarté|supprimé|déplacé|abandonné|introduit)(e?s?)|(avant|depuis) (la PR )?#[0-9]|[Cc]orrigé le [0-9]{2}/[0-9]{2}"
recit=$( { commentaires_rs
           docs_rediges | grep -vE '^(docs/adr/|docs/style-doc\.md$)' \
             | tr '\n' '\0' | xargs -0 grep -nH ''
         } | grep -iE "$TEMPOREL" || true)
if [[ -n $recit ]]; then
  warn "$(echo "$recit" | wc -l) lignes racontent le passé au lieu de décrire l'état actuel"
  echo "$recit" | cut -d: -f1 | sort | uniq -c | sort -rn | head -5 | sed 's/^/      /'
fi

# 11. référence périmée : un chemin cité entre accents graves qui n'existe plus. Les citations sont
# souvent relatives à la crate (`swap.rs` dans crates/dsp/CLAUDE.md), donc un nom est accepté dès
# qu'un fichier suivi se termine par lui. Seuls les noms portant une extension connue sont
# candidats : les fractions (`1/4`), les répertoires et les conventions de suffixe (`.set.toml`)
# n'en sont pas. Les motifs à « * » sont des familles, pas des chemins.
suivis=$(git ls-files 2>/dev/null || true)
perimees=""
vues=0
while IFS='|' read -r f ref; do
  vues=$((vues + 1))
  [[ -e $ref ]] && continue
  grep -qxF -- "$ref" <<<"$suivis" && continue
  grep -qF -- "/$ref" <<<"$suivis" && continue
  perimees+="$f : \`$ref\`"$'\n'
done < <(
  for f in $(docs_rediges); do
    grep -oE '`[^`]*`' "$f" | tr -d '`' \
      | grep -E '^[A-Za-z0-9_-][A-Za-z0-9_./-]*\.(rs|md|toml|sh|py|yml|txt)$' \
      | while IFS= read -r ref; do printf '%s|%s\n' "$f" "$ref"; done
  done | sort -u
)
echo "  $vues chemins cités vérifiés dans les CLAUDE.md et docs/"
if [[ -n ${perimees%$'\n'} ]]; then
  warn "$(echo "${perimees%$'\n'}" | wc -l) références périmées (chemin cité, fichier absent) :"
  echo "${perimees%$'\n'}" | head -10 | sed 's/^/      /'
fi

# 12. taille des documents : même raison que l'étape 8, et même seuil.
while IFS= read -r f; do
  n=$(wc -l < "$f")
  if (( n > 600 )); then warn "$f : $n lignes (> 600) : découper par famille de sujets"; fi
done < <(docs_rediges)

if (( fail )); then echo "docs : ÉCHEC"; exit 1; fi
echo "docs : OK"
