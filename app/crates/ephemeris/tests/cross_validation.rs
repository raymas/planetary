//! Cross-validation: SPICE vs the Python Keplerian reference (Phase 2).
//!
//! `scripts/dump_reference_positions.py` dumps heliocentric ECLIPJ2000
//! positions from the Python prototype's Keplerian propagation (JPL
//! approximate elements, valid 1800-2050). This test queries SPICE for the
//! same instants and asserts agreement within per-body tolerances.
//!
//! The tolerances reflect the documented accuracy of the JPL approximate
//! element tables (arcminutes for inner planets near J2000, degrading with
//! time from epoch and distance; Jupiter/Saturn carry the great-inequality
//! term, Pluto is the worst case). They guard against unit and frame
//! mistakes — the classic ecliptic-vs-equatorial bug — not against
//! sub-arcsecond ephemeris error.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use ephemeris::{Ephemeris, BODIES};
use serde::Deserialize;

static SPICE_LOCK: Mutex<()> = Mutex::new(());

fn kernels_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KERNEL_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join("kernels")
}

fn eph() -> (Ephemeris, MutexGuard<'static, ()>) {
    let guard = SPICE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let eph = Ephemeris::new(kernels_dir()).expect("kernels not found; run scripts/fetch_kernels.sh");
    (eph, guard)
}

#[derive(Deserialize)]
struct Reference {
    #[serde(rename = "dates_utc")]
    dates: Vec<String>,
    bodies: HashMap<String, Vec<[f64; 3]>>,
}

/// Max |SPICE - Keplerian| position error per body, in AU.
///
/// Calibrated against the committed reference data (worst measured deltas
/// across 1950-2050: inner planets <= 6e-4, Jupiter 0.010, Saturn 0.033,
/// Uranus 0.012, Neptune 0.010, Pluto 0.30 — the last reflecting Pluto's
/// crude Keplerian elements and Jupiter/Saturn the great-inequality terms
/// commented out in objects.yaml). Roughly 10x margin.
fn tolerance_au(body: &str) -> f64 {
    match body {
        "sun" => 1e-9,
        "mercury" | "venus" | "earth" | "mars" => 0.005,
        "jupiter" | "saturn" => 0.5,
        "uranus" | "neptune" => 0.15,
        "pluto" => 5.0,
        _ => 0.01,
    }
}

#[test]
fn spice_matches_keplerian_reference() {
    let (eph, _lock) = eph();

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/reference_positions.json");
    let reference: Reference =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("reference data present"))
            .expect("reference data parses");

    assert_eq!(
        reference.dates.len(),
        5,
        "expected 5 sample dates in the reference dump"
    );

    let mut worst: HashMap<&str, f64> = HashMap::new();
    for body in BODIES {
        // The reference dump covers the Sun and planets only; moons have no
        // Keplerian counterpart to compare against.
        let Some(keplerian) = reference.bodies.get(body.name) else {
            continue;
        };
        assert_eq!(keplerian.len(), reference.dates.len());

        for (i, date) in reference.dates.iter().enumerate() {
            let et = eph.utc_to_et(date).expect("date converts");
            let spice = eph.position(body, et).expect("SPICE position");
            let delta = (spice[0] - keplerian[i][0]).abs()
                + (spice[1] - keplerian[i][1]).abs()
                + (spice[2] - keplerian[i][2]).abs();
            worst.entry(body.name).and_modify(|w| *w = w.max(delta)).or_insert(delta);

            let tol = tolerance_au(body.name);
            let name = body.name;
            assert!(
                delta < tol,
                "{name} at {date}: SPICE {spice:?} vs Keplerian {:?}, |delta| {delta:.6} >= tol {tol}",
                keplerian[i]
            );
        }
    }

    // Report the measured worst-case deltas so future edits can keep the
    // tolerances honest.
    let mut report: Vec<_> = worst.iter().collect();
    report.sort_by_key(|(name, _)| *name);
    for (name, delta) in report {
        println!("{name:>8}: max |delta| = {delta:.6} AU (tol {})", tolerance_au(name));
    }
}
