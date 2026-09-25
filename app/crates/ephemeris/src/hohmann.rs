//! Transfer between two bodies orbiting the same primary (the Sun for
//! planets, the parent planet for moons).
//!
//! The transfer is a Lambert arc through the bodies' real SPICE
//! positions and velocities, seeded from the idealized Hohmann
//! ellipse: the departure window is scanned across one synodic period
//! and the time of flight across a grid around the Hohmann value (a
//! coarse porkchop), keeping the cheapest total delta-v. Because the
//! arc connects actual states — not circular, coplanar approximations
//! — orbital eccentricity, inclination and the true phase angle all
//! enter the delta-v.
//!
//! Burns are impulsive and patched-conic: departure from a circular
//! parking orbit around the departure body, capture into a circular
//! parking orbit around the arrival body. GM values come from the
//! loaded PCK (primary) and the catalog masses (departure/arrival
//! bodies).
//!
//! The result is a planning aid, not a flight plan: two-body dynamics
//! only (no perturbations, no finite-burn or gravity losses), and the
//! porkchop grid is coarse — a mission design tool would refine it.

use std::f64::consts::PI;

use crate::lambert::{self, cross, dot, norm, normalize, scale, sub};
use crate::{body_by_name, Body, Ephemeris, EphemerisError, KM_PER_AU, SUN};

/// Coarse departure-window scan samples across one synodic period.
const SCAN_SAMPLES: usize = 160;
/// Coarse time-of-flight grid, as multiples of the Hohmann time of
/// flight. The optimum typically sits between 0.9 and 1.3.
const TOF_MULTS: [f64; 8] = [0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.4];
/// Refinement grid around the coarse minimum: departure samples and
/// time-of-flight samples (each axis spans one coarse grid step).
const REFINE_DEPARTURES: usize = 20;
const REFINE_TOFS: usize = 10;

/// A computed transfer (Hohmann-seeded Lambert arc).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct HohmannTransfer {
    /// Departure burn from the parking orbit, m/s.
    pub dv_departure_m_s: f64,
    /// Arrival capture burn into the parking orbit, m/s.
    pub dv_arrival_m_s: f64,
    /// Total delta-v, m/s.
    pub dv_total_m_s: f64,
    /// Time of flight on the transfer arc, seconds.
    pub transfer_time_s: f64,
    /// Phase angle of the arrival body relative to the departure body
    /// at the departure epoch, degrees (negative = trailing).
    pub phase_angle_deg: f64,
    /// Synodic period: how often launch windows recur, seconds.
    pub synodic_period_s: f64,
    /// Best departure epoch found in [et, et + synodic period], TDB
    /// seconds past J2000.
    pub departure_et: f64,
    /// Arrival epoch (departure + time of flight).
    pub arrival_et: f64,
    /// Hyperbolic excess speed relative to the departure body, m/s.
    pub v_inf_departure_m_s: f64,
    /// Hyperbolic excess speed relative to the arrival body, m/s.
    pub v_inf_arrival_m_s: f64,
    /// Inclination of the transfer plane to the ecliptic, degrees
    /// (over 90 for retrograde transfers).
    pub transfer_inclination_deg: f64,
    /// Hyperbolic excess velocity at departure, parent-relative
    /// km/s, ECLIPJ2000 (Lambert velocity minus the body's velocity).
    pub v_inf_departure_km_s: [f64; 3],
    /// Hyperbolic excess velocity at arrival, parent-relative
    /// km/s, ECLIPJ2000.
    pub v_inf_arrival_km_s: [f64; 3],
    /// Angle between the departure asymptote and the departure
    /// body's orbital plane, degrees, signed (positive = north of
    /// it). The parking orbit plane must contain the v_inf vector
    /// for the quoted dV; this is the tilt that requires.
    pub eject_declination_deg: f64,
    /// Same for the arrival asymptote and the arrival body's
    /// orbital plane, degrees.
    pub insert_declination_deg: f64,
    /// Angle between the parking-orbit burn point and the departure
    /// asymptote, degrees: the hyperbolic asymptote true anomaly
    /// acos(-1/e). The burn happens this far "before" the asymptote
    /// direction, at the escape hyperbola's periapsis.
    pub eject_burn_angle_deg: f64,
    /// Same for the arrival capture burn, degrees.
    pub insert_burn_angle_deg: f64,
    /// Arrival miss distance if the Lambert arc is flown as-is under
    /// the gravity of the other planets (or sibling moons), km.
    /// None when the perturber ephemerides are unavailable.
    pub nbody_miss_km: Option<f64>,
    /// Departure velocity correction that cancels the n-body miss,
    /// m/s — fold it into the eject burn or spend it mid-course.
    /// None alongside `nbody_miss_km`.
    pub nbody_correction_m_s: Option<f64>,
    /// Remaining miss after applying the correction, km.
    pub nbody_residual_km: Option<f64>,
}

/// One candidate departure epoch, fully evaluated.
struct Candidate {
    departure_et: f64,
    /// Time of flight, seconds.
    tof: f64,
    /// Departure body position, parent-relative km.
    r1: [f64; 3],
    /// Arrival body position at arrival, parent-relative km.
    r2: [f64; 3],
    /// Lambert departure velocity, parent-relative km/s.
    v1: [f64; 3],
    /// Lambert arrival velocity, parent-relative km/s.
    v2: [f64; 3],
    /// Departure body velocity, parent-relative km/s.
    v1_body: [f64; 3],
    /// Arrival body velocity at arrival, parent-relative km/s.
    v2_body: [f64; 3],
    /// Hyperbolic excess speeds, parent-relative km/s.
    v_inf_dep: f64,
    v_inf_arr: f64,
    /// Parking-orbit burns, km/s.
    dv_dep: f64,
    dv_arr: f64,
}

impl Candidate {
    fn dv_total(&self) -> f64 {
        self.dv_dep + self.dv_arr
    }
}

impl Ephemeris {
    /// Computes the transfer `from` -> `to` at epoch `et`.
    ///
    /// `departure_alt_m` / `arrival_alt_m` are the parking orbit
    /// altitudes above the bodies' surfaces, in meters. The transfer
    /// arc (parent-relative, AU, ECLIPJ2000, departure -> arrival) is
    /// returned separately in `path_au`.
    pub fn hohmann_transfer(
        &self,
        from: &Body,
        to: &Body,
        et: f64,
        departure_alt_m: f64,
        arrival_alt_m: f64,
    ) -> Result<(HohmannTransfer, Vec<[f64; 3]>), EphemerisError> {
        if from.name == to.name {
            return Err(EphemerisError::Invalid(
                "transfer needs two different bodies".into(),
            ));
        }
        if !from.is_orbiting() || !to.is_orbiting() {
            return Err(EphemerisError::Invalid(format!(
                "{} and {} must both orbit a primary",
                from.name, to.name
            )));
        }
        if from.parent != to.parent {
            return Err(EphemerisError::Invalid(format!(
                "{} and {} do not share a primary ({} vs {})",
                from.name,
                to.name,
                from.parent.unwrap_or("the Sun"),
                to.parent.unwrap_or("the Sun")
            )));
        }

        // The transfer primary: the Sun for planets, the parent for moons.
        let primary: &Body = match from.parent {
            Some(name) => body_by_name(name)?,
            None => &SUN,
        };
        let mu_p = self.gm(primary)?; // km^3/s^2
        let mu_from = from.gm_from_mass_km3_s2();
        let mu_to = to.gm_from_mass_km3_s2();

        // The Hohmann seed: the idealized transfer ellipse between the
        // semi-major axes gives the time-of-flight grid and the synodic
        // period sets the departure scan.
        let r1 = self.orbital_elements(from, et)?.semi_major_axis_au * KM_PER_AU;
        let r2 = self.orbital_elements(to, et)?.semi_major_axis_au * KM_PER_AU;
        if !(r1.is_finite() && r2.is_finite()) {
            return Err(EphemerisError::Invalid(
                "both orbits must be elliptic for a Hohmann transfer".into(),
            ));
        }
        if (r1 - r2).abs() / (r1 + r2) < 1e-9 {
            return Err(EphemerisError::Invalid(
                "orbits are too similar for a Hohmann transfer".into(),
            ));
        }
        let a_t = (r1 + r2) / 2.0;
        let hohmann_tof = PI * (a_t * a_t * a_t / mu_p).sqrt();
        let n1 = (mu_p / (r1 * r1 * r1)).sqrt();
        let n2 = (mu_p / (r2 * r2 * r2)).sqrt();
        let rate = n2 - n1;
        if rate.abs() < 1e-12 {
            return Err(EphemerisError::Invalid(
                "orbits are too similar for a launch window".into(),
            ));
        }
        let synodic = 2.0 * PI / rate.abs();

        // Parking-orbit radii for the patched-conic burns.
        let rp1 = from.radius_m() / 1000.0 + departure_alt_m / 1000.0;
        let rp2 = to.radius_m() / 1000.0 + arrival_alt_m / 1000.0;

        // Coarse porkchop: departure epochs across one synodic period,
        // times of flight around the Hohmann value. Epochs outside
        // kernel coverage are skipped, not fatal.
        let step = synodic / SCAN_SAMPLES as f64;
        let mut best: Option<Candidate> = None;
        for i in 0..=SCAN_SAMPLES {
            let t = et + step * i as f64;
            for &m in TOF_MULTS.iter() {
                let tof = hohmann_tof * m;
                if let Ok(c) =
                    self.eval_candidate(from, to, primary, mu_p, mu_from, mu_to, rp1, rp2, t, tof)
                {
                    if best.as_ref().is_none_or(|b| c.dv_total() < b.dv_total()) {
                        best = Some(c);
                    }
                }
            }
        }
        // Refine around the coarse minimum, one grid step on each axis.
        if let Some(b) = &best {
            let dep_lo = (b.departure_et - step).max(et);
            let tof_lo = (b.tof - 0.05 * hohmann_tof).max(0.1 * hohmann_tof);
            for i in 0..=REFINE_DEPARTURES {
                let t = dep_lo + 2.0 * step * i as f64 / REFINE_DEPARTURES as f64;
                for j in 0..=REFINE_TOFS {
                    let tof = tof_lo + 0.1 * hohmann_tof * j as f64 / REFINE_TOFS as f64;
                    if let Ok(c) = self.eval_candidate(
                        from, to, primary, mu_p, mu_from, mu_to, rp1, rp2, t, tof,
                    ) {
                        if best.as_ref().is_none_or(|b| c.dv_total() < b.dv_total()) {
                            best = Some(c);
                        }
                    }
                }
            }
        }
        let best = best.ok_or_else(|| {
            EphemerisError::Invalid(
                "no departure window inside the kernel coverage; pick an epoch closer to the present".into(),
            )
        })?;

        // Phase angle at the chosen departure, from the real geometry:
        // the angle from the departure body to the arrival body,
        // measured around the departure body's orbital angular momentum.
        let r_to_dep = self.state_relative_km(to, primary, best.departure_et)?.0;
        let n_hat = normalize(cross(best.r1, best.v1_body))?;
        let phase = dot(cross(best.r1, r_to_dep), n_hat).atan2(dot(best.r1, r_to_dep));

        // Inclination of the transfer plane to the ecliptic.
        let h_t = cross(best.r1, best.v1);
        let inc = (h_t[2] / norm(h_t)).clamp(-1.0, 1.0).acos().to_degrees();

        // Eject/insert geometry: the hyperbolic excess vectors, their
        // tilt out of the bodies' orbital planes, and where in the
        // parking orbits the burns happen (the escape/capture
        // hyperbola's asymptote true anomaly).
        let v_inf_dep_vec = sub(best.v1, best.v1_body);
        let v_inf_arr_vec = sub(best.v2, best.v2_body);
        let h_from = normalize(cross(best.r1, best.v1_body))?;
        let h_to = normalize(cross(best.r2, best.v2_body))?;
        let decl_dep = dot(scale(v_inf_dep_vec, 1.0 / norm(v_inf_dep_vec)), h_from)
            .clamp(-1.0, 1.0)
            .asin()
            .to_degrees();
        let decl_arr = dot(scale(v_inf_arr_vec, 1.0 / norm(v_inf_arr_vec)), h_to)
            .clamp(-1.0, 1.0)
            .asin()
            .to_degrees();
        let e_dep = 1.0 + rp1 * best.v_inf_dep * best.v_inf_dep / mu_from;
        let e_arr = 1.0 + rp2 * best.v_inf_arr * best.v_inf_arr / mu_to;
        let burn_dep = (-1.0 / e_dep).clamp(-1.0, 1.0).acos().to_degrees();
        let burn_arr = (-1.0 / e_arr).clamp(-1.0, 1.0).acos().to_degrees();

        // N-body check of the Lambert arc: the other planets' (or
        // sibling moons') pull bends it off target; measure the miss
        // and the correction that cancels it. Unavailable ephemerides
        // degrade to None, not an error.
        let nbody = self
            .nbody_refinement(
                from,
                to,
                primary,
                best.r1,
                best.v1,
                best.departure_et,
                best.tof,
            )
            .ok();

        let transfer = HohmannTransfer {
            dv_departure_m_s: best.dv_dep * 1000.0,
            dv_arrival_m_s: best.dv_arr * 1000.0,
            dv_total_m_s: best.dv_total() * 1000.0,
            transfer_time_s: best.tof,
            phase_angle_deg: phase.to_degrees(),
            synodic_period_s: synodic,
            departure_et: best.departure_et,
            arrival_et: best.departure_et + best.tof,
            v_inf_departure_m_s: best.v_inf_dep * 1000.0,
            v_inf_arrival_m_s: best.v_inf_arr * 1000.0,
            transfer_inclination_deg: inc,
            v_inf_departure_km_s: v_inf_dep_vec,
            v_inf_arrival_km_s: v_inf_arr_vec,
            eject_declination_deg: decl_dep,
            insert_declination_deg: decl_arr,
            eject_burn_angle_deg: burn_dep,
            insert_burn_angle_deg: burn_arr,
            nbody_miss_km: nbody.map(|n| n.miss_km),
            nbody_correction_m_s: nbody.map(|n| norm(n.correction_km_s) * 1000.0),
            nbody_residual_km: nbody.map(|n| n.residual_km),
        };

        let path = transfer_path(best.r1, best.v1, best.tof, mu_p)?;
        Ok((transfer, path))
    }

    /// Evaluates one departure epoch: Lambert arc from the departure
    /// body's real position to the arrival body's real position at
    /// `t + tof`, with the patched-conic parking-orbit burns.
    #[allow(clippy::too_many_arguments)]
    fn eval_candidate(
        &self,
        from: &Body,
        to: &Body,
        primary: &Body,
        mu_p: f64,
        mu_from: f64,
        mu_to: f64,
        rp1: f64,
        rp2: f64,
        t: f64,
        tof: f64,
    ) -> Result<Candidate, EphemerisError> {
        let (r1, v1_body) = self.state_relative_km(from, primary, t)?;
        let (r2, v2_body) = self.state_relative_km(to, primary, t + tof)?;
        // Anchor the transfer plane to the departure orbit: the arc
        // leaves along the departure body's motion, whatever its
        // inclination (or retrograde direction).
        let n_hat = normalize(cross(r1, v1_body))?;
        let (v1, v2) = lambert::solve(r1, r2, tof, mu_p, n_hat)?;
        let v_inf_dep = norm(sub(v1, v1_body));
        let v_inf_arr = norm(sub(v2, v2_body));
        let dv_dep =
            (v_inf_dep * v_inf_dep + 2.0 * mu_from / rp1).sqrt() - (mu_from / rp1).sqrt();
        let dv_arr = (v_inf_arr * v_inf_arr + 2.0 * mu_to / rp2).sqrt() - (mu_to / rp2).sqrt();
        Ok(Candidate {
            departure_et: t,
            tof,
            r1,
            r2,
            v1,
            v2,
            v1_body,
            v2_body,
            v_inf_dep,
            v_inf_arr,
            dv_dep,
            dv_arr,
        })
    }
}

/// Samples the Lambert transfer arc by propagating the departure state,
/// parent-relative AU, ECLIPJ2000 (129 samples, departure first).
fn transfer_path(
    r1: [f64; 3],
    v1: [f64; 3],
    tof: f64,
    mu_p: f64,
) -> Result<Vec<[f64; 3]>, EphemerisError> {
    let samples = 128;
    let mut path = Vec::with_capacity(samples + 1);
    path.push([
        r1[0] / KM_PER_AU,
        r1[1] / KM_PER_AU,
        r1[2] / KM_PER_AU,
    ]);
    for i in 1..=samples {
        let (r, _) = lambert::propagate(r1, v1, tof * i as f64 / samples as f64, mu_p)?;
        path.push([r[0] / KM_PER_AU, r[1] / KM_PER_AU, r[2] / KM_PER_AU]);
    }
    Ok(path)
}
