//! Integration tests against real SPICE kernels.
//!
//! Requires kernels in `kernels/` at the repo root (run
//! `scripts/fetch_kernels.sh` first). Reference values come from the JPL
//! Horizons system (heliocentric ECLIPJ2000 geometric states at the J2000
//! epoch, JD 2451545.0 TDB), queried 2026-09-20.

use ephemeris::{body_by_name, Ephemeris, BODIES, SUN};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

// CSPICE keeps process-global state; tests run on parallel threads, so all
// SPICE access in this binary must be serialized.
static SPICE_LOCK: Mutex<()> = Mutex::new(());

fn kernels_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KERNEL_DIR") {
        return PathBuf::from(dir);
    }
    // crates/ephemeris -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join("kernels")
}

/// A loaded ephemeris plus the lock guard keeping SPICE access serialized.
/// Keep the guard alive for the whole test.
fn eph() -> (Ephemeris, MutexGuard<'static, ()>) {
    let guard = SPICE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let eph = Ephemeris::new(kernels_dir()).expect("kernels not found; run scripts/fetch_kernels.sh");
    (eph, guard)
}

/// Earth heliocentric position at J2000, from Horizons (DE441), AU.
const EARTH_J2000_POS: [f64; 3] = [
    -1.771350992727098e-01,
    9.672416867665306e-01,
    -4.085281582511366e-06,
];
/// Earth heliocentric velocity at J2000, from Horizons, AU/day.
const EARTH_J2000_VEL: [f64; 3] = [
    -1.720762506872895e-02,
    -3.158782144324866e-03,
    1.049888594613343e-07,
];
/// Mars heliocentric position at J2000, from Horizons, AU.
const MARS_J2000_POS: [f64; 3] = [
    1.390715921746351e+00,
    -1.341631815101244e-02,
    -3.446766277581799e-02,
];

fn assert_close(actual: [f64; 3], expected: [f64; 3], tol: f64, what: &str) {
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert!(
            (a - e).abs() < tol,
            "{what}[{i}]: got {a}, expected {e} (tol {tol})"
        );
    }
}

#[test]
fn j2000_epoch_maps_to_et_zero() {
    let (eph, _lock) = eph();
    let et = eph.utc_to_et("2000-01-01 12:00:00 TDB").unwrap();
    assert!((et).abs() < 1e-3, "et at J2000 should be ~0, got {et}");
}

#[test]
fn utc_to_et_and_back_round_trip() {
    let (eph, _lock) = eph();
    let utc = "2024-06-01 12:34:56";
    let et = eph.utc_to_et(utc).unwrap();
    let back = eph.et_to_utc(et).unwrap();
    assert_eq!(back, "2024-06-01T12:34:56.000");
}

#[test]
fn sun_is_at_origin() {
    let (eph, _lock) = eph();
    let pos = eph.position(&SUN, 0.0).unwrap();
    assert_eq!(pos, [0.0, 0.0, 0.0]);
    let state = eph.state(&SUN, 12345.0).unwrap();
    assert_eq!(state.position_au, [0.0, 0.0, 0.0]);
}

#[test]
fn earth_position_matches_horizons_at_j2000() {
    let (eph, _lock) = eph();
    let earth = body_by_name("earth").unwrap();
    let pos = eph.position(earth, 0.0).unwrap();
    // de440s vs the DE441 Horizons source differ by well under 1 km;
    // 1e-6 AU (~150 km) leaves generous margin.
    assert_close(pos, EARTH_J2000_POS, 1e-6, "Earth position");
}

#[test]
fn earth_velocity_matches_horizons_at_j2000() {
    let (eph, _lock) = eph();
    let earth = body_by_name("earth").unwrap();
    let state = eph.state(earth, 0.0).unwrap();
    assert_close(state.velocity_au_per_day, EARTH_J2000_VEL, 1e-8, "Earth velocity");
}

#[test]
fn mars_position_matches_horizons_at_j2000() {
    let (eph, _lock) = eph();
    let mars = body_by_name("mars").unwrap();
    let pos = eph.position(mars, 0.0).unwrap();
    assert_close(pos, MARS_J2000_POS, 1e-6, "Mars position");
}

#[test]
fn planets_have_positions_across_the_kernel_span() {
    let (eph, _lock) = eph();
    // de440s covers 1849-2150; sample near both ends.
    for date in ["1850-01-01 00:00:00", "2149-01-01 00:00:00"] {
        let et = eph.utc_to_et(date).unwrap();
        for body in BODIES {
            if body.parent.is_some() {
                continue; // moon kernels have narrower, per-kernel spans
            }
            let pos = eph.position(body, et).unwrap();
            assert!(
                pos.iter().all(|c| c.is_finite()),
                "{date}: {} has non-finite position",
                body.name
            );
        }
    }
}

#[test]
fn moon_distance_from_earth_is_lunar() {
    let (eph, _lock) = eph();
    let moon = body_by_name("moon").unwrap();
    let earth = body_by_name("earth").unwrap();
    let et = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let rel = eph.position_relative(moon, earth, et).unwrap();
    let dist_km = rel.iter().map(|c| c * c).sum::<f64>().sqrt() * ephemeris::KM_PER_AU;
    // Perigee 356,500 km, apogee 406,700 km.
    assert!(
        (350_000.0..=410_000.0).contains(&dist_km),
        "Moon distance {dist_km} km is not lunar"
    );
}

#[test]
fn moon_orbit_path_is_parent_relative_and_closed() {
    let (eph, _lock) = eph();
    let moon = body_by_name("moon").unwrap();
    let et = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let path = eph.orbit_path(moon, et, 512).unwrap();
    assert_eq!(path.len(), 512);
    // Parent-relative: every point sits a lunar distance from the origin
    // (a heliocentric path would be ~1 AU out).
    for p in &path {
        let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        assert!(
            (0.0023..=0.0029).contains(&r),
            "Moon orbit radius {r} AU is not Earth-relative"
        );
    }
    // One sidereal period later the moon is back: ends nearly coincide.
    let d = (path[0][0] - path[511][0]).abs()
        + (path[0][1] - path[511][1]).abs()
        + (path[0][2] - path[511][2]).abs();
    assert!(d < 1e-4, "moon orbit path not closed, endpoint gap {d}");
}

#[test]
fn moon_orbital_elements_are_parent_relative() {
    let (eph, _lock) = eph();
    let io = body_by_name("io").unwrap();
    let et = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let el = eph.orbital_elements(io, et).unwrap();
    // Io orbits Jupiter at ~421,700 km = 0.002819 AU, period 1.769 days.
    assert!(
        (el.semi_major_axis_au - 0.002819).abs() < 1e-4,
        "Io a = {} AU",
        el.semi_major_axis_au
    );
    assert!(
        (el.period_days - 1.769).abs() < 0.01,
        "Io period = {} days",
        el.period_days
    );
}

#[test]
fn hohmann_earth_to_mars_matches_textbook_values() {
    let (eph, _lock) = eph();
    let earth = body_by_name("earth").unwrap();
    let mars = body_by_name("mars").unwrap();
    let et = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let (t, path) = eph
        .hohmann_transfer(earth, mars, et, 300e3, 300e3)
        .unwrap();

    // Earth -> Mars around the 2026 window, 300 km parking orbits.
    // The circular-coplanar textbook gives dV ~3.6 + ~2.1 km/s, TOF
    // ~259 d, phase ~44.3 deg; the Lambert arc through real positions
    // optimizes to ~3.62 + ~2.05 km/s over ~310 days of flight
    // (verified against an independent porkchop scan). The phase angle
    // at the optimized window is larger than the textbook value because
    // the longer flight lets Mars travel further before interception.
    assert!((t.dv_departure_m_s - 3600.0).abs() < 150.0, "dV dep {}", t.dv_departure_m_s);
    assert!((t.dv_arrival_m_s - 2050.0).abs() < 150.0, "dV arr {}", t.dv_arrival_m_s);
    assert!((t.dv_total_m_s - 5670.0).abs() < 250.0, "dV tot {}", t.dv_total_m_s);
    let tof_days = t.transfer_time_s / 86400.0;
    assert!((tof_days - 310.0).abs() < 20.0, "TOF {tof_days} days");
    assert!(
        (40.0..70.0).contains(&t.phase_angle_deg),
        "phase {}",
        t.phase_angle_deg
    );
    let synodic_days = t.synodic_period_s / 86400.0;
    assert!((synodic_days - 780.0).abs() < 15.0, "synodic {synodic_days} days");

    // The window is in the future and the arrival follows the flight time.
    assert!(t.departure_et >= et);
    assert!((t.arrival_et - t.departure_et - t.transfer_time_s).abs() < 1.0);

    // The arc is 3D: it starts exactly at Earth's real position at the
    // departure epoch and ends at Mars' real position at arrival.
    assert_eq!(path.len(), 129);
    let earth_dep = eph.position_relative(earth, &SUN, t.departure_et).unwrap();
    let mars_arr = eph.position_relative(mars, &SUN, t.arrival_et).unwrap();
    assert_close(path[0], earth_dep, 1e-9, "path start at Earth");
    assert_close(path[128], mars_arr, 1e-6, "path end at Mars");
    assert!(path.iter().all(|p| p.iter().all(|c| c.is_finite())));
    // Both bodies orbit near the ecliptic, so the transfer plane is
    // only slightly inclined.
    assert!(
        t.transfer_inclination_deg < 5.0,
        "transfer inclination {} deg",
        t.transfer_inclination_deg
    );

    // N-body refinement: the other planets bend the arc by tens of
    // thousands of km over the flight; a sub-m/s departure correction
    // cancels almost all of it.
    let miss = t.nbody_miss_km.expect("n-body miss");
    let corr = t.nbody_correction_m_s.expect("n-body correction");
    let resid = t.nbody_residual_km.expect("n-body residual");
    assert!((1e3..1e6).contains(&miss), "n-body miss {miss} km");
    assert!(corr < 10.0, "n-body correction {corr} m/s");
    assert!(resid < 100.0, "n-body residual {resid} km");
    assert!(resid < 0.01 * miss, "correction ineffective: {resid} vs {miss}");

    // Eject/insert geometry: the burn happens at the escape/capture
    // hyperbola's periapsis, ~150 deg from the departure asymptote
    // for Earth's ~3 km/s v_inf at 300 km altitude, ~130 deg for the
    // Mars capture burn.
    assert!(
        (145.0..155.0).contains(&t.eject_burn_angle_deg),
        "eject burn angle {}",
        t.eject_burn_angle_deg
    );
    assert!(
        (125.0..135.0).contains(&t.insert_burn_angle_deg),
        "insert burn angle {}",
        t.insert_burn_angle_deg
    );
    // The asymptotes tilt out of the bodies' orbital planes: the
    // transfer plane is anchored at Earth but tilted to reach Mars,
    // and the small v_inf amplifies that tilt into a double-digit
    // departure declination.
    assert!(
        (5.0..20.0).contains(&t.eject_declination_deg),
        "eject declination {}",
        t.eject_declination_deg
    );
    assert!(
        (2.0..12.0).contains(&t.insert_declination_deg),
        "insert declination {}",
        t.insert_declination_deg
    );
}

#[test]
fn hohmann_inbound_transfer_has_negative_phase_angle() {
    let (eph, _lock) = eph();
    let mars = body_by_name("mars").unwrap();
    let earth = body_by_name("earth").unwrap();
    let et = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let (t, path) = eph
        .hohmann_transfer(mars, earth, et, 300e3, 300e3)
        .unwrap();
    // Mars -> Earth: the target trails, so the phase angle is negative.
    assert!(t.phase_angle_deg < 0.0, "phase {}", t.phase_angle_deg);
    // The flight time is optimized per direction, so it differs from
    // the outbound leg; both stay within the search grid.
    let (t2, _) = eph
        .hohmann_transfer(earth, mars, et, 300e3, 300e3)
        .unwrap();
    assert!((t.transfer_time_s - t2.transfer_time_s).abs() > 1.0);
    assert!(t.transfer_time_s > 0.0);
    // The path starts at Mars' real position and ends at Earth's.
    let mars_dep = eph.position_relative(mars, &SUN, t.departure_et).unwrap();
    let earth_arr = eph.position_relative(earth, &SUN, t.arrival_et).unwrap();
    assert_close(path[0], mars_dep, 1e-9, "path start at Mars");
    assert_close(path[128], earth_arr, 1e-6, "path end at Earth");
}

#[test]
fn hohmann_works_between_moons_of_the_same_planet() {
    let (eph, _lock) = eph();
    let io = body_by_name("io").unwrap();
    let europa = body_by_name("europa").unwrap();
    let jupiter = body_by_name("jupiter").unwrap();
    let et = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let (t, path) = eph.hohmann_transfer(io, europa, et, 100e3, 100e3).unwrap();
    // Io -> Europa around Jupiter: the Hohmann seed is ~1.3 days of
    // flight; the optimized time of flight stays within the grid
    // (0.7-1.4x the seed).
    let tof_days = t.transfer_time_s / 86400.0;
    assert!(
        (0.9..=1.9).contains(&tof_days),
        "TOF {tof_days} days is outside the search grid"
    );
    // Parent-relative path: it starts at Io's real position and ends at
    // Europa's, both Jupiter-relative.
    let io_dep = eph.position_relative(io, jupiter, t.departure_et).unwrap();
    let europa_arr = eph.position_relative(europa, jupiter, t.arrival_et).unwrap();
    assert_close(path[0], io_dep, 1e-9, "path start at Io");
    assert_close(path[128], europa_arr, 1e-6, "path end at Europa");
    // Sibling moons (Ganymede, Callisto) and the Sun bend the arc
    // only slightly over the short flight; the correction is tiny.
    let miss = t.nbody_miss_km.expect("n-body miss");
    assert!(miss < 1e4, "n-body miss {miss} km");
    assert!(t.nbody_correction_m_s.unwrap() < 5.0, "n-body correction too big");
}

#[test]
fn hohmann_takes_target_inclination_into_account() {
    // Mercury's orbit is inclined 7 degrees to the ecliptic; the
    // transfer plane must tilt out of the ecliptic to reach it.
    let (eph, _lock) = eph();
    let earth = body_by_name("earth").unwrap();
    let mercury = body_by_name("mercury").unwrap();
    let et = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let (t, path) = eph.hohmann_transfer(earth, mercury, et, 300e3, 300e3).unwrap();
    assert!(
        t.transfer_inclination_deg > 0.5,
        "transfer inclination {} deg should be nontrivial",
        t.transfer_inclination_deg
    );
    // The arc really reaches Mercury's out-of-plane position.
    let mercury_arr = eph.position_relative(mercury, &SUN, t.arrival_et).unwrap();
    assert_close(path[128], mercury_arr, 1e-6, "path end at Mercury");
    // Mercury's 7-degree orbital inclination shows up amplified in
    // the arrival asymptote geometry.
    assert!(
        t.insert_declination_deg > 10.0,
        "insert declination {}",
        t.insert_declination_deg
    );
    // The n-body check works on short, steeply inclined arcs too.
    let miss = t.nbody_miss_km.expect("n-body miss");
    let resid = t.nbody_residual_km.expect("n-body residual");
    assert!(t.nbody_correction_m_s.unwrap() < 10.0);
    assert!(resid < 0.01 * miss + 1.0, "residual {resid} vs miss {miss}");
}

#[test]
fn lambert_arc_matches_real_two_body_motion() {
    // Mars' heliocentric motion is two-body to a few m/s over 40 days,
    // so Lambert between two real Mars positions must recover Mars'
    // actual velocity at both ends.
    let (eph, _lock) = eph();
    let mars = body_by_name("mars").unwrap();
    let et = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let dt = 40.0 * 86400.0;
    let (r1, v1) = eph.state_relative_km(mars, &SUN, et).unwrap();
    let (r2, v2) = eph.state_relative_km(mars, &SUN, et + dt).unwrap();
    // GM of the Sun (DE440) plus Mars, as oscelt uses.
    let mu = 1.32712440018e11 + mars.gm_from_mass_km3_s2();
    let n_hat = ephemeris::lambert::normalize(ephemeris::lambert::cross(r1, v1)).unwrap();
    let (lv1, lv2) = ephemeris::lambert::solve(r1, r2, dt, mu, n_hat).unwrap();
    let err1 = ephemeris::lambert::norm(ephemeris::lambert::sub(lv1, v1));
    let err2 = ephemeris::lambert::norm(ephemeris::lambert::sub(lv2, v2));
    assert!(err1 < 0.02, "departure velocity off by {err1:.4} km/s");
    assert!(err2 < 0.02, "arrival velocity off by {err2:.4} km/s");
}

#[test]
fn hohmann_rejects_impossible_transfers() {
    let (eph, _lock) = eph();
    let earth = body_by_name("earth").unwrap();
    let moon = body_by_name("moon").unwrap();
    let io = body_by_name("io").unwrap();
    let sun = &SUN;
    let et = 0.0;
    // Same body.
    assert!(eph.hohmann_transfer(earth, earth, et, 0.0, 0.0).is_err());
    // Different primaries (Earth orbits the Sun, the Moon orbits Earth).
    assert!(eph.hohmann_transfer(earth, moon, et, 0.0, 0.0).is_err());
    assert!(eph.hohmann_transfer(earth, io, et, 0.0, 0.0).is_err());
    // The Sun does not orbit anything.
    assert!(eph.hohmann_transfer(sun, earth, et, 0.0, 0.0).is_err());
}

#[test]
fn moon_outside_its_kernel_span_degrades_gracefully() {
    let (eph, _lock) = eph();
    let phobos = body_by_name("phobos").unwrap();
    // mar099s covers 1995-2050 only; 1850 is outside it.
    let et = eph.utc_to_et("1850-01-01 00:00:00").unwrap();
    assert!(eph.position(phobos, et).is_err());
    // The orbit path truncates to a partial (here: empty) arc, not an error.
    let path = eph.orbit_path(phobos, et, 64).unwrap();
    assert!(path.len() < 64, "Phobos path should be truncated at 1850");
    // The error state must not poison subsequent queries.
    let now = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let path = eph.orbit_path(phobos, now, 64).unwrap();
    assert_eq!(path.len(), 64, "Phobos path should be full in 2026");
    let rel = eph
        .position_relative(phobos, body_by_name("mars").unwrap(), now)
        .unwrap();
    let dist_km = rel.iter().map(|c| c * c).sum::<f64>().sqrt() * ephemeris::KM_PER_AU;
    // Phobos orbits Mars at 9,376 km.
    assert!((dist_km - 9376.0).abs() < 500.0, "Phobos distance {dist_km} km");
}

#[test]
fn orbit_path_is_closed_and_sun_centered() {
    let (eph, _lock) = eph();
    let earth = body_by_name("earth").unwrap();
    let path = eph.orbit_path(earth, 0.0, 512).unwrap();
    assert_eq!(path.len(), 512);
    // One sidereal period later the body is back: first and last samples
    // nearly coincide.
    let d = (path[0][0] - path[511][0]).abs()
        + (path[0][1] - path[511][1]).abs()
        + (path[0][2] - path[511][2]).abs();
    assert!(d < 1e-3, "orbit path not closed, endpoint gap {d}");
    // Every point sits near 1 AU from the Sun.
    for p in &path {
        let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        assert!((r - 1.0).abs() < 0.03, "Earth orbit radius {r}");
    }
}

#[test]
fn sun_has_no_orbit_path() {
    let (eph, _lock) = eph();
    assert!(eph.orbit_path(&SUN, 0.0, 100).is_err());
}

#[test]
fn pluto_trail_at_present_does_not_leave_the_kernel() {
    // Pluto's half-period is ~124 years; a naive full-period trail around
    // the present runs past de440s's 2150 edge. The path must clamp to a
    // partial arc instead of erroring.
    let (eph, _lock) = eph();
    let pluto = body_by_name("pluto").unwrap();
    let now = eph.utc_to_et("2026-01-01 00:00:00").unwrap();
    let path = eph.orbit_path(pluto, now, 256).unwrap();
    assert_eq!(path.len(), 256);
    for p in &path {
        assert!(p.iter().all(|c| c.is_finite()));
    }
}

#[test]
fn orbital_elements_of_earth_are_sane() {
    let (eph, _lock) = eph();
    let earth = body_by_name("earth").unwrap();
    let el = eph.orbital_elements(earth, 0.0).unwrap();
    assert!((el.semi_major_axis_au - 1.0000).abs() < 1e-3, "a = {}", el.semi_major_axis_au);
    assert!((el.eccentricity - 0.0167).abs() < 1e-3, "e = {}", el.eccentricity);
    assert!(el.inclination_deg.abs() < 0.1, "i = {}", el.inclination_deg);
    assert!((el.period_days - 365.25).abs() < 1.0, "P = {}", el.period_days);
}

#[test]
fn out_of_span_time_is_an_error_not_a_crash() {
    let (eph, _lock) = eph();
    let mercury = body_by_name("mercury").unwrap();
    // 1700-01-01 is outside de440s coverage (1849-2150), and no satellite
    // kernel carries Mercury (Earth at 1700 now resolves via jup365).
    let et = eph.utc_to_et("1700-01-01 00:00:00").unwrap();
    let err = eph.position(mercury, et).unwrap_err();
    assert!(
        err.to_string().contains("SPICE"),
        "expected a SPICE error, got {err}"
    );
    // The error state must not poison subsequent queries.
    let pos = eph.position(mercury, 0.0).unwrap();
    assert!(
        pos.iter().all(|c| c.is_finite()),
        "Mercury position after error is not finite"
    );
}

#[test]
fn missing_kernel_dir_is_a_clear_error() {
    let err = match Ephemeris::new("/nonexistent-kernel-dir") {
        Err(e) => e,
        Ok(_) => panic!("kernel dir should not have loaded"),
    };
    assert!(err.to_string().contains("fetch_kernels"));
}
