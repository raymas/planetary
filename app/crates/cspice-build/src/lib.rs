//! Raw FFI to the CSPICE library built by this crate's `build.rs`.
//!
//! Only the handful of routines the ephemeris core needs are declared. All
//! functions are unsafe; the safe wrapper lives in the `ephemeris` crate.
//!
//! CSPICE is process-global state (kernel pool, error status): it is NOT
//! thread-safe. Callers must serialize access.

#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]

use std::os::raw::{c_char, c_int};

pub type SpiceDouble = f64;
pub type SpiceInt = c_int;
pub type SpiceBoolean = c_int;
pub type SpiceChar = c_char;

pub const SPICETRUE: SpiceBoolean = 1;
pub const SPICEFALSE: SpiceBoolean = 0;

extern "C" {
    // Kernel management
    pub fn furnsh_c(file: *const c_char);
    pub fn kclear_c();
    pub fn ktotal_c(kind: *const c_char, count: *mut SpiceInt);

    // Error handling
    pub fn failed_c() -> SpiceBoolean;
    pub fn reset_c();
    pub fn erract_c(operation: *const c_char, lenout: SpiceInt, action: *mut c_char);
    pub fn errdev_c(operation: *const c_char, lenout: SpiceInt, device: *mut c_char);
    pub fn errprt_c(operation: *const c_char, lenout: SpiceInt, list: *mut c_char);
    pub fn getmsg_c(option: *const c_char, lenout: SpiceInt, msg: *mut c_char);

    // Time
    pub fn str2et_c(date: *const c_char, et: *mut SpiceDouble);
    pub fn et2utc_c(
        et: SpiceDouble,
        format: *const c_char,
        prec: SpiceInt,
        lenout: SpiceInt,
        utcstr: *mut c_char,
    );

    // Ephemeris
    pub fn spkpos_c(
        targ: *const c_char,
        et: SpiceDouble,
        frame: *const c_char,
        abcorr: *const c_char,
        obs: *const c_char,
        ptarg: *mut SpiceDouble,
        lt: *mut SpiceDouble,
    );
    pub fn spkezr_c(
        target: *const c_char,
        epoch: SpiceDouble,
        frame: *const c_char,
        abcorr: *const c_char,
        observer: *const c_char,
        state: *mut SpiceDouble,
        lt: *mut SpiceDouble,
    );

    // Body name <-> NAIF id
    pub fn bodn2c_c(name: *const c_char, code: *mut SpiceInt, found: *mut SpiceBoolean);

    // Body constants from the kernel pool (e.g. GM)
    pub fn bodvrd_c(
        body: *const c_char,
        item: *const c_char,
        maxn: SpiceInt,
        dim: *mut SpiceInt,
        values: *mut SpiceDouble,
    );

    // State -> conic orbital elements
    pub fn oscelt_c(
        state: *const SpiceDouble,
        et: SpiceDouble,
        mu: SpiceDouble,
        elts: *mut SpiceDouble,
    );
}

/// Maximum length of a CSPICE error message buffer.
pub const MAX_MSG_LEN: usize = 1841;

/// Reads a CSPICE output string buffer (null-terminated, null-padded) as UTF-8.
pub fn read_cstr(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}
