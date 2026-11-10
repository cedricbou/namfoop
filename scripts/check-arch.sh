#!/usr/bin/env bash
# Sens des dépendances entre crates (docs/architecture.md, #47). Appelé par scripts/check.sh.
# Lit `cargo metadata` et refuse toute arête interdite par la table ci-dessous.
#
# Table des règles, une par ligne :
#   <crate> : interdit <dép> <dép> ...   -> ces dépendances directes sont refusées
#   <crate> : seulement <dép> <dép> ...  -> toute autre dépendance directe est refusée
# Sont vérifiées les dépendances normales et de build ; les dev-dependencies (tests) sont libres.
# Chaque issue qui crée une crate ajoute ici ses règles.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

RULES='
nam-sys  : seulement thiserror cc
dsp      : interdit alsa jack clap ratatui libc namfoop audio-io engine
audio-io : interdit clap ratatui dsp namfoop toml nam-sys engine
engine   : interdit clap ratatui namfoop toml alsa jack
'

cargo metadata --format-version 1 --no-deps --offline 2>/dev/null >/dev/null \
  || cargo metadata --format-version 1 --no-deps >/dev/null # télécharge l'index si besoin
cargo metadata --format-version 1 --no-deps --offline 2>/dev/null | RULES=$RULES python3 -c '
import json, os, sys

meta = json.load(sys.stdin)
deps = {
    p["name"]: sorted({d["name"] for d in p["dependencies"] if d["kind"] in (None, "build")})
    for p in meta["packages"]
}
errors = []
for n, line in enumerate(os.environ["RULES"].strip().splitlines(), 1):
    crate, _, rule = (x.strip() for x in line.partition(":"))
    mode, *names = rule.split()
    if crate not in deps:
        errors.append(f"règle {n} : crate « {crate} » inconnue (renommée ? mettre la table à jour)")
        continue
    if mode == "interdit":
        bad = [d for d in deps[crate] if d in names]
    elif mode == "seulement":
        bad = [d for d in deps[crate] if d not in names]
    else:
        errors.append(f"règle {n} : mode « {mode} » inconnu (interdit | seulement)")
        continue
    for d in bad:
        errors.append(f"{crate} -> {d} : dépendance interdite ({line.strip()})")
if errors:
    print("\n".join("✗ " + e for e in errors), file=sys.stderr)
    print("Voir docs/architecture.md : les dépendances ne vont que vers le bas.", file=sys.stderr)
    sys.exit(1)
print(f"{len(deps)} crates, sens des dépendances respecté")
'
