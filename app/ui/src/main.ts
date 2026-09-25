/**
 * App bootstrap: owns the render loop, wires the panels to the scene and
 * the Tauri backend.
 */

import * as api from "./api";
import { SimClock } from "./clock";
import { SolarSystemScene } from "./scene";
import type { Body, BodyState, SceneOptions, Units } from "./types";

const AU_KM = 149597870.7;
const TRAIL_SAMPLES = 512;

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el;
}

function formatAu(au: number, units: Units): string {
  if (units === "km") {
    const km = au * AU_KM;
    return km > 1e7 ? `${(km / 1e6).toFixed(2)}e6 km` : `${km.toLocaleString(undefined, { maximumFractionDigits: 0 })} km`;
  }
  return `${au.toFixed(4)} AU`;
}

function formatVec(v: readonly [number, number, number], units: Units): string {
  return `(${v.map((c) => formatAu(c, units)).join(", ")})`;
}

function nowUtcInputValue(): string {
  return new Date().toISOString().slice(0, 19);
}

/**
 * Waits for the ephemeris backend. On Android's first launch the
 * kernels are extracted from the APK (one time, can take minutes);
 * show a note instead of a black scene until they are ready.
 */
async function waitForKernels(): Promise<void> {
  let status: string;
  try {
    status = await api.kernelStatus();
  } catch {
    return; // status endpoint unavailable (mock backend): proceed
  }
  if (status === "ready") return;
  const el = $("scene-error");
  if (status.startsWith("error")) throw new Error(status);
  el.textContent =
    "Loading ephemeris kernels \u2014 the first launch extracts them from the app package (one time only). This can take a few minutes...";
  el.classList.remove("hidden");
  for (;;) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    try {
      status = await api.kernelStatus();
    } catch {
      continue;
    }
    if (status === "ready") break;
    if (status.startsWith("error")) {
      el.classList.add("hidden");
      throw new Error(status);
    }
  }
  el.classList.add("hidden");
}

async function main(): Promise<void> {
  const bodies: Body[] = await api.getBodies();
  const bodyByName = new Map(bodies.map((b) => [b.name, b]));

  await waitForKernels();
  const startEt = await api.utcToEt(nowUtcInputValue());
  const clock = new SimClock(startEt);
  // Keep the clock inside the de440s kernel coverage (1849-12-26 to
  // 2150-01-22); a small margin avoids partial trails at the edges.
  const [etMin, etMax] = await Promise.all([
    api.utcToEt("1850-01-01T00:00:00"),
    api.utcToEt("2149-12-01T00:00:00"),
  ]);
  clock.setLimits(etMin, etMax);

  const options: SceneOptions = {
    scaleMode: "realistic",
    showTrails: true,
    showLabels: true,
    followSelected: true,
  };
  let units: Units = "au";
  let selected: string | null = null;

  const container = $("scene-container");
  const scene = new SolarSystemScene(container, bodies, options);

  const dateInput = $("date-input") as HTMLInputElement;
  const simUtc = $("sim-utc");
  const playBtn = $("btn-play") as HTMLButtonElement;
  const inspector = $("inspector");

  // ---- selection ----
  function select(name: string | null): void {
    selected = name;
    scene.setSelectedName(name);
    if (name) {
      void refreshInspector();
    } else {
      inspector.innerHTML = `<p class="hint">Click or tap a body to select it.</p>`;
    }
  }
  scene.onPick = (name) => select(name);

  // ---- inspector ----
  async function refreshInspector(): Promise<void> {
    if (!selected) return;
    let state: BodyState;
    try {
      state = await api.getState(selected, clock.get());
    } catch (e) {
      inspector.innerHTML = `<p class="error">${String(e)}</p>`;
      return;
    }
    const body = bodyByName.get(state.name);
    const rows: [string, string][] = [
      ["Position (ECLIPJ2000)", formatVec(state.position_au, units)],
      ["Velocity", formatVec(state.velocity_au_per_day, units) + " /day"],
      ["Distance from Sun", formatAu(state.distance_sun_au, units)],
      ["Distance from Earth", formatAu(state.distance_earth_au, units)],
    ];
    if (body) {
      rows.push(["Mass", `${body.mass_kg.toExponential(4)} kg`]);
      rows.push(["Diameter", `${(body.diameter_m / 1000).toLocaleString()} km`]);
      if (body.parent) {
        const parent = bodyByName.get(body.parent);
        if (parent) rows.push(["Orbits", parent.display_name]);
      }
    }
    if (state.elements) {
      const el = state.elements;
      rows.push(["Semi-major axis", formatAu(el.semi_major_axis_au, units)]);
      rows.push(["Eccentricity", el.eccentricity.toFixed(5)]);
      rows.push(["Inclination", `${el.inclination_deg.toFixed(3)}\u00b0`]);
      rows.push(["Ascending node", `${el.ascending_node_deg.toFixed(3)}\u00b0`]);
      rows.push(["Arg. of perihelion", `${el.argument_of_perihelion_deg.toFixed(3)}\u00b0`]);
      rows.push(["Mean anomaly", `${el.mean_anomaly_deg.toFixed(3)}\u00b0`]);
      rows.push(["Period", `${el.period_days.toFixed(2)} days`]);
    }
    inspector.innerHTML = `
      <h3>${state.display_name}</h3>
      <table>${rows.map(([k, v]) => `<tr><th>${k}</th><td>${v}</td></tr>`).join("")}</table>`;
  }

  // ---- trails ----
  let trailsRefreshing = false;
  async function refreshTrails(): Promise<void> {
    if (trailsRefreshing) return;
    trailsRefreshing = true;
    try {
      const et = clock.get();
      const stale = bodies.filter(
        (b) => scene.isOrbiting(b.name) && scene.trailStale(b.name, et)
      );
      await Promise.all(
        stale.map(async (b) => {
          const points = await api.getOrbitPath(b.name, et, TRAIL_SAMPLES);
          scene.setOrbitPath(b.name, points, et);
        })
      );
    } finally {
      trailsRefreshing = false;
    }
  }

  // ---- time panel ----
  function setPlayButton(playing: boolean): void {
    playBtn.innerHTML = playing ? "&#10074;&#10074;" : "&#9654;";
  }

  dateInput.addEventListener("change", () => {
    if (!dateInput.value) return;
    void api
      .utcToEt(dateInput.value)
      .then((et) => {
        clock.set(et);
        void refreshTrails();
        void refreshInspector();
      })
      .catch((e) => showError(String(e)));
  });

  $("btn-now").addEventListener("click", () => {
    void api
      .utcToEt(nowUtcInputValue())
      .then((et) => {
        clock.set(et);
        void refreshTrails();
        void refreshInspector();
      })
      .catch((e) => showError(String(e)));
  });

  playBtn.addEventListener("click", () => setPlayButton(clock.toggle()));
  $("btn-back-day").addEventListener("click", () => {
    clock.stepDays(-1);
    void refreshTrails();
  });
  $("btn-fwd-day").addEventListener("click", () => {
    clock.stepDays(1);
    void refreshTrails();
  });
  $("btn-back-hour").addEventListener("click", () => clock.step(-3600));
  $("btn-fwd-hour").addEventListener("click", () => clock.step(3600));
  $("speed").addEventListener("change", (e) => {
    const value = Number((e.target as HTMLSelectElement).value);
    clock.setSpeed(value);
  });

  function showError(message: string): void {
    const el = $("scene-error");
    el.textContent = message;
    el.classList.remove("hidden");
  }

  // ---- settings ----
  function applyOptions(): void {
    scene.setOptions(options);
  }
  $("units").addEventListener("change", (e) => {
    units = (e.target as HTMLSelectElement).value as Units;
    void refreshInspector();
  });
  $("scale").addEventListener("change", (e) => {
    options.scaleMode = (e.target as HTMLSelectElement).value as SceneOptions["scaleMode"];
    applyOptions();
    void refreshTrails();
  });
  $("show-trails").addEventListener("change", (e) => {
    options.showTrails = (e.target as HTMLInputElement).checked;
    applyOptions();
  });
  $("show-labels").addEventListener("change", (e) => {
    options.showLabels = (e.target as HTMLInputElement).checked;
    applyOptions();
  });
  $("follow-selected").addEventListener("change", (e) => {
    options.followSelected = (e.target as HTMLInputElement).checked;
    applyOptions();
  });

  // ---- body list ----
  const bodyList = $("body-list");
  for (const body of bodies) {
    const li = document.createElement("li");
    if (body.parent) li.classList.add("moon");
    const label = document.createElement("label");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = true;
    checkbox.addEventListener("change", () => {
      scene.setVisible(body.name, checkbox.checked);
    });
    const swatch = document.createElement("span");
    swatch.className = "swatch";
    swatch.style.background = `rgb(${body.color.join(",")})`;
    const name = document.createElement("span");
    name.textContent = body.display_name;
    label.append(checkbox, swatch, name);
    li.appendChild(label);
    bodyList.appendChild(li);
  }

  // ---- hohmann transfer ----
  const transferFrom = $("transfer-from") as HTMLSelectElement;
  const transferTo = $("transfer-to") as HTMLSelectElement;
  const transferResults = $("transfer-results");
  {
    // Group the options by primary: planets around the Sun, moons under
    // their planet.
    const groups = new Map<string, Body[]>([["sun", [] as Body[]]]);
    for (const body of bodies) {
      if (body.name === "sun") continue;
      const key = body.parent ?? "sun";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(body);
    }
    const labels: Record<string, string> = { sun: "Sun system" };
    for (const body of bodies) {
      if (body.parent) labels[body.name] = `${body.display_name} system`;
    }
    for (const select of [transferFrom, transferTo]) {
      for (const [key, groupBodies] of groups) {
        if (groupBodies.length === 0) continue;
        const optgroup = document.createElement("optgroup");
        optgroup.label = labels[key] ?? key;
        for (const body of groupBodies) {
          const option = document.createElement("option");
          option.value = body.name;
          option.textContent = body.display_name;
          optgroup.appendChild(option);
        }
        select.appendChild(optgroup);
      }
    }
    transferFrom.value = "earth";
    transferTo.value = "mars";
  }

  function formatSeconds(s: number): string {
    const days = s / 86400;
    return days >= 2 ? `${days.toFixed(1)} days` : `${(s / 3600).toFixed(1)} h`;
  }

  function formatKm(km: number): string {
    return km >= 1e6 ? `${(km / 1e6).toFixed(2)}M` : Math.round(km).toLocaleString("en-US");
  }

  async function computeTransfer(): Promise<void> {
    const from = transferFrom.value;
    const to = transferTo.value;
    const depAlt = Number(($("transfer-dep-alt") as HTMLInputElement).value) * 1000;
    const arrAlt = Number(($("transfer-arr-alt") as HTMLInputElement).value) * 1000;
    transferResults.innerHTML = `<p class="hint">Computing...</p>`;
    try {
      const t = await api.getHohmannTransfer(from, to, clock.get(), depAlt, arrAlt);
      const [depUtc, arrUtc] = await Promise.all([
        api.etToUtc(t.departure_et),
        api.etToUtc(t.arrival_et),
      ]);
      const rows: [string, string][] = [
        ["dV departure", `${(t.dv_departure_m_s / 1000).toFixed(3)} km/s`],
        ["dV arrival", `${(t.dv_arrival_m_s / 1000).toFixed(3)} km/s`],
        ["dV total", `${(t.dv_total_m_s / 1000).toFixed(3)} km/s`],
        ["v-inf departure", `${(t.v_inf_departure_m_s / 1000).toFixed(3)} km/s`],
        ["v-inf arrival", `${(t.v_inf_arrival_m_s / 1000).toFixed(3)} km/s`],
        ["Transfer time", formatSeconds(t.transfer_time_s)],
        ["Phase angle", `${t.phase_angle_deg.toFixed(1)}\u00b0`],
        ["Plane inclination", `${t.transfer_inclination_deg.toFixed(2)}\u00b0`],
        ["Eject burn angle", `${t.eject_burn_angle_deg.toFixed(1)}\u00b0`],
        ["Eject declination", `${t.eject_declination_deg.toFixed(1)}\u00b0`],
        ["Insert burn angle", `${t.insert_burn_angle_deg.toFixed(1)}\u00b0`],
        ["Insert declination", `${t.insert_declination_deg.toFixed(1)}\u00b0`],
        ["N-body miss", t.nbody_miss_km != null ? `${formatKm(t.nbody_miss_km)} km` : "n/a"],
        [
          "N-body correction",
          t.nbody_correction_m_s != null ? `${t.nbody_correction_m_s.toFixed(2)} m/s` : "n/a",
        ],
        ["Synodic period", formatSeconds(t.synodic_period_s)],
        ["Departure window", `${depUtc.slice(0, 10)} UTC`],
        ["Arrival", `${arrUtc.slice(0, 10)} UTC`],
      ];
      transferResults.innerHTML = `
        <h3>${bodyByName.get(from)?.display_name ?? from} \u2192 ${bodyByName.get(to)?.display_name ?? to}</h3>
        <table>${rows.map(([k, v]) => `<tr><th>${k}</th><td>${v}</td></tr>`).join("")}</table>
        <p class="hint">Lambert arc through real positions, n-body checked; window and flight time minimize total dV.</p>`;
      scene.setTransferPath(t.path_au, t.primary);
    } catch (e) {
      scene.clearTransferPath();
      transferResults.innerHTML = `<p class="error">${String(e)}</p>`;
    }
  }

  $("btn-transfer").addEventListener("click", () => void computeTransfer());
  $("btn-transfer-clear").addEventListener("click", () => {
    scene.clearTransferPath();
    transferResults.innerHTML = `<p class="hint">Pick two bodies orbiting the same primary.</p>`;
  });

  // ---- mobile panel toggles ----
  // One drawer at a time; tapping the scene closes whatever is open.
  const drawerById = new Map<string, HTMLElement>();
  for (const id of ["left", "right"]) {
    const el = document.getElementById(id);
    if (el) drawerById.set(id, el);
  }
  function closeDrawers(except?: HTMLElement): void {
    for (const drawer of drawerById.values()) {
      if (drawer !== except) drawer.classList.remove("open");
    }
  }
  for (const btn of document.querySelectorAll("#mobile-toggles button")) {
    btn.addEventListener("click", () => {
      const target = drawerById.get(btn.getAttribute("data-toggle") ?? "");
      if (!target) return;
      const willOpen = !target.classList.contains("open");
      closeDrawers(willOpen ? target : undefined);
      target.classList.toggle("open", willOpen);
    });
  }
  document.getElementById("viewport")?.addEventListener("pointerdown", (e) => {
    // The toggle buttons live inside the viewport; keep them clickable.
    if ((e.target as HTMLElement).closest("#mobile-toggles")) return;
    closeDrawers();
  });

  // ---- render loop ----
  let lastFrame = performance.now();
  let lastUtcUpdate = 0;
  let lastInspectorUpdate = 0;
  let positionQuery: Promise<void> | null = null;

  function frame(now: number): void {
    const dt = Math.min((now - lastFrame) / 1000, 0.25);
    lastFrame = now;
    const et = clock.tick(dt);

    // One positions query in flight at a time; results apply whenever
    // they land (SPICE answers in microseconds).
    if (!positionQuery) {
      positionQuery = api
        .getPositions(et)
        .then((positions) => {
          scene.setPositions(new Map(positions.map((p) => [p.name, p.position_au])));
        })
        .catch((e) => showError(String(e)))
        .finally(() => {
          positionQuery = null;
        });
    }

    if (now - lastUtcUpdate > 250) {
      lastUtcUpdate = now;
      void api
        .etToUtc(et)
        .then((utc) => {
          simUtc.textContent = `${utc} UTC`;
          if (document.activeElement !== dateInput) {
            dateInput.value = utc.slice(0, 19);
          }
        })
        .catch(() => undefined);
    }

    if (selected && now - lastInspectorUpdate > 500) {
      lastInspectorUpdate = now;
      void refreshInspector();
    }

    void refreshTrails();
    requestAnimationFrame(frame);
  }

  // Initial state: trails + first positions, then start the loop.
  await refreshTrails();
  const positions = await api.getPositions(clock.get()).catch((e) => {
    showError(String(e));
    return [];
  });
  scene.setPositions(new Map(positions.map((p) => [p.name, p.position_au])));

  // Start running in real time (1 s/s).
  clock.play();
  setPlayButton(true);
  requestAnimationFrame(frame);
}

main().catch((e) => {
  const el = document.getElementById("scene-error");
  if (el) {
    el.textContent = `Failed to start: ${String(e)}`;
    el.classList.remove("hidden");
  }
});
