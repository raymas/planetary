//! Body catalog: the Sun, the eight planets, Pluto, and the major natural
//! satellites.
//!
//! Physical data (mass, diameter, color) is ported from the Python reference
//! `res/objects.yaml`. NAIF ids and sidereal orbital periods are added for
//! SPICE queries and orbit-path sampling.
//!
//! The SPICE target ids are the ones `de440s.bsp` actually contains: the
//! planetary *barycenters* (1-9) rather than planet centers (199-999). The
//! barycenter-to-planet-center offset is negligible at AU scale (largest is
//! Pluto's, ~1.3e-5 AU). Earth (399) and the Sun (10) are present directly.
//!
//! Moons carry a `parent` and come from the satellite SPK kernels listed in
//! `scripts/fetch_kernels.sh`. Their ephemerides are planet-barycenter-
//! relative and only cover each kernel's time span (e.g. mar099s holds
//! Phobos/Deimos for 1995-2050 only); queries outside the span fail and
//! callers must tolerate the absence.

use crate::EphemerisError;
use serde::Serialize;

/// Gravitational constant, m^3 kg^-1 s^-2 (CODATA 2018).
const G_SI: f64 = 6.67430e-11;

/// One solar-system body.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Body {
    /// Lowercase name, same keys as `res/objects.yaml`.
    pub name: &'static str,
    /// Display name.
    pub display_name: &'static str,
    /// NAIF id used as the SPICE query target (see the module docs).
    pub naif_id: i32,
    /// Mass in kilograms.
    pub mass_kg: f64,
    /// Equatorial diameter in meters.
    pub diameter_m: f64,
    /// Render color as 8-bit RGB, from the Python reference.
    pub color: [u8; 3],
    /// Sidereal orbital period in days (0 for the Sun).
    pub orbital_period_days: f64,
    /// Catalog name of the parent body for natural satellites, else None.
    pub parent: Option<&'static str>,
}

impl Body {
    /// Mean radius in meters.
    pub fn radius_m(&self) -> f64 {
        self.diameter_m / 2.0
    }

    /// True if the body orbits the Sun or a planet (i.e. has an orbit path).
    pub fn is_orbiting(&self) -> bool {
        self.orbital_period_days > 0.0
    }

    /// GM in km^3/s^2 derived from the catalog mass. Used for moons, whose
    /// GM the loaded PCK may not carry (it lacks e.g. Styx).
    pub fn gm_from_mass_km3_s2(&self) -> f64 {
        G_SI * self.mass_kg / 1e9
    }

    /// Name SPICE's name-to-id translation understands (bodn2c/bodvrd).
    /// Matches the query id: barycenter names for the planets.
    pub fn spice_name(&self) -> &'static str {
        match self.naif_id {
            10 => "SUN",
            399 => "EARTH",
            1 => "MERCURY BARYCENTER",
            2 => "VENUS BARYCENTER",
            4 => "MARS BARYCENTER",
            5 => "JUPITER BARYCENTER",
            6 => "SATURN BARYCENTER",
            7 => "URANUS BARYCENTER",
            8 => "NEPTUNE BARYCENTER",
            9 => "PLUTO BARYCENTER",
            301 => "MOON",
            401 => "PHOBOS",
            402 => "DEIMOS",
            501 => "IO",
            502 => "EUROPA",
            503 => "GANYMEDE",
            504 => "CALLISTO",
            601 => "MIMAS",
            602 => "ENCELADUS",
            603 => "TETHYS",
            604 => "DIONE",
            605 => "RHEA",
            606 => "TITAN",
            607 => "HYPERION",
            608 => "IAPETUS",
            609 => "PHOEBE",
            701 => "ARIEL",
            702 => "UMBRIEL",
            703 => "TITANIA",
            704 => "OBERON",
            705 => "MIRANDA",
            801 => "TRITON",
            901 => "CHARON",
            902 => "NIX",
            903 => "HYDRA",
            904 => "KERBEROS",
            905 => "STYX",
            _ => "SUN",
        }
    }
}

/// The Sun.
pub const SUN: Body = Body {
    name: "sun",
    display_name: "Sun",
    naif_id: 10,
    mass_kg: 1988500e24,
    diameter_m: 1392684e3,
    color: [238, 207, 1],
    orbital_period_days: 0.0,
    parent: None,
};

/// All bodies, Sun first, then outward from Mercury; each planet is followed
/// by its satellites, innermost first.
pub const BODIES: &[Body] = &[
    SUN,
    Body {
        name: "mercury",
        display_name: "Mercury",
        naif_id: 1,
        mass_kg: 0.33010e24,
        diameter_m: 4879e3,
        color: [173, 168, 165],
        orbital_period_days: 87.969,
        parent: None,
    },
    Body {
        name: "venus",
        display_name: "Venus",
        naif_id: 2,
        mass_kg: 4.8673e24,
        diameter_m: 12104e3,
        color: [227, 158, 28],
        orbital_period_days: 224.701,
        parent: None,
    },
    Body {
        name: "earth",
        display_name: "Earth",
        naif_id: 399,
        mass_kg: 5.9722e24,
        diameter_m: 12756e3,
        color: [0, 0, 255],
        orbital_period_days: 365.256,
        parent: None,
    },
    Body {
        name: "moon",
        display_name: "Moon",
        naif_id: 301,
        mass_kg: 0.07346e24,
        diameter_m: 3475e3,
        color: [169, 165, 161],
        orbital_period_days: 27.3217,
        parent: Some("earth"),
    },
    Body {
        name: "mars",
        display_name: "Mars",
        naif_id: 4,
        mass_kg: 0.64169e24,
        diameter_m: 6792e3,
        color: [231, 125, 17],
        orbital_period_days: 686.980,
        parent: None,
    },
    Body {
        name: "phobos",
        display_name: "Phobos",
        naif_id: 401,
        mass_kg: 1.0659e16,
        diameter_m: 22.2e3,
        color: [148, 140, 130],
        orbital_period_days: 0.31891,
        parent: Some("mars"),
    },
    Body {
        name: "deimos",
        display_name: "Deimos",
        naif_id: 402,
        mass_kg: 1.4762e15,
        diameter_m: 12.4e3,
        color: [170, 160, 150],
        orbital_period_days: 1.263,
        parent: Some("mars"),
    },
    Body {
        name: "jupiter",
        display_name: "Jupiter",
        naif_id: 5,
        mass_kg: 1898.13e24,
        diameter_m: 142984e3,
        color: [209, 167, 127],
        orbital_period_days: 4332.589,
        parent: None,
    },
    Body {
        name: "io",
        display_name: "Io",
        naif_id: 501,
        mass_kg: 89.319e21,
        diameter_m: 3643e3,
        color: [230, 200, 120],
        orbital_period_days: 1.769138,
        parent: Some("jupiter"),
    },
    Body {
        name: "europa",
        display_name: "Europa",
        naif_id: 502,
        mass_kg: 47.998e21,
        diameter_m: 3122e3,
        color: [220, 216, 190],
        orbital_period_days: 3.551181,
        parent: Some("jupiter"),
    },
    Body {
        name: "ganymede",
        display_name: "Ganymede",
        naif_id: 503,
        mass_kg: 148.19e21,
        diameter_m: 5268e3,
        color: [160, 150, 140],
        orbital_period_days: 7.15455,
        parent: Some("jupiter"),
    },
    Body {
        name: "callisto",
        display_name: "Callisto",
        naif_id: 504,
        mass_kg: 107.59e21,
        diameter_m: 4821e3,
        color: [120, 110, 100],
        orbital_period_days: 16.6890,
        parent: Some("jupiter"),
    },
    Body {
        name: "saturn",
        display_name: "Saturn",
        naif_id: 6,
        mass_kg: 568.32e24,
        diameter_m: 120536e3,
        color: [237, 219, 173],
        orbital_period_days: 10759.22,
        parent: None,
    },
    Body {
        name: "mimas",
        display_name: "Mimas",
        naif_id: 601,
        mass_kg: 3.7493e19,
        diameter_m: 396e3,
        color: [190, 185, 180],
        orbital_period_days: 0.9424220,
        parent: Some("saturn"),
    },
    Body {
        name: "enceladus",
        display_name: "Enceladus",
        naif_id: 602,
        mass_kg: 1.0802e20,
        diameter_m: 504e3,
        color: [230, 230, 235],
        orbital_period_days: 1.370218,
        parent: Some("saturn"),
    },
    Body {
        name: "tethys",
        display_name: "Tethys",
        naif_id: 603,
        mass_kg: 6.1749e20,
        diameter_m: 1062e3,
        color: [215, 210, 205],
        orbital_period_days: 1.887802,
        parent: Some("saturn"),
    },
    Body {
        name: "dione",
        display_name: "Dione",
        naif_id: 604,
        mass_kg: 1.0955e21,
        diameter_m: 1123e3,
        color: [200, 195, 190],
        orbital_period_days: 2.736915,
        parent: Some("saturn"),
    },
    Body {
        name: "rhea",
        display_name: "Rhea",
        naif_id: 605,
        mass_kg: 2.3063e21,
        diameter_m: 1528e3,
        color: [195, 190, 185],
        orbital_period_days: 4.518212,
        parent: Some("saturn"),
    },
    Body {
        name: "titan",
        display_name: "Titan",
        naif_id: 606,
        mass_kg: 134.52e21,
        diameter_m: 5150e3,
        color: [220, 160, 80],
        orbital_period_days: 15.945,
        parent: Some("saturn"),
    },
    Body {
        name: "hyperion",
        display_name: "Hyperion",
        naif_id: 607,
        mass_kg: 5.62e18,
        diameter_m: 270e3,
        color: [175, 165, 155],
        orbital_period_days: 21.276,
        parent: Some("saturn"),
    },
    Body {
        name: "iapetus",
        display_name: "Iapetus",
        naif_id: 608,
        mass_kg: 1.8056e21,
        diameter_m: 1469e3,
        color: [160, 150, 140],
        orbital_period_days: 79.322,
        parent: Some("saturn"),
    },
    Body {
        name: "phoebe",
        display_name: "Phoebe",
        naif_id: 609,
        mass_kg: 8.292e18,
        diameter_m: 213e3,
        color: [140, 130, 120],
        orbital_period_days: 550.565,
        parent: Some("saturn"),
    },
    Body {
        name: "uranus",
        display_name: "Uranus",
        naif_id: 7,
        mass_kg: 86.811e24,
        diameter_m: 51118e3,
        color: [98, 174, 231],
        orbital_period_days: 30685.4,
        parent: None,
    },
    Body {
        name: "miranda",
        display_name: "Miranda",
        naif_id: 705,
        mass_kg: 6.59e19,
        diameter_m: 472e3,
        color: [170, 175, 180],
        orbital_period_days: 1.4135,
        parent: Some("uranus"),
    },
    Body {
        name: "ariel",
        display_name: "Ariel",
        naif_id: 701,
        mass_kg: 1.353e21,
        diameter_m: 1158e3,
        color: [185, 190, 195],
        orbital_period_days: 2.520,
        parent: Some("uranus"),
    },
    Body {
        name: "umbriel",
        display_name: "Umbriel",
        naif_id: 702,
        mass_kg: 1.172e21,
        diameter_m: 1169e3,
        color: [130, 130, 135],
        orbital_period_days: 4.144,
        parent: Some("uranus"),
    },
    Body {
        name: "titania",
        display_name: "Titania",
        naif_id: 703,
        mass_kg: 3.527e21,
        diameter_m: 1578e3,
        color: [155, 150, 148],
        orbital_period_days: 8.706,
        parent: Some("uranus"),
    },
    Body {
        name: "oberon",
        display_name: "Oberon",
        naif_id: 704,
        mass_kg: 3.014e21,
        diameter_m: 1524e3,
        color: [150, 145, 142],
        orbital_period_days: 13.463,
        parent: Some("uranus"),
    },
    Body {
        name: "neptune",
        display_name: "Neptune",
        naif_id: 8,
        mass_kg: 102.409e24,
        diameter_m: 49528e3,
        color: [61, 94, 249],
        orbital_period_days: 60189.0,
        parent: None,
    },
    Body {
        name: "triton",
        display_name: "Triton",
        naif_id: 801,
        mass_kg: 21.39e21,
        diameter_m: 2707e3,
        color: [200, 190, 180],
        orbital_period_days: 5.876854,
        parent: Some("neptune"),
    },
    Body {
        name: "pluto",
        display_name: "Pluto",
        naif_id: 9,
        mass_kg: 0.01303e24,
        diameter_m: 2376e3,
        color: [229, 107, 106],
        orbital_period_days: 90560.0,
        parent: None,
    },
    Body {
        name: "charon",
        display_name: "Charon",
        naif_id: 901,
        mass_kg: 1.586e21,
        diameter_m: 1212e3,
        color: [160, 155, 150],
        orbital_period_days: 6.3872,
        parent: Some("pluto"),
    },
    Body {
        name: "nix",
        display_name: "Nix",
        naif_id: 902,
        mass_kg: 4.5e16,
        diameter_m: 49.8e3,
        color: [165, 160, 155],
        orbital_period_days: 24.85,
        parent: Some("pluto"),
    },
    Body {
        name: "hydra",
        display_name: "Hydra",
        naif_id: 903,
        mass_kg: 4.8e16,
        diameter_m: 50.9e3,
        color: [170, 165, 160],
        orbital_period_days: 38.20,
        parent: Some("pluto"),
    },
    Body {
        name: "kerberos",
        display_name: "Kerberos",
        naif_id: 904,
        mass_kg: 1.65e16,
        diameter_m: 19.0e3,
        color: [150, 145, 140],
        orbital_period_days: 32.17,
        parent: Some("pluto"),
    },
    Body {
        name: "styx",
        display_name: "Styx",
        naif_id: 905,
        mass_kg: 7.5e15,
        diameter_m: 16.0e3,
        color: [155, 150, 145],
        orbital_period_days: 20.16,
        parent: Some("pluto"),
    },
];

/// Looks a body up by its lowercase catalog name.
pub fn body_by_name(name: &str) -> Result<&'static Body, EphemerisError> {
    BODIES
        .iter()
        .find(|b| b.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| EphemerisError::UnknownBody(name.to_owned()))
}

/// Looks a body up by its NAIF id.
pub fn body_by_naif_id(id: i32) -> Result<&'static Body, EphemerisError> {
    BODIES
        .iter()
        .find(|b| b.naif_id == id)
        .ok_or_else(|| EphemerisError::UnknownBody(format!("NAIF id {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_sun_planets_and_moons() {
        assert_eq!(BODIES.len(), 37);
        assert_eq!(BODIES[0].naif_id, 10);
        assert_eq!(BODIES[1].name, "mercury");
        assert_eq!(BODIES[3].name, "earth");
        assert_eq!(BODIES[4].name, "moon");
        assert_eq!(BODIES[36].name, "styx");
    }

    #[test]
    fn naif_ids_match_spk_targets() {
        // Sun, planetary barycenters 1-9 (in order), Earth center, and the
        // moons from the satellite SPK kernels.
        let expected = [
            10, 1, 2, 399, 301, 4, 401, 402, 5, 501, 502, 503, 504, 6, 601, 602, 603, 604, 605,
            606, 607, 608, 609, 7, 705, 701, 702, 703, 704, 8, 801, 9, 901, 902, 903, 904, 905,
        ];
        let ids: Vec<i32> = BODIES.iter().map(|b| b.naif_id).collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn lookup_by_name_and_id() {
        assert_eq!(body_by_name("Earth").unwrap().naif_id, 399);
        assert_eq!(body_by_naif_id(9).unwrap().name, "pluto");
        assert_eq!(body_by_naif_id(301).unwrap().name, "moon");
        assert!(body_by_name("vulcan").is_err());
    }

    #[test]
    fn only_sun_is_non_orbiting() {
        assert!(!SUN.is_orbiting());
        assert!(BODIES.iter().skip(1).all(Body::is_orbiting));
    }

    #[test]
    fn moons_reference_a_parent_that_exists() {
        for body in BODIES {
            if let Some(parent) = body.parent {
                let parent = body_by_name(parent).unwrap();
                assert!(parent.parent.is_none(), "{} has a moon parent", body.name);
            }
        }
    }

    #[test]
    fn moon_gm_from_mass_is_sane() {
        let moon = body_by_name("moon").unwrap();
        // Known GM of the Moon: 4902.8 km^3/s^2.
        assert!((moon.gm_from_mass_km3_s2() - 4902.8).abs() < 5.0);
    }
}
