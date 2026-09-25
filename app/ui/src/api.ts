/** Typed wrappers over the Tauri invoke bridge. */

import { invoke } from "@tauri-apps/api/core";
import type { Body, BodyPosition, BodyState, HohmannTransfer } from "./types";

export function getBodies(): Promise<Body[]> {
  return invoke("get_bodies");
}

export function getPositions(et: number): Promise<BodyPosition[]> {
  return invoke("get_positions", { et });
}

export function getState(body: string, et: number): Promise<BodyState> {
  return invoke("get_state", { body, et });
}

export function getOrbitPath(
  body: string,
  et: number,
  samples: number
): Promise<[number, number, number][]> {
  return invoke("get_orbit_path", { body, et, samples });
}

export function getHohmannTransfer(
  from: string,
  to: string,
  et: number,
  departureAltM: number,
  arrivalAltM: number
): Promise<HohmannTransfer> {
  return invoke("get_hohmann_transfer", {
    from,
    to,
    et,
    departureAltM,
    arrivalAltM,
  });
}

export function utcToEt(utc: string): Promise<number> {
  return invoke("utc_to_et", { utc });
}

export function etToUtc(et: number): Promise<string> {
  return invoke("et_to_utc", { et });
}

/** Backend readiness: "loading", "ready", or "error: ...". */
export function kernelStatus(): Promise<string> {
  return invoke("kernel_status");
}
