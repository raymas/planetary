"""Dump heliocentric ecliptic positions from the Keplerian reference model.

Cross-validation oracle for the SPICE-backed Rust core (Phase 2 of the
refactor): writes the positions of every body in res/objects.yaml at a set
of UTC dates, in AU, ECLIPJ2000, so a Rust test can compare them against
SPICE queries for the same instants.

Usage:
    python3 scripts/dump_reference_positions.py [output.json]

The default output is app/crates/ephemeris/tests/data/reference_positions.json
and is committed to the repository so `cargo test` does not need Python.
"""

import datetime
import json
import pathlib
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from src.orbit import Orbit  # noqa: E402
from src.timeframe import TimeFrame  # noqa: E402

AU_M = 149597870700.0

# Sample dates inside the JPL approximate-elements validity window
# (1800-2050, best within +/-50 years of J2000).
DATES = [
    "1950-01-01T00:00:00",
    "1975-01-01T00:00:00",
    "2000-01-01T00:00:00",
    "2024-06-01T00:00:00",
    "2050-01-01T00:00:00",
]

DEFAULT_OUTPUT = ROOT / "app" / "crates" / "ephemeris" / "tests" / "data" / "reference_positions.json"


def main() -> None:
    with open(ROOT / "res" / "objects.yaml") as f:
        objects = yaml.safe_load(f)["objects"]

    orbits = {name: Orbit(spec["j2000"]) for name, spec in objects.items()}

    bodies = {}
    for name, orbit in orbits.items():
        positions = []
        for date in DATES:
            t = TimeFrame()
            t.set_date(datetime.datetime.fromisoformat(date).replace(tzinfo=datetime.timezone.utc))
            pos_m = orbit.compute_orbit(t)
            positions.append([float(c / AU_M) for c in pos_m])
        bodies[name] = positions

    out = {
        "description": "Keplerian (JPL approximate elements) heliocentric positions, AU, ECLIPJ2000",
        "dates_utc": DATES,
        "bodies": bodies,
    }

    output = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_OUTPUT
    output.parent.mkdir(parents=True, exist_ok=True)
    with open(output, "w") as f:
        json.dump(out, f, indent=1)
    print(f"wrote {output}")


if __name__ == "__main__":
    main()
