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
#
# Garde-fous (#128) : `start` ne réutilise jamais un nom de branche déjà pris (local ou origin),
# `pr` ne reprend qu'une PR OUVERTE, et `merge` refuse une PR déjà fusionnée puis vérifie que le
# commit de fusion est bien sur origin/main avant d'annoncer un succès.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

die() { echo "✗ $*" >&2; exit 1; }
need_clean() { [[ -z $(git status --porcelain) ]] || die "arbre de travail non propre : commiter ou ranger d'abord"; }
branch() { git branch --show-current; }
issue_of_branch() { [[ $(branch) =~ ^(feat|fix|docs)/([0-9]+)- ]] && echo "${BASH_REMATCH[2]}"; }
branch_exists() {
  git show-ref -q --verify "refs/heads/$1" && return 0
  [[ -n $(git ls-remote --heads origin "$1" 2>/dev/null) ]]
}
# Numéro de la PR portant cette branche head, dans l'état demandé (open | merged | all) ; vide s'il
# n'y en a pas. `gh pr view` sans argument prendrait aussi une PR fusionnée pour la nôtre (#128).
pr_of_branch() {
  gh pr list --state "$1" --head "$(branch)" --limit 1 --json number --jq '.[0].number // empty'
}

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
  local base="$prefix/$n-$(slug "$title")" b k=2
  b=$base
  # Un nom de branche réutilisé reprendrait la PR déjà fusionnée de la même head (#128) : on
  # cherche un nom libre, localement ET sur origin.
  while branch_exists "$b"; do
    b="$base-$k"; k=$((k + 1))
  done
  [[ $b == "$base" ]] || echo "« $base » existe déjà (locale ou distante) : branche « $b »"
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
  local open_pr merged_pr
  open_pr=$(pr_of_branch open)
  if [[ -n $open_pr ]]; then
    echo "PR existante mise à jour : $(gh pr view "$open_pr" --json url --jq .url)"
    return
  fi
  merged_pr=$(pr_of_branch merged)
  [[ -z $merged_pr ]] ||
    echo "⚠ la PR #$merged_pr, déjà fusionnée, porte la branche « $(branch) » : on en crée une nouvelle"
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

# Juste après un push, GitHub n'a pas encore enregistré les checks : `gh pr checks --watch`
# sortirait aussitôt (« no checks reported »). On attend qu'ils apparaissent (3 min max), puis
# quelques secondes de plus pour que tous les workflows soient inscrits.
wait_checks() {
  local pr=$1 n=0 waited=0 max=${NAMFOOP_CHECKS_TIMEOUT:-180}
  while :; do
    n=$(gh pr checks "$pr" --json name --jq length 2>/dev/null || echo 0)
    (( n > 0 )) && break
    (( waited >= max )) && die "aucun check enregistré pour la PR #$pr après ${max} s : voir l'onglet Actions"
    sleep 5; waited=$((waited + 5))
  done
  (( waited > 0 )) && sleep 10
  return 0
}

cmd_merge() {
  local n pr
  n=$(issue_of_branch) || die "pas sur une branche de fonctionnalité"
  pr=$(pr_of_branch open)
  if [[ -z $pr ]]; then
    local old
    old=$(gh pr list --state all --head "$(branch)" --limit 1 --json number,state \
      --jq '.[0] | select(.state != "OPEN") | "#\(.number) (\(.state | ascii_downcase))"')
    [[ -z $old ]] || die "la seule PR de « $(branch) » est $old : il n'y a rien à fusionner.
  Renommer la branche (git branch -m $(branch)-2), puis scripts/feature.sh pr"
    die "pas de PR ouverte pour $(branch) : scripts/feature.sh pr"
  fi
  need_clean
  [[ -z $(git log --oneline "origin/$(branch)..HEAD" 2>/dev/null) ]] || die "commits non poussés : scripts/feature.sh pr"
  echo "attente de la CI de la PR #$pr…"
  wait_checks "$pr"
  gh pr checks "$pr" --watch --interval 20 >/dev/null || { gh pr checks "$pr"; die "CI en échec : corriger avant de fusionner"; }
  local title link coauthors head touched
  head=$(git rev-parse HEAD)
  mapfile -t touched < <(git diff --name-only main...HEAD)
  title=$(gh pr view "$pr" --json title --jq .title)
  link=$(gh pr view "$pr" --json body --jq .body | grep -E '^(Closes|Refs) #' | head -1 || true)
  # co-auteurs repris des commits de la branche (rien d'imposé)
  coauthors=$(git log "main..HEAD" --format='%(trailers:key=Co-Authored-By)' | grep -v '^$' | sort -u || true)
  gh pr merge "$pr" --squash --delete-branch --subject "$title (#$pr)" --body "$link${coauthors:+

$coauthors}"
  # Ne JAMAIS annoncer un succès sans l'avoir vérifié (#128) : la PR doit être fusionnée et son
  # commit de fusion présent sur origin/main.
  git fetch -q --prune
  local state oid
  state=$(gh pr view "$pr" --json state --jq .state)
  oid=$(gh pr view "$pr" --json mergeCommit --jq '.mergeCommit.oid // empty')
  [[ $state == MERGED && -n $oid ]] ||
    die "la PR #$pr n'est pas fusionnée (état $state) : RIEN n'a été livré sur main"
  git merge-base --is-ancestor "$oid" origin/main ||
    die "le commit de fusion $oid de la PR #$pr n'est pas sur origin/main : RIEN n'a été livré"
  git checkout -q main
  git pull -q --ff-only
  # Dernier filet : chaque fichier touché par la branche doit être identique dans main (une autre
  # PR fusionnée entre-temps sur les mêmes fichiers expliquerait un écart : on avertit, c'est tout).
  if (( ${#touched[@]} )) && ! git diff --quiet "$head" main -- "${touched[@]}"; then
    echo "⚠ main diffère de la branche sur : $(git diff --name-only "$head" main -- "${touched[@]}" | tr '\n' ' ')"
  fi
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
  *) sed -n '2,20p' "$0"; exit 1 ;;
esac
