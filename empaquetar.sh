#!/usr/bin/env bash
# empaquetar.sh — compila Fuentes y deja el RPM en ../Build/bookos-fonts/,
# donde lo recoge instalar-todo.sh con el resto de apps.
set -euo pipefail
RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SALIDA="$RAIZ/../Build/bookos-fonts"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cargo build --release --locked --manifest-path "$RAIZ/Cargo.toml"
rpmbuild -bb "$RAIZ/bookos-fonts.spec" \
  --define "_topdir $TMP" --define "raiz $RAIZ" --quiet
mkdir -p "$SALIDA"
rm -f "$SALIDA"/*.rpm
cp "$TMP"/RPMS/*/*.rpm "$SALIDA/"
ls "$SALIDA"/*.rpm
