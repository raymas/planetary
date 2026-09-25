//! Tauri app: commands over the ephemeris core, kernel management.
//!
//! The frontend owns the simulation clock and asks for positions at the
//! current sim time each frame; SPICE queries are microseconds, so no
//! queueing is needed (the Python prototype's multiprocessing machinery
//! disappears).

use ephemeris::{
    body_by_name, Ephemeris, HohmannTransfer, OrbitalElements, State as BodyState, BODIES,
};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, State};

/// CSPICE is process-global; all commands go through one mutex-guarded
/// handle. The ephemeris loads on a background thread (the Android
/// first launch extracts ~2.4 GB of kernels from the APK), so
/// commands may arrive before it is up.
enum EphemerisState {
    Loading,
    Ready(Ephemeris),
    Failed(String),
}

struct App {
    eph: Mutex<EphemerisState>,
}

impl App {
    fn with_eph<T>(
        &self,
        f: impl FnOnce(&Ephemeris) -> Result<T, ephemeris::EphemerisError>,
    ) -> Result<T, String> {
        let guard = self.eph.lock().map_err(|_| "state poisoned".to_owned())?;
        match &*guard {
            EphemerisState::Ready(eph) => f(eph).map_err(|e| e.to_string()),
            EphemerisState::Loading => Err("kernels are still loading".to_owned()),
            EphemerisState::Failed(e) => Err(e.clone()),
        }
    }
}

/// Reports backend readiness so the UI can show a loading state on
/// the first launch instead of a black screen.
#[tauri::command]
fn kernel_status(state: State<App>) -> Result<String, String> {
    let guard = state.eph.lock().map_err(|_| "state poisoned".to_owned())?;
    Ok(match &*guard {
        EphemerisState::Loading => "loading".to_owned(),
        EphemerisState::Ready(_) => "ready".to_owned(),
        EphemerisState::Failed(e) => format!("error: {e}"),
    })
}

#[derive(serde::Serialize)]
struct BodyPosition {
    name: &'static str,
    /// Heliocentric position in AU, ECLIPJ2000.
    position_au: [f64; 3],
}

#[derive(serde::Serialize)]
struct BodyStateInfo {
    name: &'static str,
    display_name: &'static str,
    /// Heliocentric position in AU, ECLIPJ2000.
    position_au: [f64; 3],
    /// Heliocentric velocity in AU/day.
    velocity_au_per_day: [f64; 3],
    /// Distance from the Sun in AU.
    distance_sun_au: f64,
    /// Distance from Earth in AU.
    distance_earth_au: f64,
    /// Conic elements at the epoch, if the body orbits the Sun.
    elements: Option<OrbitalElements>,
}

/// Where the kernels live: env override, then the repo checkout (dev),
/// then the bundled resources (installed apps).
fn kernel_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("PLANETARY_KERNELS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    // Dev builds run from the repo: app/src-tauri -> repo root.
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../kernels");
    if dev.is_dir() {
        return Ok(dev);
    }
    #[cfg(target_os = "android")]
    {
        // The APK asset archive is not a filesystem directory, and
        // CSPICE needs real files: extract the bundled kernels to the
        // app data dir on first launch.
        prepare_android_kernels(app)
    }
    #[cfg(not(target_os = "android"))]
    {
        let resource = app
            .path()
            .resource_dir()
            .map_err(|e| format!("resource dir: {e}"))?
            .join("kernels");
        Ok(resource)
    }
}

/// Kernel filenames bundled into the Android APK. Keep in sync with
/// the `bundle.resources` list in `tauri.conf.json`.
#[cfg(target_os = "android")]
const ANDROID_KERNELS: &[&str] = &[
    "de440s.bsp",
    "gm_de431.tpc",
    "jup365.bsp",
    "mar099s.bsp",
    "naif0012.tls",
    "nep097.bsp",
    "pck00011.tpc",
    "plu060.bsp",
    "sat441.bsp",
    "ura184_part-3.bsp",
];

/// Extracts the bundled kernels from the APK into the app's data
/// directory. Files already extracted are skipped, and each file is
/// written under a `.part` name and renamed, so an interrupted first
/// launch never leaves a truncated kernel behind.
#[cfg(target_os = "android")]
fn prepare_android_kernels(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?
        .join("kernels");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    let apk = android_apk_path()?;
    let file = std::fs::File::open(&apk).map_err(|e| format!("open APK {apk}: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("read APK: {e}"))?;

    for name in ANDROID_KERNELS {
        let dest = dir.join(name);
        if dest.is_file() {
            continue;
        }
        let entry_path = format!("assets/kernels/{name}");
        let mut entry = archive
            .by_name(&entry_path)
            .map_err(|e| format!("APK entry {entry_path}: {e}"))?;
        let tmp = dir.join(format!("{name}.part"));
        let mut out =
            std::fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("extract {name}: {e}"))?;
        drop(out);
        std::fs::rename(&tmp, &dest).map_err(|e| format!("finalize {name}: {e}"))?;
    }
    Ok(dir)
}

/// The running app's own APK path, from the process memory map. The
/// APK is always mapped for its resources, so this avoids JNI.
#[cfg(target_os = "android")]
fn android_apk_path() -> Result<String, String> {
    let maps =
        std::fs::read_to_string("/proc/self/maps").map_err(|e| format!("/proc/self/maps: {e}"))?;
    let mut first: Option<String> = None;
    for path in maps.lines().filter_map(|l| l.rsplit_once(' ').map(|(_, p)| p)) {
        if !path.ends_with(".apk") {
            continue;
        }
        // Prefer our own package's APK over anything else mapped.
        if path.contains("planetarytransfer") {
            return Ok(path.to_owned());
        }
        first.get_or_insert_with(|| path.to_owned());
    }
    first.ok_or_else(|| "no APK found in /proc/self/maps".to_owned())
}

fn init_ephemeris(app: &tauri::AppHandle) -> Result<Ephemeris, String> {
    let dir = kernel_dir(app)?;
    Ephemeris::new(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_bodies() -> Vec<ephemeris::Body> {
    BODIES.to_vec()
}

#[tauri::command]
fn get_positions(et: f64, state: State<App>) -> Result<Vec<BodyPosition>, String> {
    state.with_eph(|eph| {
        let mut out = Vec::with_capacity(BODIES.len());
        for body in BODIES {
            // A moon outside its satellite kernel's coverage (e.g. Phobos
            // before 1995) is skipped, not fatal: the UI hides absent bodies.
            if let Ok(position_au) = eph.position(body, et) {
                out.push(BodyPosition {
                    name: body.name,
                    position_au,
                });
            }
        }
        Ok(out)
    })
}

#[tauri::command]
fn get_state(body: String, et: f64, state: State<App>) -> Result<BodyStateInfo, String> {
    state.with_eph(|eph| {
        let body = body_by_name(&body)?;
        let BodyState {
            position_au,
            velocity_au_per_day,
        } = eph.state(body, et)?;
        let distance_sun_au =
            (position_au[0] * position_au[0] + position_au[1] * position_au[1] + position_au[2] * position_au[2]).sqrt();
        let rel = {
            let earth = body_by_name("earth")?;
            eph.position_relative(body, earth, et)?
        };
        let distance_earth_au =
            (rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2]).sqrt();
        let elements = if body.is_orbiting() {
            Some(eph.orbital_elements(body, et)?)
        } else {
            None
        };
        Ok(BodyStateInfo {
            name: body.name,
            display_name: body.display_name,
            position_au,
            velocity_au_per_day,
            distance_sun_au,
            distance_earth_au,
            elements,
        })
    })
}

#[tauri::command]
fn get_orbit_path(
    body: String,
    et: f64,
    samples: u32,
    state: State<App>,
) -> Result<Vec<[f64; 3]>, String> {
    state.with_eph(|eph| {
        let body = body_by_name(&body)?;
        Ok(eph.orbit_path(body, et, samples.max(2) as usize)?)
    })
}

#[derive(serde::Serialize)]
struct HohmannInfo {
    /// The transfer numbers (dV, timing, window).
    #[serde(flatten)]
    transfer: HohmannTransfer,
    /// Catalog name of the primary both bodies orbit ("sun" for planets).
    primary: &'static str,
    /// The idealized transfer ellipse, parent-relative AU, ECLIPJ2000,
    /// departure point first.
    path_au: Vec<[f64; 3]>,
}

#[tauri::command]
fn get_hohmann_transfer(
    from: String,
    to: String,
    et: f64,
    departure_alt_m: f64,
    arrival_alt_m: f64,
    state: State<App>,
) -> Result<HohmannInfo, String> {
    state.with_eph(|eph| {
        let from = body_by_name(&from)?;
        let to = body_by_name(&to)?;
        let (transfer, path_au) =
            eph.hohmann_transfer(from, to, et, departure_alt_m, arrival_alt_m)?;
        Ok(HohmannInfo {
            transfer,
            primary: from.parent.unwrap_or("sun"),
            path_au,
        })
    })
}

#[tauri::command]
fn utc_to_et(utc: String, state: State<App>) -> Result<f64, String> {
    state.with_eph(|eph| eph.utc_to_et(&utc))
}

#[tauri::command]
fn et_to_utc(et: f64, state: State<App>) -> Result<String, String> {
    state.with_eph(|eph| eph.et_to_utc(et))
}

#[tauri::command]
fn get_kernel_dir(app: tauri::AppHandle) -> Result<String, String> {
    kernel_dir(&app).map(|p| p.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(App {
                eph: Mutex::new(EphemerisState::Loading),
            });
            // Kernel loading (and, on Android, the one-time extraction
            // from the APK) can take minutes; keep it off the main
            // thread so the window appears immediately.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let state = match init_ephemeris(&handle) {
                    Ok(eph) => EphemerisState::Ready(eph),
                    Err(e) => {
                        eprintln!("kernel init failed: {e}");
                        EphemerisState::Failed(e)
                    }
                };
                let app_state: State<App> = handle.state();
                let mut guard = app_state.eph.lock().unwrap_or_else(|p| p.into_inner());
                *guard = state;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bodies,
            get_positions,
            get_state,
            get_orbit_path,
            get_hohmann_transfer,
            utc_to_et,
            et_to_utc,
            get_kernel_dir,
            kernel_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
