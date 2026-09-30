# Planetary Transfer

Cross-platform application that computes and plots planet positions using
JPL SPICE ephemerides. A Python desktop prototype is kept as a reference
implementation and cross-validation oracle.

## Repository layout

```
├── app/                        # Cross-platform application (Tauri 2 + Rust + Three.js)
│   ├── crates/
│   │   ├── cspice-build/       # Builds CSPICE from source via the cc crate
│   │   └── ephemeris/          # Ephemeris core (SPICE bindings, catalog, orbit paths)
│   ├── src-tauri/              # Tauri shell: commands, config, capabilities, icons
│   └── ui/                     # TypeScript + Three.js frontend (Vite)
├── kernels/                    # SPICE kernels (gitignored, see below)
├── scripts/
│   ├── fetch_kernels.sh        # Downloads the required SPICE kernels
│   ├── fetch_cspice.sh
│   └── dump_reference_positions.py  # Regenerates the cross-validation fixture
├── src/                        # Python reference implementation
├── tests/                      # Python tests
└── objects.yaml                # J2000 orbital elements (Python reference)
```

## The application

- 3D solar system scene (Three.js) with planet positions computed from the
  DE440 SPICE kernel, a live simulation clock, and orbit trails clamped to
  the kernel time span.
- The 27 major natural satellites (the Moon, Phobos/Deimos, the Galilean
  moons, Saturn's from Mimas to Phoebe, the five Uranian moons, Triton, and
  Pluto's system) from JPL's satellite SPK kernels. Moon trails are
  parent-relative rings that follow their planet; moons are shown in the
  realistic scale mode and hidden in compressed mode. A moon outside its
  kernel's time span (Phobos/Deimos cover 1995-2050 only) is skipped
  instead of breaking the frame.
- Runs on Windows, macOS and Linux; the Tauri entry points follow the
  mobile-ready pattern so Android/iOS targets can be added without
  restructuring.
- The Rust ephemeris core is cross-validated against the Python reference
  implementation, which itself is checked against JPL Horizons.

### Prerequisites

- Rust (stable) and Cargo
- Node.js and npm
- A C compiler (CSPICE is built from source; on Linux the build needs
  `-std=gnu89 -fcommon`, which the build script sets automatically)
- Linux only: Tauri system dependencies
  (https://tauri.app/start/prerequisites/), e.g. on Debian/Ubuntu:
  `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`
- SPICE kernels: run `scripts/fetch_kernels.sh` (downloads `de440s.bsp`,
  `naif0012.tls`, `pck00011.tpc`, `gm_de431.tpc`, plus the satellite SPK
  kernels for the moons — about 2.3 GB total — into `kernels/`).
  Kernels are gitignored due to size.

### Run (desktop)

```bash
scripts/fetch_kernels.sh
scripts/fetch_cspice.sh
cd app/ui && npm install
npm run tauri dev
```

The `tauri` npm script changes to `app/` before invoking the CLI, so the
Tauri project (`src-tauri/`) is found; the frontend dev server and hooks run
from `app/ui` as usual.

The kernel directory is resolved in this order: `PLANETARY_KERNELS_DIR`
environment variable, the repository `kernels/` directory (development),
then `resource_dir/kernels` (bundled builds).

### Build

```bash
cd app/ui && npm run tauri build
```

### Test

```bash
# Rust: catalog, integration (incl. JPL Horizons checkpoints and a
# kernel-edge regression test), and cross-validation against the Python fixture
cd app && cargo test

# Frontend type-check and build
cd app/ui && npm run build

# Python reference suite
pytest
```

Cross-validation tolerances (Rust vs Python, AU): Sun 1e-9, inner planets
0.005, Jupiter/Saturn 0.5, Uranus/Neptune 0.15, Pluto 5.0. These reflect the
known accuracy of the Python analytic model against JPL Horizons.

### Frontend development without Tauri

`app/ui/mock.html` loads the UI with `window.__TAURI_INTERNALS__.invoke`
mocked over HTTP, so the frontend can be developed against a plain
ephemeris server. See `app/ui/src/mock-backend.ts`.

### Mobile

The Tauri shell uses the mobile-ready entry point pattern. To initialize
mobile targets:

```bash
cd app/ui && npm run tauri android init   # requires Android SDK/NDK
cd app/ui && npm run tauri ios init       # requires macOS with Xcode
```

Android local:

```bash
export JAVA_HOME="/data/android/jdk-21.0.12.1+1"
export ANDROID_HOME=/data/android/sdk
export NDK_HOME=/data/android/sdk/ndk/27.0.12077973
cd app/ui
npm run tauri android build -- --target aarch64 --ap
```

## Python reference implementation

The `src/` package computes approximate planet positions from J2000 orbital
elements ([objects.yaml](objects.yaml)) and remains the cross-validation
oracle for the Rust core.

Two issues were found and fixed while cross-validating:

1. **Rotation formula in `src/orbit.py`**: the ecliptic rotation used the
   argument of perihelion where the longitude of the ascending node belongs
   (and vice versa). The two only coincide at near-zero inclination, so Earth
   matched while Mercury was off by ~0.0155 AU in z. The corrected formula
   matches JPL Horizons to ~1e-5 AU.
2. **Orbit paths past the kernel edge**: sampling a full orbital period
   centered on the current time ran past the DE440s kernel time span for
   Pluto (half-period ~124 years). Sampling is now clamped to the kernel
   span, in both the Rust core and the frontend clock.

### Hohmann transfer

Work in progress (Python):

- [X] dV required for A to B transfer
- [X] Period of transfer
- [X] Window size in second

The app ships an interactive transfer tool (right-hand panel):

- Pick a departure and arrival body orbiting the same primary (the Sun or
  a planet for its moons), plus parking-orbit altitudes (default 300 km).
- It reports dV departure/arrival/total (patched-conic burns from and to
  the parking orbits), hyperbolic excess speeds, transfer time, phase
  angle, transfer-plane inclination, synodic period, and the best
  departure window / arrival dates.
- The transfer is a Lambert arc through the bodies' real SPICE positions
  and velocities, so orbital eccentricity, inclination and the true
  phase angle all enter the delta-v. The departure epoch and time of
  flight are optimized on a coarse porkchop grid seeded from the
  idealized Hohmann ellipse (departure scan across one synodic period,
  flight times 0.7-1.4x the Hohmann value).
- Eject/insert geometry: the hyperbolic excess velocity vectors, the
  asymptotes' tilt out of the bodies' orbital planes (the parking-orbit
  plane change the quoted dV assumes), and the burn point in the
  parking orbit (the escape/capture hyperbola's asymptote true
  anomaly).
- N-body check: the arc is re-propagated (RK4) through the gravity of
  the other planets — or sibling moons, for moon-to-moon transfers —
  reporting the arrival miss distance and the sub-m/s departure
  velocity correction that cancels it (first-order differential
  correction).
- The arc is drawn in the 3D view as a dashed light-gray line, from the
  departure body's real position at the window date to the arrival
  body's real position at arrival.
- Two-body dynamics plus the n-body check (no finite burns, no
  perturbed re-optimization): a planning aid, not a flight plan;
  implemented in `app/crates/ephemeris/src/hohmann.rs` (solvers in
  `app/crates/ephemeris/src/lambert.rs` and
  `app/crates/ephemeris/src/nbody.rs`).

## References

- [NAIF SPICE toolkit](https://naif.jpl.nasa.gov/naif/toolkit.html)
- [ssd.jpl.nasa.gov](https://ssd.jpl.nasa.gov/planets/approx_pos.html)
- [www.stjarnhimlen.se](https://www.stjarnhimlen.se/comp/ppcomp.html)
- [www.projectrho.com](http://www.projectrho.com/public_html/rocket/mission.php)
