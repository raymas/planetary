/** Shared types mirroring the Rust core's serialized output. */

export interface Body {
  name: string;
  display_name: string;
  naif_id: number;
  mass_kg: number;
  diameter_m: number;
  color: [number, number, number];
  orbital_period_days: number;
  /** Catalog name of the parent body for natural satellites, else null. */
  parent: string | null;
}

export interface BodyPosition {
  name: string;
  position_au: [number, number, number];
}

export interface OrbitalElements {
  semi_major_axis_au: number;
  eccentricity: number;
  inclination_deg: number;
  ascending_node_deg: number;
  argument_of_perihelion_deg: number;
  mean_anomaly_deg: number;
  period_days: number;
}

export interface BodyState {
  name: string;
  display_name: string;
  position_au: [number, number, number];
  velocity_au_per_day: [number, number, number];
  distance_sun_au: number;
  distance_earth_au: number;
  elements: OrbitalElements | null;
}

export type ScaleMode = "compressed" | "realistic";
export type Units = "au" | "km";

/** Lambert transfer between two bodies sharing a primary, seeded from
 *  the idealized Hohmann ellipse. */
export interface HohmannTransfer {
  dv_departure_m_s: number;
  dv_arrival_m_s: number;
  dv_total_m_s: number;
  transfer_time_s: number;
  /** Signed: negative when the target trails (inbound transfer). */
  phase_angle_deg: number;
  synodic_period_s: number;
  /** Best departure window in [epoch, epoch + synodic period] (TDB seconds). */
  departure_et: number;
  arrival_et: number;
  /** Hyperbolic excess speed relative to the departure body, m/s. */
  v_inf_departure_m_s: number;
  /** Hyperbolic excess speed relative to the arrival body, m/s. */
  v_inf_arrival_m_s: number;
  /** Inclination of the transfer plane to the ecliptic, degrees. */
  transfer_inclination_deg: number;
  /** Hyperbolic excess velocity at departure, parent-relative km/s, ECLIPJ2000. */
  v_inf_departure_km_s: [number, number, number];
  /** Hyperbolic excess velocity at arrival, parent-relative km/s, ECLIPJ2000. */
  v_inf_arrival_km_s: [number, number, number];
  /** Angle of the departure asymptote out of the departure body's orbital plane, signed degrees. */
  eject_declination_deg: number;
  /** Angle of the arrival asymptote out of the arrival body's orbital plane, signed degrees. */
  insert_declination_deg: number;
  /** Parking-orbit burn point vs departure asymptote (hyperbolic true anomaly), degrees. */
  eject_burn_angle_deg: number;
  /** Parking-orbit burn point vs arrival asymptote, degrees. */
  insert_burn_angle_deg: number;
  /** Arrival miss under n-body gravity if the arc is flown as-is, km; null when unavailable. */
  nbody_miss_km: number | null;
  /** Departure velocity correction cancelling the n-body miss, m/s; null when unavailable. */
  nbody_correction_m_s: number | null;
  /** Remaining miss after the correction, km; null when unavailable. */
  nbody_residual_km: number | null;
  /** Catalog name of the shared primary ("sun" for planets). */
  primary: string;
  /** Transfer arc, parent-relative AU, ECLIPJ2000, departure first. */
  path_au: [number, number, number][];
}

export interface SceneOptions {
  scaleMode: ScaleMode;
  showTrails: boolean;
  showLabels: boolean;
  followSelected: boolean;
}
