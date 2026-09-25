#!/usr/bin/env bash
# Download the SPICE kernels required by the ephemeris core into kernels/.
# Kernels are gitignored; run this once after cloning.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/kernels"
mkdir -p "$OUT"

BASE="https://naif.jpl.nasa.gov/pub/naif/generic_kernels"
FILES=(
  "spk/planets/de440s.bsp"     # planetary ephemeris, 1849-2150 (~32 MB)
  "lsk/naif0012.tls"           # leap seconds
  "pck/pck00011.tpc"           # rotation constants
  "pck/gm_de431.tpc"           # body GM values (orbital elements)
  # natural satellites (~2.3 GB total; optional but needed for moons)
  "spk/satellites/mar099s.bsp"       # Phobos, Deimos (1995-2050 only, ~64 MB)
  "spk/satellites/jup365.bsp"        # Galilean moons, 1600-2200 (~1.1 GB)
  "spk/satellites/sat441.bsp"        # Mimas..Iapetus, Phoebe, 1749-2250 (~630 MB)
  "spk/satellites/ura184_part-3.bsp" # Miranda..Oberon, 1600-2399 (~368 MB)
  "spk/satellites/nep097.bsp"        # Triton, 1600-2399 (~100 MB)
  "spk/satellites/plu060.bsp"        # Charon, Nix, Hydra, Kerberos, Styx, 1800-2199 (~128 MB)
)

for rel in "${FILES[@]}"; do
  name="$(basename "$rel")"
  dest="$OUT/$name"
  if [[ -s "$dest" ]]; then
    echo "already present: $dest"
    continue
  fi
  echo "downloading $name ..."
  curl -sS --fail -o "$dest" "$BASE/$rel"
done

echo "kernels ready in $OUT"
