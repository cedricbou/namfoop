#!/usr/bin/env bash
# Test HORS-LIGNE de scripts/feature.sh : rejoue les scénarios dangereux du cycle
# (branche réutilisée, PR déjà fusionnée, fusion qui n'a pas eu lieu) et le contrôle du style des
# issues (docs/style-issues.md) sur un dépôt jetable, avec un faux `gh`. Aucun accès réseau, aucune issue ni PR réelle.
#
#   scripts/test-feature.sh          (appelé par scripts/check.sh)
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=${NAMFOOP_FEATURE_SH:-$PWD/scripts/feature.sh}   # pointer l'ancienne version prouve que le test voit le bug
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
export GIT_AUTHOR_NAME=Test GIT_AUTHOR_EMAIL=test@example.invalid
export GIT_COMMITTER_NAME=Test GIT_COMMITTER_EMAIL=test@example.invalid
export GH_STATE=$TMP/state GH_ORIGIN=$TMP/origin
export PATH="$TMP/bin:$PATH"
ko=0

# ---------------------------------------------------------------- faux gh
mkdir -p "$TMP/bin" "$TMP/state"
cat > "$TMP/bin/gh" <<'GH'
#!/usr/bin/env bash
# Faux `gh` : juste ce que scripts/feature.sh utilise. Les filtres --jq sont reconnus un par un, et
# un filtre INCONNU fait échouer le test — sinon une modification de feature.sh passerait inaperçue.
set -euo pipefail
echo "$*" >> "$GH_STATE/calls"
die() { echo "faux gh : $*" >&2; exit 64; }
opt() { local want=$1; shift; while (($#)); do [[ $1 == "--$want" ]] && { echo "${2:-}"; return; }; shift; done; }
PRS=$GH_STATE/prs            # une ligne par PR : numéro|head|état|titre|corps|mergeCommit
row_of() { grep -m1 "^$1|" "$PRS" || true; }
field() { cut -d'|' -f"$2" <<<"$1"; }

case "$1 ${2:-}" in
  "issue view")
    case "$(opt jq "$@")" in
      .state) echo OPEN ;;
      .title) cat "$GH_STATE/title" ;;
      *join*) cat "$GH_STATE/labels" ;;
      .body) cat "$GH_STATE/body" ;;
      *) die "filtre d'issue inconnu : $(opt jq "$@")" ;;
    esac ;;

  "issue create")
    cp "$(opt body-file "$@")" "$GH_STATE/body"
    opt title "$@" > "$GH_STATE/title"; opt label "$@" > "$GH_STATE/labels"
    echo "https://example.invalid/issues/129" ;;

  "issue list")
    echo "  #203  Changer d'instrument sans refaire la calibration (label $(opt label "$@"))" ;;

  "issue comment")
    opt body "$@" > "$GH_STATE/comment" ;;

  "pr list")
    want=$(opt state "$@"); head=$(opt head "$@"); jq=$(opt jq "$@")
    row=""
    while IFS= read -r l; do
      [[ $(field "$l" 2) == "$head" ]] || continue
      case "$want" in
        all) : ;;
        open) [[ $(field "$l" 3) == OPEN ]] || continue ;;
        merged) [[ $(field "$l" 3) == MERGED ]] || continue ;;
        *) die "état inconnu : $want" ;;
      esac
      row=$l; break
    done < "$PRS"
    case "$jq" in
      '.[0].number // empty') [[ -z $row ]] || field "$row" 1 ;;
      *'select(.state != "OPEN")'*)
        [[ -n $row && $(field "$row" 3) != OPEN ]] &&
          echo "#$(field "$row" 1) ($(tr 'A-Z' 'a-z' <<<"$(field "$row" 3)"))" || true ;;
      *) die "filtre de liste inconnu : $jq" ;;
    esac ;;

  "pr view")
    if [[ ${3:-} == --* || -z ${3:-} ]]; then
      # `gh pr view` sans numéro : GitHub rend LA PR de la branche courante, fusionnée ou non —
      # c'est exactement ce qui trompait scripts/feature.sh avant #128.
      head=$(git branch --show-current)
      row=$(awk -F'|' -v h="$head" '$2==h && $3=="OPEN"' "$PRS" | tail -1)
      [[ -n $row ]] || row=$(awk -F'|' -v h="$head" '$2==h' "$PRS" | tail -1)
    else
      row=$(row_of "$3")
    fi
    [[ -n $row ]] || die "aucune PR pour « $3 »"
    case "$(opt jq "$@")" in
      .number) field "$row" 1 ;;
      .url) echo "https://example.invalid/pull/$(field "$row" 1)" ;;
      .title) field "$row" 4 ;;
      .body) field "$row" 5 ;;
      .state) field "$row" 3 ;;
      '.mergeCommit.oid // empty') field "$row" 6 ;;
      *) die "filtre de PR inconnu : $(opt jq "$@")" ;;
    esac ;;

  "pr create")
    n=$(( $(wc -l < "$PRS") + 200 ))
    echo "$n|$(opt head "$@")|OPEN|$(opt title "$@")|$(tr '\n' ' ' <<<"$(opt body "$@")")|" >> "$PRS"
    echo "https://example.invalid/pull/$n" ;;

  "pr checks")
    [[ "$*" == *--watch* ]] && exit 0
    echo 1 ;;

  "pr merge")
    row=$(row_of "$3"); head=$(field "$row" 2); oid=""
    case "$(cat "$GH_STATE/merge_mode")" in
      ok)   # vraie fusion en squash dans origin, comme GitHub
        w=$GH_STATE/merge-work
        [[ -d $w ]] || git clone -q "$GH_ORIGIN" "$w"
        git -C "$w" fetch -q origin '+refs/heads/*:refs/remotes/origin/*'
        git -C "$w" checkout -q main && git -C "$w" reset -q --hard origin/main
        git -C "$w" merge --squash -q "origin/$head"
        git -C "$w" commit -qm "$(opt subject "$@")"
        git -C "$w" push -q origin main
        oid=$(git -C "$w" rev-parse HEAD)
        git -C "$w" push -q origin --delete "$head" ;;
      lie)  # GitHub dit « fusionnée » mais rien n'arrive sur main (le faux succès de #128)
        oid=$(git -C "$GH_ORIGIN" rev-parse "refs/heads/$head") ;;
      noop) echo "Déjà à jour."; exit 0 ;;      # ne fusionne rien et ne change pas l'état
      *) die "mode de fusion inconnu" ;;
    esac
    awk -F'|' -v OFS='|' -v n="$3" -v o="$oid" '$1==n {$3="MERGED"; $6=o} {print}' "$PRS" > "$PRS.new"
    mv "$PRS.new" "$PRS"
    echo "PR #$3 fusionnée (faux gh)" ;;

  *) die "commande non prévue : $*" ;;
esac
GH
chmod +x "$TMP/bin/gh"

# ---------------------------------------------------------------- dépôt jetable
setup() {
  rm -rf "$TMP/origin" "$TMP/work" "$TMP/state"; mkdir -p "$TMP/state"
  git init -q --bare -b main "$TMP/origin"
  git init -q -b main "$TMP/work"
  cd "$TMP/work"
  mkdir -p scripts
  install -m 755 "$SRC" scripts/feature.sh
  printf '#!/usr/bin/env bash\necho "(faux check.sh $*)"\n' > scripts/check.sh
  chmod +x scripts/check.sh
  echo depart > fichier.txt
  git add -A && git commit -qm "départ"
  git remote add origin "$TMP/origin" && git push -q -u origin main
  : > "$TMP/state/prs"; : > "$TMP/state/calls"
  echo ok > "$TMP/state/merge_mode"
  echo "Titre de l'issue" > "$TMP/state/title"
  echo story > "$TMP/state/labels"
  story_body > "$TMP/state/body"
}
# Un corps de story conforme à docs/style-issues.md.
story_body() { cat <<'EOT'
**En tant que** bassiste qui règle un son, **je veux** entendre un réglage tout de suite, **afin de**
régler à l'oreille.

## Aujourd'hui
Chaque appui coupe le son.

## Terminé quand
- [ ] Le son suit sans coupure
EOT
}
commit() { echo "$1" >> fichier.txt; git add -A; git commit -qm "$1"; }
pr_line() { echo "$1|$2|$3|titre|Closes #128|$4" >> "$TMP/state/prs"; }

fail() { echo "  ✗ $*"; ko=$((ko + 1)); }
has()  { [[ $1 == *"$2"* ]] && echo "  ✓ $3" || fail "$3 — « $2 » absent de : $1"; }
run_ok()   { local w=$1 t=$2; shift 2; local o
  if o=$("$@" 2>&1); then has "$o" "$w" "$t"; else fail "$t — a échoué : $o"; fi; }
run_fail() { local w=$1 t=$2; shift 2; local o
  if o=$("$@" 2>&1); then fail "$t — aurait dû échouer : $o"; else has "$o" "$w" "$t"; fi; }

# 1. start ne réutilise pas un nom de branche déjà pris (local, puis distant)
echo "== start : nom de branche libre"
setup
run_ok "branche : feat/128-titre-de-l-issue " "première branche" scripts/feature.sh start 128
git checkout -q main
run_ok "feat/128-titre-de-l-issue-2" "branche locale déjà prise -> suffixe" scripts/feature.sh start 128
git push -q -u origin HEAD
git checkout -q main && git branch -qD feat/128-titre-de-l-issue-2
run_ok "feat/128-titre-de-l-issue-3" "branche distante déjà prise -> suffixe" scripts/feature.sh start 128

# 2. pr ne reprend jamais une PR déjà fusionnée (le scénario qui a fait croire à un commit livré)
echo "== pr : une PR fusionnée n'est pas reprise"
setup
scripts/feature.sh start 128 >/dev/null
b=$(git branch --show-current)
commit "travail"
pr_line 126 "$b" MERGED deadbeef
run_ok "déjà fusionnée" "avertissement sur la PR fusionnée" scripts/feature.sh pr --quick
has "$(cat "$TMP/state/calls")" "pr create" "une nouvelle PR est créée"
run_ok "PR existante mise à jour" "la PR OUVERTE, elle, est reprise" scripts/feature.sh pr --quick

# 3. merge refuse quand la seule PR de la branche est déjà fusionnée
echo "== merge : refus quand il n'y a rien à fusionner"
setup
scripts/feature.sh start 128 >/dev/null
b=$(git branch --show-current)
commit "travail"
git push -q -u origin "$b"
pr_line 126 "$b" MERGED deadbeef
run_fail "il n'y a rien à fusionner" "refus explicite, pas un faux succès" scripts/feature.sh merge

# 4. merge n'annonce jamais un succès sans fusion réelle
echo "== merge : aucun faux succès"
setup
scripts/feature.sh start 128 >/dev/null
b=$(git branch --show-current)
commit "travail"
scripts/feature.sh pr --quick >/dev/null
echo noop > "$TMP/state/merge_mode"
run_fail "RIEN n'a été livré" "la PR reste ouverte -> échec" scripts/feature.sh merge
echo lie > "$TMP/state/merge_mode"
run_fail "RIEN n'a été livré" "GitHub dit « fusionnée » mais main n'a rien -> échec" scripts/feature.sh merge
[[ -z $(git -C "$TMP/origin" log --oneline main --grep=travail) ]] ||
  fail "main a changé alors que la fusion était simulée"

# 5. le chemin nominal fusionne vraiment
echo "== merge : fusion réelle"
setup
scripts/feature.sh start 128 >/dev/null
commit "travail"
scripts/feature.sh pr --quick >/dev/null
run_ok "fusionnée, main à jour" "succès annoncé" scripts/feature.sh merge
has "$(git -C "$TMP/origin" show main:fichier.txt)" "travail" "le travail est bien sur main"
has "$(git branch --show-current)" "main" "on revient sur main"

# 6. le style des issues : start refuse une issue technique, issue refuse de la créer
echo "== issues : le style est contrôlé"
setup
printf '## Besoin\n`dsp::gain::LiveGain` rebâtit la chaîne.\n\n## Crates touchées\n`dsp`\n' > "$TMP/state/body"
run_fail "ne suit pas docs/style-issues.md" "start refuse une issue technique" scripts/feature.sh start 128
has "$(git branch --show-current)" "main" "aucune branche n'est ouverte"
run_ok "branche : feat/128-" "NAMFOOP_ALLOW_ISSUE=1 passe outre" env NAMFOOP_ALLOW_ISSUE=1 scripts/feature.sh start 128
git checkout -q main
echo enhancement > "$TMP/state/labels"; story_body > "$TMP/state/body"
run_fail "une étiquette parmi story, epic, chore, bug" "start refuse une issue sans type" scripts/feature.sh start 128
echo epic > "$TMP/state/labels"
run_fail "est une épique" "start refuse une épique" scripts/feature.sh start 128
run_fail "issue <type>" "start \"Titre\" renvoie à la commande issue" scripts/feature.sh start "Un titre"
story_body > "$TMP/corps.md"; echo "Crates : dsp, namfoop." > "$TMP/cadrage.md"
run_fail "titre préfixé" "issue refuse un titre préfixé d'une crate" \
  scripts/feature.sh issue story "dsp : chorus.rate synchronisable" "$TMP/corps.md"
run_fail "rubrique absente" "issue refuse un corps sans les rubriques du type" \
  scripts/feature.sh issue chore "Une CI plus sûre" "$TMP/corps.md"
: > "$TMP/state/calls"
run_ok "issue créée : #129 (cadrage technique en commentaire)" "issue crée la story et poste le cadrage" \
  scripts/feature.sh issue story "Entendre un réglage au moment où je le tourne" "$TMP/corps.md" "$TMP/cadrage.md"
has "$(cat "$TMP/state/labels")" "story" "l'étiquette est le type"
has "$(cat "$TMP/state/comment")" "## Cadrage technique" "le cadrage est un commentaire titré"
run_ok "issue #129 conforme" "lint accepte l'issue créée" scripts/feature.sh lint 129

# 7. une issue à cadrer ne se code pas
echo "== issues : à cadrer"
setup
echo story,needs-refinement > "$TMP/state/labels"
run_fail "rubrique « ## À trancher »" "lint refuse une issue à cadrer qui ne dit pas quoi" scripts/feature.sh lint 128
{ story_body; printf '\n## À trancher\n- Que contient un profil ?\n'; } > "$TMP/state/body"
run_ok "issue #128 conforme" "lint l'accepte avec sa rubrique" scripts/feature.sh lint 128
run_fail "est à cadrer" "start refuse une issue à cadrer" scripts/feature.sh start 128
run_fail "est à cadrer" "NAMFOOP_ALLOW_ISSUE=1 n'y change rien" env NAMFOOP_ALLOW_ISSUE=1 scripts/feature.sh start 128
has "$(git branch --show-current)" "main" "aucune branche n'est ouverte"
story_body > "$TMP/corps.md"
run_fail "rubrique « ## À trancher »" "issue --refine exige la rubrique" \
  scripts/feature.sh issue --refine story "Changer d'instrument sans recalibrer" "$TMP/corps.md"
cp "$TMP/state/body" "$TMP/corps.md"
run_ok "issue créée : #129, à cadrer" "issue --refine crée l'issue à cadrer" \
  scripts/feature.sh issue --refine story "Changer d'instrument sans recalibrer" "$TMP/corps.md"
has "$(cat "$TMP/state/labels")" "story,needs-refinement" "elle porte les deux étiquettes"
run_ok "label needs-refinement" "refine liste les issues à cadrer" scripts/feature.sh refine

cd "$OLDPWD" 2>/dev/null || true
if (( ko )); then echo "✗ $ko vérification(s) en échec" >&2; exit 1; fi
echo "OK (cycle feature.sh)"
