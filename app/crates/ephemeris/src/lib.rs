//! SPICE-backed ephemeris core.
//!
//! Wraps CSPICE to answer the questions the UI asks: where is a body at time
//! t, how fast is it moving, what does its orbit look like. Positions are
//! heliocentric, in the ECLIPJ2000 frame, in AU — the same convention as the
//! Python reference implementation, so the two can be cross-validated.
//! Moon orbit paths and orbital elements are the exception: they are
//! parent-relative, since a heliocentric moon ring would be a braided arc.
//!
//! CSPICE keeps process-global state (kernel pool, error status), so this
//! type is not thread-safe by itself; wrap it in a mutex at the boundary
//! (the Tauri app does).

mod catalog;
mod hohmann;
mod nbody;

pub mod lambert;

pub use catalog::{body_by_name, body_by_naif_id, Body, BODIES, SUN};
pub use hohmann::HohmannTransfer;

use std::ffi::CString;
use std::path::{Path, PathBuf};

use cspice_build::{
    bodvrd_c, erract_c, errdev_c, errprt_c, et2utc_c, failed_c, furnsh_c, getmsg_c, ktotal_c,
    oscelt_c, read_cstr, reset_c, spkezr_c, spkpos_c, str2et_c, MAX_MSG_LEN,
};

/// Kilometers per astronomical unit (IAU 2012 value, matches the Python
/// reference's 149597870700 m).
pub const KM_PER_AU: f64 = 149_597_870.7;

/// Seconds per day.
pub const SEC_PER_DAY: f64 = 86_400.0;

/// Reference frame for all positions: the J2000 ecliptic.
const FRAME: &str = "ECLIPJ2000";

/// Aberration correction: geometric positions, no light-time — the Python
/// reference computes geometric positions too.
const ABCORR: &str = "NONE";

#[derive(Debug, thiserror::Error)]
pub enum EphemerisError {
    #[error("SPICE error: {0}")]
    Spice(String),
    #[error("unknown body: {0}")]
    UnknownBody(String),
    #[error("kernel directory {0} does not exist or holds no kernels; run scripts/fetch_kernels.sh")]
    NoKernels(PathBuf),
    #[error("body {0} has no orbit path")]
    NoOrbit(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// Position and velocity of a body at an epoch.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct State {
    /// Heliocentric position in AU, ECLIPJ2000.
    pub position_au: [f64; 3],
    /// Heliocentric velocity in AU/day, ECLIPJ2000.
    pub velocity_au_per_day: [f64; 3],
}

/// Conic orbital elements at an epoch, from `oscelt`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct OrbitalElements {
    /// Semi-major axis in AU.
    pub semi_major_axis_au: f64,
    /// Eccentricity.
    pub eccentricity: f64,
    /// Inclination to the ecliptic in degrees.
    pub inclination_deg: f64,
    /// Longitude of the ascending node in degrees.
    pub ascending_node_deg: f64,
    /// Argument of perihelion in degrees.
    pub argument_of_perihelion_deg: f64,
    /// Mean anomaly at epoch in degrees.
    pub mean_anomaly_deg: f64,
    /// Orbital period in days.
    pub period_days: f64,
}

/// A loaded SPICE kernel set.
pub struct Ephemeris {
    kernel_dir: PathBuf,
}

impl Ephemeris {
    /// Loads every kernel file found in `kernel_dir` (typically
    /// `de440s.bsp`, `naif0012.tls`, `pck00011.tpc`).
    pub fn new(kernel_dir: impl AsRef<Path>) -> Result<Self, EphemerisError> {
        let dir = kernel_dir.as_ref().to_path_buf();
        if !dir.is_dir() {
            return Err(EphemerisError::NoKernels(dir));
        }

        unsafe { configure_error_handling() };

        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|_| EphemerisError::NoKernels(dir.clone()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        if files.is_empty() {
            return Err(EphemerisError::NoKernels(dir));
        }

        for file in &files {
            let path = cstring(file.to_str().ok_or_else(|| {
                EphemerisError::Invalid(format!("non-UTF-8 kernel path {}", file.display()))
            })?)?;
            unsafe { furnsh_c(path.as_ptr()) };
            unsafe { check() }?;
        }

        // Sanity check: the SPK pool must hold at least one kernel.
        let mut count = 0i32;
        let kind = cstring("SPK")?;
        unsafe { ktotal_c(kind.as_ptr(), &mut count) };
        unsafe { check() }?;
        if count < 1 {
            return Err(EphemerisError::NoKernels(dir));
        }

        Ok(Self { kernel_dir: dir })
    }

    /// The directory the kernels were loaded from.
    pub fn kernel_dir(&self) -> &Path {
        &self.kernel_dir
    }

    /// Converts a UTC or UTC-TDB string ("2024-01-01 00:00", "2000-01-01
    /// 12:00:00 TDB", ...) to ephemeris seconds past J2000.
    pub fn utc_to_et(&self, time: &str) -> Result<f64, EphemerisError> {
        let c = cstring(time)?;
        let mut et = 0.0;
        unsafe { str2et_c(c.as_ptr(), &mut et) };
        unsafe { check() }?;
        Ok(et)
    }

    /// Converts ephemeris seconds past J2000 to an ISO UTC string
    /// ("YYYY-MM-DDTHH:MM:SS.sss").
    pub fn et_to_utc(&self, et: f64) -> Result<String, EphemerisError> {
        let format = cstring("ISOC")?;
        let mut buf = vec![0u8; 64];
        unsafe {
            et2utc_c(et, format.as_ptr(), 3, (buf.len() - 1) as i32, buf.as_mut_ptr() as _);
        }
        unsafe { check() }?;
        Ok(read_cstr(&buf))
    }

    /// Heliocentric position of a body at `et`, in AU, ECLIPJ2000.
    pub fn position(&self, body: &Body, et: f64) -> Result<[f64; 3], EphemerisError> {
        self.position_relative(body, &SUN, et)
    }

    /// Position of `body` relative to `observer`, in AU, ECLIPJ2000.
    pub fn position_relative(
        &self,
        body: &Body,
        observer: &Body,
        et: f64,
    ) -> Result<[f64; 3], EphemerisError> {
        if body.naif_id == observer.naif_id {
            return Ok([0.0; 3]);
        }
        let targ = cstring(&body.naif_id.to_string())?;
        let obs = cstring(&observer.naif_id.to_string())?;
        let frame = cstring(FRAME)?;
        let abcorr = cstring(ABCORR)?;
        let mut pos = [0.0f64; 3];
        let mut lt = 0.0;
        unsafe {
            spkpos_c(
                targ.as_ptr(),
                et,
                frame.as_ptr(),
                abcorr.as_ptr(),
                obs.as_ptr(),
                pos.as_mut_ptr(),
                &mut lt,
            );
        }
        unsafe { check() }?;
        Ok([pos[0] / KM_PER_AU, pos[1] / KM_PER_AU, pos[2] / KM_PER_AU])
    }

    /// Heliocentric state (position in AU, velocity in AU/day) at `et`.
    pub fn state(&self, body: &Body, et: f64) -> Result<State, EphemerisError> {
        if body.naif_id == SUN.naif_id {
            return Ok(State {
                position_au: [0.0; 3],
                velocity_au_per_day: [0.0; 3],
            });
        }
        let targ = cstring(&body.naif_id.to_string())?;
        let obs = cstring(&SUN.naif_id.to_string())?;
        let frame = cstring(FRAME)?;
        let abcorr = cstring(ABCORR)?;
        let mut state = [0.0f64; 6];
        let mut lt = 0.0;
        unsafe {
            spkezr_c(
                targ.as_ptr(),
                et,
                frame.as_ptr(),
                abcorr.as_ptr(),
                obs.as_ptr(),
                state.as_mut_ptr(),
                &mut lt,
            );
        }
        unsafe { check() }?;
        Ok(State {
            position_au: [
                state[0] / KM_PER_AU,
                state[1] / KM_PER_AU,
                state[2] / KM_PER_AU,
            ],
            velocity_au_per_day: [
                state[3] / KM_PER_AU * SEC_PER_DAY,
                state[4] / KM_PER_AU * SEC_PER_DAY,
                state[5] / KM_PER_AU * SEC_PER_DAY,
            ],
        })
    }

    /// State of `body` relative to `observer` in km and km/s,
    /// ECLIPJ2000. The AU-based `state` is heliocentric only; transfer
    /// math needs parent-relative km/s velocities.
    pub fn state_relative_km(
        &self,
        body: &Body,
        observer: &Body,
        et: f64,
    ) -> Result<([f64; 3], [f64; 3]), EphemerisError> {
        if body.naif_id == observer.naif_id {
            return Ok(([0.0; 3], [0.0; 3]));
        }
        let targ = cstring(&body.naif_id.to_string())?;
        let obs = cstring(&observer.naif_id.to_string())?;
        let frame = cstring(FRAME)?;
        let abcorr = cstring(ABCORR)?;
        let mut state = [0.0f64; 6];
        let mut lt = 0.0;
        unsafe {
            spkezr_c(
                targ.as_ptr(),
                et,
                frame.as_ptr(),
                abcorr.as_ptr(),
                obs.as_ptr(),
                state.as_mut_ptr(),
                &mut lt,
            );
        }
        unsafe { check() }?;
        Ok((
            [state[0], state[1], state[2]],
            [state[3], state[4], state[5]],
        ))
    }

    /// Samples the body's orbit over one sidereal period centered on `et`.
    ///
    /// Planets get `samples` heliocentric positions in AU, ECLIPJ2000,
    /// forming a polyline around the Sun. Sample times are clamped to the
    /// de440s coverage window (1849-12-26 to 2150-01-22): outer-body
    /// trails that would reach past the kernel edge (Pluto's half-period
    /// is ~124 years) become partial arcs instead of errors. The Sun
    /// itself has no orbit path.
    ///
    /// Moons get positions relative to their parent body (a ring around
    /// it, not the Sun). Moon kernels have their own, shorter coverage
    /// windows (e.g. mar099s: 1995-2050), so sampling stops at the first
    /// out-of-span point and returns a partial (possibly empty) arc.
    pub fn orbit_path(
        &self,
        body: &Body,
        et: f64,
        samples: usize,
    ) -> Result<Vec<[f64; 3]>, EphemerisError> {
        if !body.is_orbiting() {
            return Err(EphemerisError::NoOrbit(body.name.to_owned()));
        }
        if samples < 2 {
            return Err(EphemerisError::Invalid("orbit_path needs >= 2 samples".into()));
        }
        if let Some(parent_name) = body.parent {
            let parent = body_by_name(parent_name)?;
            return self.moon_orbit_path(body, parent, et, samples);
        }
        let (et_min, et_max) = self.kernel_span()?;
        let period = body.orbital_period_days * SEC_PER_DAY;
        let start = (et - period / 2.0).max(et_min);
        let end = (et + period / 2.0).min(et_max);
        let step = (end - start) / (samples - 1) as f64;
        let mut path = Vec::with_capacity(samples);
        for i in 0..samples {
            path.push(self.position(body, start + i as f64 * step)?);
        }
        Ok(path)
    }

    /// Parent-relative orbit ring for a moon; truncates at the moon
    /// kernel's coverage edge instead of erroring.
    fn moon_orbit_path(
        &self,
        body: &Body,
        parent: &Body,
        et: f64,
        samples: usize,
    ) -> Result<Vec<[f64; 3]>, EphemerisError> {
        let period = body.orbital_period_days * SEC_PER_DAY;
        let start = et - period / 2.0;
        let step = period / (samples - 1) as f64;
        let mut path = Vec::with_capacity(samples);
        for i in 0..samples {
            match self.position_relative(body, parent, start + i as f64 * step) {
                Ok(p) => path.push(p),
                Err(_) => break, // outside the satellite kernel's span
            }
        }
        Ok(path)
    }

    /// The de440s coverage window in ET, converted once per call (two
    /// str2et round trips, microseconds). Dates from the kernel's own
    /// header: 26-DEC-1849 to 22-JAN-2150.
    fn kernel_span(&self) -> Result<(f64, f64), EphemerisError> {
        Ok((
            self.utc_to_et("1849-12-26 00:00:00")?,
            self.utc_to_et("2150-01-22 00:00:00")?,
        ))
    }

    /// Conic orbital elements of `body` at `et`: heliocentric for planets,
    /// parent-relative for moons.
    pub fn orbital_elements(
        &self,
        body: &Body,
        et: f64,
    ) -> Result<OrbitalElements, EphemerisError> {
        if !body.is_orbiting() {
            return Err(EphemerisError::NoOrbit(body.name.to_owned()));
        }
        // The two-body focus: the Sun for planets, the parent for moons.
        // mu = GM_primary + GM_secondary; moon GMs come from the catalog
        // mass because the loaded PCK does not carry all of them.
        let (obs_id, mu) = match body.parent {
            Some(parent_name) => {
                let parent = body_by_name(parent_name)?;
                (parent.naif_id, self.gm(parent)? + body.gm_from_mass_km3_s2())
            }
            None => (SUN.naif_id, self.gm(&SUN)? + self.gm(body)?),
        };
        // spkezr in km, km/s relative to the focus, as oscelt expects.
        let targ = cstring(&body.naif_id.to_string())?;
        let obs = cstring(&obs_id.to_string())?;
        let frame = cstring(FRAME)?;
        let abcorr = cstring(ABCORR)?;
        let mut km_state = [0.0f64; 6];
        let mut lt = 0.0;
        unsafe {
            spkezr_c(
                targ.as_ptr(),
                et,
                frame.as_ptr(),
                abcorr.as_ptr(),
                obs.as_ptr(),
                km_state.as_mut_ptr(),
                &mut lt,
            );
        }
        unsafe { check() }?;

        let mut elts = [0.0f64; 8];
        unsafe { oscelt_c(km_state.as_ptr(), et, mu, elts.as_mut_ptr()) };
        unsafe { check() }?;

        let q = elts[0]; // perifocal distance, km
        let e = elts[1];
        let a_km = if e < 1.0 { q / (1.0 - e) } else { f64::NAN };
        let period = if e < 1.0 {
            2.0 * std::f64::consts::PI * (a_km * a_km * a_km / mu).sqrt() / SEC_PER_DAY
        } else {
            f64::INFINITY
        };
        let deg = 180.0 / std::f64::consts::PI;
        Ok(OrbitalElements {
            semi_major_axis_au: a_km / KM_PER_AU,
            eccentricity: e,
            inclination_deg: elts[2] * deg,
            ascending_node_deg: elts[3] * deg,
            argument_of_perihelion_deg: elts[4] * deg,
            mean_anomaly_deg: elts[5] * deg,
            period_days: period,
        })
    }

    /// GM (km^3/s^2) of a body from the loaded PCK kernel pool.
    fn gm(&self, body: &Body) -> Result<f64, EphemerisError> {
        let name = cstring(body.spice_name())?;
        let item = cstring("GM")?;
        let mut dim = 0i32;
        let mut value = 0.0f64;
        unsafe {
            bodvrd_c(name.as_ptr(), item.as_ptr(), 1, &mut dim, &mut value);
        }
        unsafe { check() }?;
        Ok(value)
    }
}

/// Puts CSPICE into "return, don't abort" error mode and silences its own
/// reporting; errors surface through `check()`.
unsafe fn configure_error_handling() {
    let set = cstring("SET").unwrap();
    let ret = cstring("RETURN").unwrap();
    let null = cstring("NULL").unwrap();
    let none = cstring("NONE").unwrap();
    erract_c(set.as_ptr(), 0, ret.as_ptr() as _);
    errdev_c(set.as_ptr(), 0, null.as_ptr() as _);
    errprt_c(set.as_ptr(), 0, none.as_ptr() as _);
    reset_c();
}

/// If CSPICE flagged an error, drain the message, reset the status and return
/// it; otherwise Ok.
unsafe fn check() -> Result<(), EphemerisError> {
    if failed_c() != 0 {
        let option = cstring("LONG").unwrap();
        let mut buf = vec![0u8; MAX_MSG_LEN];
        getmsg_c(option.as_ptr(), (buf.len() - 1) as i32, buf.as_mut_ptr() as _);
        let msg = read_cstr(&buf);
        reset_c();
        Err(EphemerisError::Spice(msg))
    } else {
        Ok(())
    }
}

fn cstring(s: &str) -> Result<CString, EphemerisError> {
    CString::new(s).map_err(|_| EphemerisError::Invalid(format!("interior NUL in {s:?}")))
}
