#!/usr/bin/env bash
# Cycle de développement namfoop : issue -> branche -> PR -> CI verte -> merge -> main à jour.
# La protection de branche GitHub n'est pas disponible (dépôt privé, offre gratuite) : ce script
# et les hooks (scripts/install-hooks.sh) imposent le cycle.
#
#   scripts/feature.sh start <n°>                    reprend l'issue n° : branche feat/<n°>-<titre>
#   scripts/feature.sh start "Titre" [labels]        crée l'issue (labels séparés par des virgules)
#   scripts/feature.sh pr [--refs] [--quick]         check.sh, push, PR « Closes #n » (--refs : issue
#                                                    laissée ouverte, ex. tests matériels à faire)
#   scripts/feature.sh merge                         attend la CI, squash merge, nettoie, main à jour
#   scripts/feature.sh status                        branche, issue, PR, CI
#
# Préfixe de branche : feat (défaut), fix si le label « bug » est présent, docs si « documentation »
# est le seul label.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

die() { echo "✗ $*" >&2; exit 1; }
need_clean() { [[ -z $(git status --porcelain) ]] || die "arbre de travail non propre : commiter ou ranger d'abord"; }
branch() { git branch --show-current; }
issue_of_branch() { [[ $(branch) =~ ^(feat|fix|docs)/([0-9]+)- ]] && echo "${BASH_REMATCH[2]}"; }

slug() {
  iconv -f utf-8 -t ascii//TRANSLIT <<<"$1" 2>/dev/null | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/[^a-z0-9]+/-/g; s/^-+|-+$//g' | cut -c1-40 | sed -E 's/-+$//'
}

cmd_start() {
  [[ $# -ge 1 ]] || die "usage : start <n° d'issue> | start \"Titre\" [labels]"
  need_clean
  local n
  if [[ $1 =~ ^[0-9]+$ ]]; then
    n=$1
    gh issue view "$n" --json state --jq .state | grep -q OPEN || die "l'issue #$n n'est pas ouverte"
  else
    local labels=${2:-enhancement}
    n=$(gh issue create --title "$1" --label "$labels" --body "À préciser : besoin, attendu, tests." | grep -oE '[0-9]+$')
    echo "issue créée : #$n"
  fi
  local title labels prefix=feat
  title=$(gh issue view "$n" --json title --jq .title)
  labels=$(gh issue view "$n" --json labels --jq '[.labels[].name] | join(",")')
  [[ ,$labels, == *,bug,* ]] && prefix=fix
  [[ $labels == documentation ]] && prefix=docs
  git checkout -q main
  git pull -q --ff-only
  local b="$prefix/$n-$(slug "$title")"
  git checkout -q -b "$b"
  echo "branche : $b  (issue #$n : $title)"
}

cmd_pr() {
  local link=Closes quick=""
  for a in "$@"; do
    case $a in
      --refs) link=Refs ;;
      --quick) quick=--quick ;;
      *) die "option inconnue : $a" ;;
    esac
  done
  local n
  n=$(issue_of_branch) || die "branche « $(branch) » : attendu feat|fix|docs/<n° d'issue>-... (scripts/feature.sh start)"
  need_clean
  [[ -n $(git log --oneline main..HEAD) ]] || die "aucun commit sur la branche"
  scripts/check.sh $quick
  if git diff --name-only main...HEAD | grep -qE '^crates/.*/src/' && ! git diff --name-only main...HEAD | grep -qx CHANGELOG.md; then
    die "du code a changé sans entrée dans CHANGELOG.md (« Non publié »)"
  fi
  git push -q -u origin "$(branch)"
  if gh pr view --json number >/dev/null 2>&1; then
    echo "PR existante mise à jour : $(gh pr view --json url --jq .url)"
    return
  fi
  local title body
  title=$(gh issue view "$n" --json title --jq .title)
  body=$(cat <<EOF
$link #$n

## Changements
$(git log --reverse --format='- %s' main..HEAD)

## Vérifications
- [x] \`scripts/check.sh\` ${quick:+(--quick)}
- [ ] Tests matériels si l'audio est touché : \`scripts/test-backends.sh\` (JACK **et** ALSA)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)
  gh pr create --base main --head "$(branch)" --title "$title" --body "$body"
}

cmd_merge() {
  local n pr
  n=$(issue_of_branch) || die "pas sur une branche de fonctionnalité"
  pr=$(gh pr view --json number --jq .number) || die "pas de PR pour $(branch) : scripts/feature.sh pr"
  need_clean
  [[ -z $(git log --oneline "origin/$(branch)..HEAD" 2>/dev/null) ]] || die "commits non poussés : scripts/feature.sh pr"
  echo "attente de la CI de la PR #$pr…"
  gh pr checks "$pr" --watch --interval 20 >/dev/null || { gh pr checks "$pr"; die "CI en échec : corriger avant de fusionner"; }
  local title link coauthors
  title=$(gh pr view "$pr" --json title --jq .title)
  link=$(gh pr view "$pr" --json body --jq .body | grep -E '^(Closes|Refs) #' | head -1 || true)
  # co-auteurs repris des commits de la branche (rien d'imposé)
  coauthors=$(git log "main..HEAD" --format='%(trailers:key=Co-Authored-By)' | grep -v '^$' | sort -u || true)
  gh pr merge "$pr" --squash --delete-branch --subject "$title (#$pr)" --body "$link${coauthors:+

$coauthors}"
  git checkout -q main
  git pull -q --ff-only
  git fetch -q --prune
  echo "✓ PR #$pr fusionnée, main à jour : $(git log --oneline -1)"
}

cmd_status() {
  echo "branche : $(branch)"
  if n=$(issue_of_branch); then
    echo "issue   : #$n $(gh issue view "$n" --json title,state --jq '"\(.title) [\(.state)]"')"
    gh pr view --json number,url,state --jq '"PR      : #\(.number) \(.state) \(.url)"' 2>/dev/null || echo "PR      : aucune"
    gh pr checks 2>/dev/null | sed 's/^/          /' || true
  fi
}

case ${1:-} in
  start) shift; cmd_start "$@" ;;
  pr) shift; cmd_pr "$@" ;;
  merge) shift; cmd_merge "$@" ;;
  status) shift; cmd_status "$@" ;;
  *) sed -n '2,15p' "$0"; exit 1 ;;
esac
