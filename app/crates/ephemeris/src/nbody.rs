//! N-body refinement of a two-body transfer arc.
//!
//! The Lambert solution pretends only the primary exists. Real
//! spacecraft fly through the gravity of every other body in the
//! system, so the arc arrives somewhere other than the target. This
//! module propagates the departure state with a fixed-step RK4
//! integrator through the gravity of the primary plus the major
//! perturbing bodies — positions sampled from SPICE once and
//! cubic-Hermite interpolated between knots — measures the arrival
//! miss, and computes the small departure-velocity correction that
//! cancels it (a first-order differential correction, one Newton
//! step on the miss vector).
//!
//! The departure and arrival bodies are excluded from the perturber
//! set: patched conics already account for them (escape hyperbola,
//! capture), and including them would put the integrator inside their
//! sphere of influence at the endpoints.

use crate::lambert::{add, cross, dot, norm, scale, sub};
use crate::{Body, Ephemeris, EphemerisError, BODIES, SUN};

/// Trajectory knots per perturber (cubic Hermite between knots).
const KNOTS: usize = 256;
/// RK4 steps along the arc.
const STEPS: usize = 1024;
/// Finite-difference step for the correction Jacobian, km/s.
const JACOBIAN_STEP: f64 = 1e-4;

/// One perturbing body: GM plus its trajectory sampled relative to
/// the primary at `KNOTS + 1` equally spaced epochs.
pub(crate) struct Perturber {
    gm: f64,
    /// (position, velocity) pairs, km and km/s, parent-relative.
    samples: Vec<([f64; 3], [f64; 3])>,
    /// First knot epoch, TDB seconds past J2000.
    t0: f64,
    /// Knot spacing, seconds.
    h: f64,
}

impl Perturber {
    /// Cubic-Hermite position at `et`, km. The interpolation error
    /// scales as h^4: about 0.3 km for a 1 AU orbit with daily knots,
    /// ~10 km for Mercury with the ~1.2-day knots of a 256-sample
    /// arc. That error enters the spacecraft's acceleration only
    /// through the perturber's tidal term, where it is negligible.
    fn position_at(&self, et: f64) -> [f64; 3] {
        let u = ((et - self.t0) / self.h).clamp(0.0, (self.samples.len() - 1) as f64);
        let i = (u.floor() as usize).min(self.samples.len() - 2);
        let s = u - i as f64;
        let (p0, v0) = self.samples[i];
        let (p1, v1) = self.samples[i + 1];
        let s2 = s * s;
        let s3 = s2 * s;
        let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let h10 = s3 - 2.0 * s2 + s;
        let h01 = -2.0 * s3 + 3.0 * s2;
        let h11 = s3 - s2;
        [
            h00 * p0[0] + h10 * self.h * v0[0] + h01 * p1[0] + h11 * self.h * v1[0],
            h00 * p0[1] + h10 * self.h * v0[1] + h01 * p1[1] + h11 * self.h * v1[1],
            h00 * p0[2] + h10 * self.h * v0[2] + h01 * p1[2] + h11 * self.h * v1[2],
        ]
    }
}

/// Primary-relative n-body acceleration at `et`, km/s^2: the primary's
/// two-body term plus each perturber's direct pull and the indirect
/// term (the primary's own acceleration by the perturber).
fn acceleration(mu_p: f64, r: [f64; 3], et: f64, perturbers: &[Perturber]) -> [f64; 3] {
    let rn = norm(r);
    let mut a = scale(r, -mu_p / (rn * rn * rn));
    for p in perturbers {
        let ri = p.position_at(et);
        let d = sub(ri, r);
        let dn = norm(d);
        let rin = norm(ri);
        let direct = scale(d, 1.0 / (dn * dn * dn));
        let indirect = scale(ri, 1.0 / (rin * rin * rin));
        a = add(a, scale(sub(direct, indirect), p.gm));
    }
    a
}

/// Propagates the state (km, km/s) for `tof` seconds from `et0` under
/// the primary plus perturbers, fixed-step RK4. Returns the arrival
/// state, parent-relative.
pub(crate) fn propagate(
    mu_p: f64,
    perturbers: &[Perturber],
    r0: [f64; 3],
    v0: [f64; 3],
    et0: f64,
    tof: f64,
) -> ([f64; 3], [f64; 3]) {
    let dt = tof / STEPS as f64;
    let mut r = r0;
    let mut v = v0;
    for i in 0..STEPS {
        let t = et0 + dt * i as f64;
        let a1 = acceleration(mu_p, r, t, perturbers);
        let v2 = add(v, scale(a1, dt / 2.0));
        let a2 = acceleration(mu_p, add(r, scale(v, dt / 2.0)), t + dt / 2.0, perturbers);
        let v3 = add(v, scale(a2, dt / 2.0));
        let a3 = acceleration(mu_p, add(r, scale(v2, dt / 2.0)), t + dt / 2.0, perturbers);
        let v4 = add(v, scale(a3, dt));
        let a4 = acceleration(mu_p, add(r, scale(v3, dt)), t + dt, perturbers);
        let dr = scale(
            add(add(v, scale(v2, 2.0)), add(scale(v3, 2.0), v4)),
            dt / 6.0,
        );
        let dv = scale(
            add(add(a1, scale(a2, 2.0)), add(scale(a3, 2.0), a4)),
            dt / 6.0,
        );
        r = add(r, dr);
        v = add(v, dv);
    }
    (r, v)
}

/// Solves the 3x3 linear system with columns `j` times x = b; None if
/// singular.
fn solve3(j: &[[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = dot(j[0], cross(j[1], j[2]));
    if !det.is_finite() || det == 0.0 {
        return None;
    }
    Some([
        dot(b, cross(j[1], j[2])) / det,
        dot(j[0], cross(b, j[2])) / det,
        dot(j[0], cross(j[1], b)) / det,
    ])
}

/// The result of checking a Lambert arc against n-body gravity.
#[derive(Clone, Copy)]
pub(crate) struct NBodyRefinement {
    /// Arrival miss distance of the uncorrected arc, km.
    pub miss_km: f64,
    /// Departure velocity correction that cancels the miss, km/s,
    /// parent-relative.
    pub correction_km_s: [f64; 3],
    /// Remaining miss after applying the correction, km.
    pub residual_km: f64,
}

impl Ephemeris {
    /// Samples the perturbing bodies for a transfer `from` -> `to`
    /// around `primary`: the other planets (Sun primary) or the
    /// sibling moons plus the Sun (moon transfers). Bodies whose
    /// ephemeris does not cover the arc are skipped, not fatal.
    pub(crate) fn sample_perturbers(
        &self,
        from: &Body,
        to: &Body,
        primary: &Body,
        et0: f64,
        tof: f64,
    ) -> Result<Vec<Perturber>, EphemerisError> {
        let h = tof / KNOTS as f64;
        let sun_primary = primary.naif_id == SUN.naif_id;
        let mut perturbers = Vec::new();
        'outer: for body in BODIES {
            if body.naif_id == from.naif_id
                || body.naif_id == to.naif_id
                || body.naif_id == primary.naif_id
            {
                continue;
            }
            let sibling = if sun_primary {
                body.parent.is_none()
            } else {
                body.parent == Some(primary.name) || body.naif_id == SUN.naif_id
            };
            if !sibling {
                continue;
            }
            // PCK GMs for the Sun and planets; catalog masses for moons.
            let gm = if body.parent.is_some() {
                body.gm_from_mass_km3_s2()
            } else {
                self.gm(body)?
            };
            let mut samples = Vec::with_capacity(KNOTS + 1);
            for i in 0..=KNOTS {
                match self.state_relative_km(body, primary, et0 + h * i as f64) {
                    Ok(s) => samples.push(s),
                    // Outside kernel coverage: drop this perturber.
                    Err(_) => continue 'outer,
                }
            }
            perturbers.push(Perturber {
                gm,
                samples,
                t0: et0,
                h,
            });
        }
        Ok(perturbers)
    }

    /// Checks a Lambert arc against n-body gravity: propagates the
    /// departure state through the perturbers' pull, measures the
    /// arrival miss against the target's real position, and computes
    /// the departure-velocity correction that cancels it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn nbody_refinement(
        &self,
        from: &Body,
        to: &Body,
        primary: &Body,
        r1: [f64; 3],
        v1: [f64; 3],
        departure_et: f64,
        tof: f64,
    ) -> Result<NBodyRefinement, EphemerisError> {
        let mu_p = self.gm(primary)?;
        let perturbers = self.sample_perturbers(from, to, primary, departure_et, tof)?;
        let (r_arr, _) = propagate(mu_p, &perturbers, r1, v1, departure_et, tof);
        let r_target = self.state_relative_km(to, primary, departure_et + tof)?.0;
        let miss = sub(r_target, r_arr);

        // First-order differential correction: the Jacobian maps a
        // departure velocity change to an arrival position change.
        let mut j = [[0.0f64; 3]; 3];
        for ax in 0..3 {
            let mut vp = v1;
            vp[ax] += JACOBIAN_STEP;
            let (rp, _) = propagate(mu_p, &perturbers, r1, vp, departure_et, tof);
            j[ax] = scale(sub(rp, r_arr), 1.0 / JACOBIAN_STEP);
        }
        let dv = solve3(&j, miss).ok_or_else(|| {
            EphemerisError::Invalid("ill-conditioned n-body correction".into())
        })?;

        let v1c = add(v1, dv);
        let (r_arr2, _) = propagate(mu_p, &perturbers, r1, v1c, departure_et, tof);
        Ok(NBodyRefinement {
            miss_km: norm(miss),
            correction_km_s: dv,
            residual_km: norm(sub(r_target, r_arr2)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// A 1 AU circular orbit at 29.78 km/s, same as the lambert tests.
    const R: f64 = 149_597_870.7;
    const V: f64 = 29.78;
    const MU: f64 = V * V * R;

    fn circular_state(theta: f64) -> ([f64; 3], [f64; 3]) {
        let (s, c) = (theta.sin(), theta.cos());
        (
            [R * c, R * s, 0.0],
            [-V * s, V * c, 0.0],
        )
    }

    /// A perturber on the same circular orbit, sampled analytically.
    fn circular_perturber(gm: f64, t0: f64, h: f64, knots: usize) -> Perturber {
        let omega = V / R;
        let mut samples = Vec::with_capacity(knots + 1);
        for i in 0..=knots {
            samples.push(circular_state(omega * (t0 + h * i as f64)));
        }
        Perturber {
            gm,
            samples,
            t0,
            h,
        }
    }

    #[test]
    fn hermite_interpolation_error_scales_with_knot_spacing() {
        // Knots on a 1 AU circular orbit. The error scales as h^4:
        // ~0.3 km with daily knots, ~340 km with 10-day knots (the
        // measured value matches the |r''''| h^4 / 384 bound).
        let omega = V / R;
        for (days, tol) in [(1.0, 5.0), (10.0, 1000.0)] {
            let p = circular_perturber(1.0, 0.0, days * 86400.0, 32);
            let mut worst = 0.0f64;
            for i in 0..32 {
                let t = p.t0 + p.h * i as f64 + p.h / 2.0;
                let err = norm(sub(p.position_at(t), circular_state(omega * t).0));
                worst = worst.max(err);
            }
            assert!(worst < tol, "{days}-day knots: error {worst} km, tol {tol}");
        }
    }

    #[test]
    fn hermite_reproduces_linear_motion_exactly() {
        let samples: Vec<([f64; 3], [f64; 3])> = (0..=8)
            .map(|i| {
                let t = i as f64;
                ([10.0 * t, -5.0 * t, 2.0 * t], [10.0, -5.0, 2.0])
            })
            .collect();
        let p = Perturber {
            gm: 1.0,
            samples,
            t0: 0.0,
            h: 1.0,
        };
        for i in 0..16 {
            let t = i as f64 / 2.0;
            let got = p.position_at(t);
            let want = [10.0 * t, -5.0 * t, 2.0 * t];
            assert!(norm(sub(got, want)) < 1e-9);
        }
    }

    #[test]
    fn rk4_without_perturbers_matches_kepler_propagation() {
        // No perturbers: RK4 must agree with the universal-variable
        // Kepler propagator to well under 100 km over 40 days.
        let (r0, v0) = circular_state(0.3);
        let dt = 40.0 * 86400.0;
        let (r, _v) = propagate(MU, &[], r0, v0, 0.0, dt);
        let (want, _wv) = crate::lambert::propagate(r0, v0, dt, MU).unwrap();
        let err = norm(sub(r, want));
        assert!(err < 100.0, "RK4 vs Kepler: {err} km");
    }

    #[test]
    fn rk4_holds_a_circular_orbit() {
        // One full period: the propagated state returns to the start.
        let (r0, v0) = circular_state(0.0);
        let period = 2.0 * PI * (R * R * R / MU).sqrt();
        let (r, v) = propagate(MU, &[], r0, v0, 0.0, period);
        assert!(norm(sub(r, r0)) < 500.0, "position drift {}", norm(sub(r, r0)));
        assert!(norm(sub(v, v0)) < 0.001, "velocity drift {}", norm(sub(v, v0)));
    }

    #[test]
    fn solve3_recovers_a_known_solution() {
        // Columns of the identity scaled: x = b.
        let j = [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]];
        let b = [6.0, 9.0, 12.0];
        let x = solve3(&j, b).unwrap();
        assert!((x[0] - 3.0).abs() < 1e-12);
        assert!((x[1] - 3.0).abs() < 1e-12);
        assert!((x[2] - 3.0).abs() < 1e-12);
        // A general system: j * x = b with known x.
        let j2 = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 10.0]];
        let want = [1.0, -2.0, 0.5];
        let b2 = [
            j2[0][0] * want[0] + j2[1][0] * want[1] + j2[2][0] * want[2],
            j2[0][1] * want[0] + j2[1][1] * want[1] + j2[2][1] * want[2],
            j2[0][2] * want[0] + j2[1][2] * want[1] + j2[2][2] * want[2],
        ];
        let x2 = solve3(&j2, b2).unwrap();
        for i in 0..3 {
            assert!((x2[i] - want[i]).abs() < 1e-12);
        }
        // Singular system.
        let j3 = [[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [1.0, 1.0, 1.0]];
        assert!(solve3(&j3, [1.0, 1.0, 1.0]).is_none());
    }
}
