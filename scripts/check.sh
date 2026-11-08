#!/usr/bin/env bash
# Vérification complète SANS carte son : à faire passer avant chaque commit.
#   scripts/check.sh          tout (format, lint, tests, build release, docs, rendu de la bibliothèque)
#   scripts/check.sh --quick  sans le rendu de la bibliothèque
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
QUICK=${1:-}
step() { printf '\n== %s\n' "$*"; }

step "submodules"
git submodule update --init --recursive -q

step "format"
cargo fmt --all --check

step "clippy (warnings = erreurs)"
cargo clippy -q --all-targets -- -D warnings

step "tests (debug : active assert_no_alloc)"
if ! out=$(cargo test -q 2>&1); then echo "$out"; exit 1; fi
grep -E '^test result' <<<"$out" | grep -v ' 0 passed' || true

step "build release"
cargo build -q --release
BIN=./target/release/namfoop

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
