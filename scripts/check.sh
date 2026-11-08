#!/usr/bin/env bash
# Vérification complète SANS carte son : à faire passer avant chaque commit.
#   scripts/check.sh              tout (format, lint, tests, build, docs, rendu de la bibliothèque)
#   scripts/check.sh --quick      sans le rendu de la bibliothèque
#   scripts/check.sh --lint-only  format + clippy seulement (job « lint » de la CI)
#   scripts/check.sh --no-lint    tout sauf format + clippy (job « check » de la CI)
# NAMFOOP_PROFILE=ci : tests et binaire compilés une seule fois dans le profil `ci` (rapide, sans
# LTO, assertions actives). Défaut : tests en debug + binaire `release` (celui qu'on joue).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
QUICK="" LINT=1 REST=1
for a in "$@"; do
  case $a in
    --quick) QUICK=--quick ;;
    --lint-only) REST=0 ;;
    --no-lint) LINT=0 ;;
    *) echo "option inconnue : $a" >&2; exit 2 ;;
  esac
done
PROFILE=${NAMFOOP_PROFILE:-release}
export NAMFOOP_PROFILE=$PROFILE
step() { printf '\n== %s\n' "$*"; }

if [[ -z ${CI:-} && ! -x $(git rev-parse --git-path hooks)/pre-push ]]; then
  echo "⚠ hooks git non installés : scripts/install-hooks.sh (cycle PR imposé)"
fi

step "submodules"
git submodule update --init --recursive -q

if (( LINT )); then
  step "format"
  cargo fmt --all --check

  step "clippy (warnings = erreurs)"
  # en CI le job lint est séparé : inutile d'y compiler le C++ (rien n'est lié en mode check)
  if [[ -n ${CI:-} ]]; then export NAMFOOP_SKIP_CPP=1; fi
  cargo clippy -q --all-targets -- -D warnings
  unset NAMFOOP_SKIP_CPP
fi
(( REST )) || { printf '\nOK (lint)\n'; exit 0; }

if [[ $PROFILE == release ]]; then
  step "tests (debug : active assert_no_alloc)"
  test_args=()
else
  step "tests (profil $PROFILE, assertions actives)"
  test_args=(--profile "$PROFILE")
fi
if ! out=$(cargo test -q "${test_args[@]}" 2>&1); then echo "$out"; exit 1; fi
grep -E '^test result' <<<"$out" | grep -v ' 0 passed' || true

step "build (profil $PROFILE)"
cargo build -q --profile "$PROFILE"
BIN=./target/$PROFILE/namfoop

step "documentation (aide CLI, docs/cli.md, guide, README, exemples, pas de sons versionnés)"
scripts/check-docs.sh

step "rendu de référence (signal de test NAM)"
$BIN render --no-config >/dev/null

if [[ "$QUICK" != "--quick" && -d library/nam ]]; then
  step "rendu de chaque modèle de la bibliothèque (NaN, silence, saturation)"
  fail=0; n=0
  while IFS= read -r -d '' m; do
    n=$((n+1))
    if ! out=$(RUST_LOG=error $BIN render --no-config -m "$m" 2>&1); then
      echo "ÉCHEC $m : $out"; fail=1; continue
    fi
    peak=$(grep -oP 'crête \K-?[0-9.]+|crête \K-inf' <<<"$out" || echo "?")
    rms=$(grep -oP 'RMS \K-?[0-9.]+|RMS \K-inf' <<<"$out" || echo "?")
    if [[ "$rms" == "-inf" ]] || awk "BEGIN{exit !($rms < -60)}"; then
      echo "SILENCE $m (RMS $rms)"; fail=1
    elif awk "BEGIN{exit !($peak > 6)}"; then
      echo "CHAUD   $m (crête $peak dBFS)"
    fi
  done < <(find library/nam -name '*.nam' -print0 | sort -z)
  echo "$n modèles rendus"
  [[ $fail -eq 0 ]] || exit 1
fi

printf '\nOK\n'
