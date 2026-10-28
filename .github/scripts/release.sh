#!/usr/bin/env bash
# Sube archivos al Release del tag actual (vX.Y.Z), creandolo si todavia no existe.
#
#   bash .github/scripts/release.sh 0.2.0 Ethereon-0.2.0-setup.exe latest.json
set -euo pipefail

V="$1"
shift
TAG="$GITHUB_REF_NAME"

cat > notas.md <<NOTAS
**Windows:** bajá \`Ethereon-$V-setup.exe\` y abrilo. Se instala solo para tu usuario, sin pedir permisos de administrador.

Si ya tenés Ethereon, no hace falta: se actualiza sola cuando no la estás usando (o con **Update** en la barra de arriba).
NOTAS

# --latest: las apps instaladas buscan latest.json en el ultimo Release.
if ! gh release view "$TAG" >/dev/null 2>&1; then
  gh release create "$TAG" --title "Ethereon $V" --notes-file notas.md --latest || gh release view "$TAG" >/dev/null
fi
gh release upload "$TAG" "$@" --clobber
