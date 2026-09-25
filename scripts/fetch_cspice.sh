#!/usr/bin/env bash
# Download the CSPICE source tree for this platform into spice/cspice.
# spice/ is gitignored; run this once after cloning (CI does it every run).
#
# NAIF ships one distribution per platform (see naif/toolkit_C.html); this
# picks the one matching the host. The C sources are identical across
# distributions, but the matching archive also carries prebuilt libs for
# the host. Override the choice with CSPICE_DISTDIR, e.g.
#   CSPICE_DISTDIR=MacIntel_OSX_AppleC_64bit ./scripts/fetch_cspice.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/spice"
mkdir -p "$OUT"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS:$ARCH" in
  Linux:x86_64)                     DIST=PC_Linux_GCC_64bit ;;
  Darwin:x86_64)                    DIST=MacIntel_OSX_AppleC_64bit ;;
  Darwin:arm64|Darwin:aarch64)      DIST=MacM1_OSX_clang_64bit ;;
  MINGW*:x86_64|MSYS*:x86_64|CYGWIN*:x86_64) DIST=PC_Windows_VisualC_64bit ;;
  *)
    echo "unsupported platform: $OS $ARCH" >&2
    echo "set CSPICE_DISTDIR to a directory under https://naif.jpl.nasa.gov/pub/naif/toolkit/C/" >&2
    exit 1
    ;;
esac
DIST="${CSPICE_DISTDIR:-$DIST}"

case "$DIST" in
  PC_Windows_*) ARCHIVE=cspice.zip ;;
  *)            ARCHIVE=cspice.tar.Z ;;
esac

if [[ -d "$OUT/cspice/src/cspice" ]]; then
  echo "already present: $OUT/cspice"
  exit 0
fi

URL="https://naif.jpl.nasa.gov/pub/naif/toolkit/C/$DIST/packages/$ARCHIVE"
echo "downloading $DIST/$ARCHIVE ..."
curl -sS --fail -o "$OUT/$ARCHIVE" "$URL"

case "$ARCHIVE" in
  cspice.zip)
    if command -v unzip >/dev/null 2>&1; then
      unzip -q "$OUT/$ARCHIVE" -d "$OUT"
    else
      powershell -NoLogo -Command "Expand-Archive -LiteralPath '$(cygpath -w "$OUT/$ARCHIVE")' -DestinationPath '$(cygpath -w "$OUT")'"
    fi
    ;;
  *)
    gzip -dc "$OUT/$ARCHIVE" | tar -x -C "$OUT"
    ;;
esac

echo "cspice source ready in $OUT/cspice"
