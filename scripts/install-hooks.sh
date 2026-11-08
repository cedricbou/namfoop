#!/usr/bin/env bash
# Installe un hook git pre-commit qui lance `scripts/check.sh --quick` (~30 s).
# Contourner exceptionnellement : git commit --no-verify
set -euo pipefail
cd "$(dirname "$0")/.."
hook=$(git rev-parse --git-path hooks)/pre-commit
cat > "$hook" <<'HOOK'
#!/usr/bin/env bash
exec scripts/check.sh --quick
HOOK
chmod +x "$hook"
echo "hook installé : $hook"
