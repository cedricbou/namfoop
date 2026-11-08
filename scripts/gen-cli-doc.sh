#!/usr/bin/env bash
# Génère docs/cli.md depuis l'aide de la CLI (source unique : les textes clap du code).
# Usage : scripts/gen-cli-doc.sh [fichier de sortie, défaut docs/cli.md]
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
OUT=${1:-docs/cli.md}
PROFILE=${NAMFOOP_PROFILE:-release}
BIN=./target/$PROFILE/namfoop
cargo build -q --profile "$PROFILE"

subcommands() {
  "$BIN" --help | sed -n '/^Commands:/,/^$/p' | awk 'NR>1 && $1 != "help" && NF {print $1}'
}

{
  echo "# Référence des commandes namfoop"
  echo
  echo "> Généré par \`scripts/gen-cli-doc.sh\` depuis l'aide intégrée (\`namfoop <commande> --help\`)."
  echo "> Ne pas éditer à la main : modifier les textes d'aide dans le code puis régénérer."
  echo "> Pour les explications et les cas d'usage, voir le [guide utilisateur](guide.md)."
  echo
  echo "Les options globales (\`--backend\`, \`--device\`, \`--buffer\`...) se placent **avant** la"
  echo "commande : \`namfoop --backend alsa --buffer 48 run -m b7k\`."
  echo
  echo "## namfoop"
  echo
  echo '```text'
  "$BIN" --help
  echo '```'
  for c in $(subcommands); do
    echo
    echo "## namfoop $c"
    echo
    echo '```text'
    "$BIN" "$c" --help
    echo '```'
  done
} > "$OUT"
echo "écrit : $OUT"
