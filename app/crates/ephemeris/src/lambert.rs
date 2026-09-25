//! Lambert's problem and universal-variable Kepler propagation.
//!
//! Both routines work in km, km/s and seconds, in whatever frame the
//! caller's vectors live in (the crate uses ECLIPJ2000). The Lambert
//! solver follows the universal-variable bisection of Vallado,
//! *Fundamentals of Astrodynamics and Applications*, algorithm 57; the
//! propagator is the universal Kepler equation, algorithm 8.
//!
//! The transfer plane is pinned by a reference normal instead of a
//! prograde flag: the solver sweeps from `r1` to `r2` in the direction
//! dictated by `normal`, which lets callers anchor the transfer to the
//! departure body's actual orbital plane (inclined or even retrograde
//! orbits included) rather than to the ecliptic's north pole.

use std::f64::consts::PI;

use crate::EphemerisError;

/// Inner product of two 3-vectors.
pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Cross product of two 3-vectors.
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Euclidean norm of a 3-vector.
pub fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// Componentwise difference of two 3-vectors.
pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Componentwise sum of two 3-vectors.
pub fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a` scaled by `s`.
pub fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// Unit-length copy of `a`; errors for a zero vector.
pub fn normalize(a: [f64; 3]) -> Result<[f64; 3], EphemerisError> {
    let n = norm(a);
    if !n.is_finite() || n <= 0.0 {
        return Err(EphemerisError::Invalid("cannot normalize a zero vector".into()));
    }
    Ok(scale(a, 1.0 / n))
}

/// Stumpff function C(z).
fn stumpff_c(z: f64) -> f64 {
    if z > 1e-8 {
        (1.0 - z.sqrt().cos()) / z
    } else if z < -1e-8 {
        ((-z).sqrt().cosh() - 1.0) / (-z)
    } else {
        0.5 - z / 24.0 + z * z / 720.0
    }
}

/// Stumpff function S(z).
fn stumpff_s(z: f64) -> f64 {
    if z > 1e-8 {
        (z.sqrt() - z.sqrt().sin()) / (z * z.sqrt())
    } else if z < -1e-8 {
        ((-z).sqrt().sinh() - (-z).sqrt()) / (-z * (-z).sqrt())
    } else {
        1.0 / 6.0 - z / 120.0 + z * z / 5040.0
    }
}

/// Solves Lambert's problem: the two-body trajectory through `r1` at the
/// start and `r2` at the start + `dt` (km, seconds, `mu` in km^3/s^2).
///
/// `normal` pins the transfer plane and sweep direction: the trajectory
/// sweeps from `r1` to `r2` counterclockwise around `normal` (the short
/// way if that sweep is under half a turn, the long way otherwise). It
/// should be the departure orbit's angular momentum direction, so the
/// transfer leaves along the departure body's motion.
///
/// Returns the velocity vectors at the endpoints, km/s.
pub fn solve(
    r1: [f64; 3],
    r2: [f64; 3],
    dt: f64,
    mu: f64,
    normal: [f64; 3],
) -> Result<([f64; 3], [f64; 3]), EphemerisError> {
    if !dt.is_finite() || dt <= 0.0 || !mu.is_finite() || mu <= 0.0 {
        return Err(EphemerisError::Invalid(
            "Lambert needs a positive dt and mu".into(),
        ));
    }
    let r1n = norm(r1);
    let r2n = norm(r2);
    if !r1n.is_finite() || r1n <= 0.0 || !r2n.is_finite() || r2n <= 0.0 {
        return Err(EphemerisError::Invalid(
            "Lambert needs nonzero position vectors".into(),
        ));
    }

    // Sweep angle around the reference normal, in (0, 2*pi).
    let cos_dnu = (dot(r1, r2) / (r1n * r2n)).clamp(-1.0, 1.0);
    let mut dnu = dot(cross(r1, r2), normal).atan2(dot(r1, r2));
    if dnu <= 0.0 {
        dnu += 2.0 * PI;
    }
    // Near 0 or 2*pi the endpoints are parallel, near pi antiparallel:
    // either way the transfer plane is degenerate.
    if (1.0 + cos_dnu).sqrt() < 1e-4 {
        return Err(EphemerisError::Invalid(
            "degenerate Lambert geometry: positions nearly aligned or antiparallel".into(),
        ));
    }

    // A > 0 for the short way (dnu < pi), A < 0 for the long way.
    let mut a = (r1n * r2n * (1.0 + cos_dnu)).sqrt();
    if dnu > PI {
        a = -a;
    }

    // Bisect the universal variable psi; dt(psi) is monotonic on each
    // branch. psi in (-4*pi, 4*pi^2) covers every zero-revolution
    // transfer, and dt is unbounded on the short-way branch.
    let mut psi_low = -4.0 * PI;
    let mut psi_up = 4.0 * PI * PI;
    let mut psi = 0.0;
    let mut y = 0.0;
    let mut converged = false;
    for _ in 0..200 {
        let c = stumpff_c(psi);
        let s = stumpff_s(psi);
        y = r1n + r2n + a * (psi * s - 1.0) / c.sqrt();
        if y > 0.0 {
            let chi = (y / c).sqrt();
            let dt_calc = (chi * chi * chi * s + a * y.sqrt()) / mu.sqrt();
            if (dt_calc - dt).abs() < 1e-3 {
                converged = true;
                break;
            }
            if dt_calc < dt {
                psi_low = psi;
            } else {
                psi_up = psi;
            }
        } else {
            // Outside the branch's valid range: short way runs out for
            // psi too low, long way for psi too high.
            if a > 0.0 {
                psi_low = psi;
            } else {
                psi_up = psi;
            }
        }
        psi = 0.5 * (psi_low + psi_up);
    }
    if !converged {
        return Err(EphemerisError::Invalid(
            "Lambert solver did not converge; the time of flight is unreachable for this geometry".into(),
        ));
    }

    let f = 1.0 - y / r1n;
    let g = a * (y / mu).sqrt();
    let gdot = 1.0 - y / r2n;
    if !g.is_finite() || g == 0.0 {
        return Err(EphemerisError::Invalid(
            "degenerate Lambert geometry: zero transfer angle".into(),
        ));
    }
    let v1 = scale(sub(r2, scale(r1, f)), 1.0 / g);
    let v2 = scale(sub(scale(r2, gdot), r1), 1.0 / g);
    Ok((v1, v2))
}

/// Two-body propagation of a state (km, km/s) by `dt` seconds around a
/// body with GM `mu` (km^3/s^2), via the universal Kepler equation.
///
/// Returns the new position and velocity. Exact for the two-body
/// problem, any conic type.
pub fn propagate(
    r0: [f64; 3],
    v0: [f64; 3],
    dt: f64,
    mu: f64,
) -> Result<([f64; 3], [f64; 3]), EphemerisError> {
    if dt == 0.0 {
        return Ok((r0, v0));
    }
    if !dt.is_finite() || dt <= 0.0 || !mu.is_finite() || mu <= 0.0 {
        return Err(EphemerisError::Invalid(
            "propagation needs a positive dt and mu".into(),
        ));
    }
    let r0n = norm(r0);
    let v0n = norm(v0);
    if !r0n.is_finite() || r0n <= 0.0 {
        return Err(EphemerisError::Invalid(
            "propagation needs a nonzero position".into(),
        ));
    }
    let rdotv = dot(r0, v0);
    let alpha = 2.0 / r0n - v0n * v0n / mu; // 1/a

    // Newton iteration on the universal anomaly chi.
    let mut chi = mu.sqrt() * alpha.abs() * dt;
    let mut r = r0;
    let mut converged = false;
    for _ in 0..100 {
        let z = alpha * chi * chi;
        let c = stumpff_c(z);
        let s = stumpff_s(z);
        let residual = rdotv / mu.sqrt() * chi * chi * c
            + (1.0 - alpha * r0n) * chi * chi * chi * s
            + r0n * chi
            - mu.sqrt() * dt;
        let d_residual = rdotv / mu.sqrt() * chi * (1.0 - z * s)
            + (1.0 - alpha * r0n) * chi * chi * c
            + r0n;
        if d_residual.abs() < 1e-12 {
            break;
        }
        let step = residual / d_residual;
        chi -= step;
        if step.abs() < 1e-9 * chi.abs().max(1.0) {
            // One more evaluation with the updated chi so r matches.
            let z = alpha * chi * chi;
            let c = stumpff_c(z);
            let s = stumpff_s(z);
            let f = 1.0 - chi * chi / r0n * c;
            let g = dt - chi * chi * chi / mu.sqrt() * s;
            r = [
                f * r0[0] + g * v0[0],
                f * r0[1] + g * v0[1],
                f * r0[2] + g * v0[2],
            ];
            converged = true;
            break;
        }
    }
    if !converged {
        return Err(EphemerisError::Invalid(
            "Kepler propagation did not converge".into(),
        ));
    }

    let rn = norm(r);
    if !rn.is_finite() || rn <= 0.0 {
        return Err(EphemerisError::Invalid(
            "degenerate propagation state".into(),
        ));
    }
    let z = alpha * chi * chi;
    let c = stumpff_c(z);
    let s = stumpff_s(z);
    let fdot = mu.sqrt() / (rn * r0n) * chi * (z * s - 1.0);
    let gdot = 1.0 - chi * chi / rn * c;
    let v = [
        fdot * r0[0] + gdot * v0[0],
        fdot * r0[1] + gdot * v0[1],
        fdot * r0[2] + gdot * v0[2],
    ];
    Ok((r, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1 AU circular orbit at 29.78 km/s, like Earth's.
    const R: f64 = 149_597_870.7;
    const V: f64 = 29.78;
    const MU: f64 = V * V * R;

    fn period() -> f64 {
        2.0 * PI * (R * R * R / MU).sqrt()
    }

    fn assert_vec_close(a: [f64; 3], b: [f64; 3], tol: f64, what: &str) {
        for i in 0..3 {
            assert!(
                (a[i] - b[i]).abs() < tol,
                "{what}[{i}]: got {}, expected {} (tol {tol})",
                a[i],
                b[i]
            );
        }
    }

    /// Rotates a vector 30 degrees about the x axis, to tilt test orbits
    /// out of the ecliptic.
    fn tilt(u: [f64; 3]) -> [f64; 3] {
        let (s, c) = (30.0_f64.to_radians().sin(), 30.0_f64.to_radians().cos());
        [u[0], u[1] * c - u[2] * s, u[1] * s + u[2] * c]
    }

    #[test]
    fn quarter_period_short_way_is_exact() {
        // Quarter of a circular orbit: r=(R,0,0) -> (0,R,0).
        let (v1, v2) = solve([R, 0.0, 0.0], [0.0, R, 0.0], period() / 4.0, MU, [0.0, 0.0, 1.0])
            .unwrap();
        assert_vec_close(v1, [0.0, V, 0.0], 1e-9, "v1");
        assert_vec_close(v2, [-V, 0.0, 0.0], 1e-9, "v2");
    }

    #[test]
    fn three_quarter_period_long_way_is_exact() {
        // Sweeping 270 degrees prograde reaches (0,-R,0) after 3/4 period.
        let (v1, v2) = solve(
            [R, 0.0, 0.0],
            [0.0, -R, 0.0],
            3.0 * period() / 4.0,
            MU,
            [0.0, 0.0, 1.0],
        )
        .unwrap();
        assert_vec_close(v1, [0.0, V, 0.0], 1e-9, "v1");
        assert_vec_close(v2, [V, 0.0, 0.0], 1e-9, "v2");
    }

    #[test]
    fn inclined_transfer_follows_the_reference_normal() {
        // Same quarter-period geometry, tilted 30 degrees: the solver must
        // recover the tilted circular orbit, not the ecliptic one.
        let (v1, v2) = solve(
            tilt([R, 0.0, 0.0]),
            tilt([0.0, R, 0.0]),
            period() / 4.0,
            MU,
            tilt([0.0, 0.0, 1.0]),
        )
        .unwrap();
        assert_vec_close(v1, tilt([0.0, V, 0.0]), 1e-9, "v1");
        assert_vec_close(v2, tilt([-V, 0.0, 0.0]), 1e-9, "v2");
    }

    #[test]
    fn antiparallel_positions_are_rejected() {
        let err = solve([R, 0.0, 0.0], [-R, 0.0, 0.0], period() / 2.0, MU, [0.0, 0.0, 1.0]);
        assert!(err.is_err(), "180-degree geometry must not solve");
    }

    #[test]
    fn unreachable_time_of_flight_is_rejected() {
        // Long way around with a time of flight the branch cannot match.
        let err = solve(
            [R, 0.0, 0.0],
            [0.0, -R, 0.0],
            1.0, // one second for a 270-degree sweep
            MU,
            [0.0, 0.0, 1.0],
        );
        assert!(err.is_err(), "impossible dt must not converge");
    }

    #[test]
    fn propagate_quarter_period_is_exact() {
        let (r, v) = propagate([R, 0.0, 0.0], [0.0, V, 0.0], period() / 4.0, MU).unwrap();
        assert_vec_close(r, [0.0, R, 0.0], 1e-6, "r");
        assert_vec_close(v, [-V, 0.0, 0.0], 1e-9, "v");
    }

    #[test]
    fn propagate_full_period_returns_to_start() {
        let (r, v) = propagate([R, 0.0, 0.0], [0.0, V, 0.0], period(), MU).unwrap();
        assert_vec_close(r, [R, 0.0, 0.0], 1e-3, "r");
        assert_vec_close(v, [0.0, V, 0.0], 1e-9, "v");
    }

    #[test]
    fn propagate_half_period_is_exact() {
        let (r, v) = propagate([R, 0.0, 0.0], [0.0, V, 0.0], period() / 2.0, MU).unwrap();
        assert_vec_close(r, [-R, 0.0, 0.0], 1e-6, "r");
        assert_vec_close(v, [0.0, -V, 0.0], 1e-9, "v");
    }

    #[test]
    fn lambert_and_propagate_agree_on_shared_states() {
        // Whatever propagate reaches, Lambert must reproduce: the two
        // routines are inverses on the same conic.
        let (r2, v2) = propagate([R, 0.0, 0.0], [0.0, V, 0.0], 0.37 * period(), MU).unwrap();
        let (lv1, lv2) = solve([R, 0.0, 0.0], r2, 0.37 * period(), MU, [0.0, 0.0, 1.0]).unwrap();
        assert_vec_close(lv1, [0.0, V, 0.0], 1e-6, "v1");
        assert_vec_close(lv2, v2, 1e-6, "v2");
    }
}
