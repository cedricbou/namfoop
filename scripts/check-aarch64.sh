#!/usr/bin/env bash
# Compile la cible du Raspberry Pi 5 (aarch64) sans l'exécuter. `crates/audio-io/src/rt.rs` et
# `crates/engine/src/rt.rs` portent chacun un bloc d'assembleur aarch64 (flush-to-zero du
# processeur, dans le chemin temps réel) qu'aucune compilation x86 ne regarde.
#
# `cargo build` et NON `cargo check` : le contenu d'un `asm!` n'est donné à l'assembleur qu'à la
# génération de code, si bien qu'un `check` passe sans broncher sur une instruction inexistante.
#
# Rien à installer côté système : le C++ NAM n'est pas compilé (NAMFOOP_SKIP_CPP ; une bibliothèque
# ne se lie pas, donc les symboles manquants ne gênent pas) et pkg-config rend les drapeaux de
# l'ALSA de l'hôte (PKG_CONFIG_ALLOW_CROSS), qui ne servent eux aussi qu'à lier. Seule la cible
# Rust est nécessaire : `rustup target add aarch64-unknown-linux-gnu`.
#
#   scripts/check-aarch64.sh              cible absente : sauté avec un avertissement (local)
#   scripts/check-aarch64.sh --required   cible absente : échec (CI)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

TARGET=aarch64-unknown-linux-gnu
REQUIRED=0
[[ ${1:-} == --required ]] && REQUIRED=1

if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
  if (( REQUIRED )); then
    echo "✗ cible $TARGET absente alors qu'elle est exigée : rustup target add $TARGET" >&2
    exit 1
  fi
  echo "  ⚠ cible $TARGET absente, compilation aarch64 sautée (rustup target add $TARGET)"
  exit 0
fi

NAMFOOP_SKIP_CPP=1 PKG_CONFIG_ALLOW_CROSS=1 \
  cargo build -q --target "$TARGET" -p audio-io -p engine
echo "aarch64 : audio-io et engine compilés (les deux blocs d'assembleur flush-to-zero compris)"
