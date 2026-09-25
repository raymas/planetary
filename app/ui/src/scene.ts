/**
 * The 3D solar-system view: Sun + planets as spheres, orbit trails,
 * ecliptic grid, starfield, labels, and click/tap picking.
 *
 * Scene units are AU. SPICE ECLIPJ2000 (x, y, z) maps to three.js
 * (x, z, -y) so the ecliptic plane lies in the scene's XZ plane with
 * ecliptic north up, keeping the frame right-handed.
 *
 * Two scale modes:
 *  - "realistic": true AU distances and true body radii (inner planets
 *    become invisible specks — kept for honesty). The default view.
 *  - "compressed": log-mapped distances and log-mapped body sizes.
 *
 * Planet names are DOM labels rendered with CSS2DRenderer, overlaid on the
 * WebGL canvas.
 */

import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { CSS2DObject, CSS2DRenderer } from "three/examples/jsm/renderers/CSS2DRenderer.js";
import { makeSunShaders } from "./sun";
import type { Body, SceneOptions } from "./types";

const AU_M = 149597870700;
const SEC_PER_DAY = 86400;
/** Wheel zoom rate: distance scales by exp(deltaY * RATE) per event. */
const ZOOM_RATE = 0.0012;
/**
 * Moon labels appear only when the camera is within this factor of the
 * moon's orbital radius from its parent planet (~40 keeps the label off
 * until the orbit spans a few dozen pixels on screen).
 */
const MOON_LABEL_RANGE = 40;

/** ECLIPJ2000 AU -> scene coordinates. */
function toScene(p: readonly [number, number, number]): THREE.Vector3 {
  return new THREE.Vector3(p[0], p[2], -p[1]);
}

/** Compressed radial mapping: keeps order, tames the outer system. */
function compress(r: number): number {
  return 10 * Math.log10(1 + Math.max(r, 0));
}

function compressVec(v: THREE.Vector3): THREE.Vector3 {
  const r = v.length();
  if (r < 1e-12) return v.clone();
  return v.clone().multiplyScalar(compress(r) / r);
}

/** Visual radius for a body in compressed mode (AU-ish scene units). */
function compressedRadius(body: Body): number {
  const dKm = body.diameter_m / 1000;
  return 0.05 + 0.11 * Math.log10(Math.max(dKm, 1));
}

function bodyColor(body: Body): THREE.Color {
  const [r, g, b] = body.color;
  return new THREE.Color(r / 255, g / 255, b / 255);
}

interface BodyVisual {
  body: Body;
  mesh: THREE.Mesh;
  label: CSS2DObject;
  trail: THREE.Line | null;
  trailEt: number;
  /** User checkbox state. */
  visible: boolean;
  /** False while the body has no ephemeris data (moon outside its kernel span). */
  present: boolean;
}

export class SolarSystemScene {
  readonly scene = new THREE.Scene();
  private renderer: THREE.WebGLRenderer;
  private labelRenderer: CSS2DRenderer;
  private camera: THREE.PerspectiveCamera;
  private controls: OrbitControls;
  private visuals = new Map<string, BodyVisual>();
  private options: SceneOptions;
  private sunSurface: THREE.ShaderMaterial | null = null;
  private selectionRing: THREE.Mesh;
  private raycaster = new THREE.Raycaster();
  private pointerDownAt: { x: number; y: number } | null = null;
  private disposed = false;
  /** The Hohmann transfer arc, if one has been computed. */
  private transferLine: THREE.Line | null = null;
  private transferPoints: [number, number, number][] = [];
  private transferPrimary: string | null = null;

  /** Called with a body name when the user clicks/taps a body. */
  onPick: ((name: string) => void) | null = null;

  constructor(
    container: HTMLElement,
    bodies: Body[],
    options: SceneOptions
  ) {
    this.options = { ...options };

    // Log depth keeps precision sane with a near plane tiny enough to sit
    // right on a real-scale planet surface (near = 1e-6 AU ~ 150 km).
    this.renderer = new THREE.WebGLRenderer({
      antialias: true,
      logarithmicDepthBuffer: true,
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(container.clientWidth, container.clientHeight);
    this.renderer.setClearColor(0x0f0f0f, 1);
    container.appendChild(this.renderer.domElement);

    // DOM overlay for the planet-name labels.
    this.labelRenderer = new CSS2DRenderer();
    this.labelRenderer.setSize(container.clientWidth, container.clientHeight);
    this.labelRenderer.domElement.style.position = "absolute";
    this.labelRenderer.domElement.style.top = "0";
    this.labelRenderer.domElement.style.left = "0";
    this.labelRenderer.domElement.style.pointerEvents = "none";
    container.appendChild(this.labelRenderer.domElement);

    this.camera = new THREE.PerspectiveCamera(
      50,
      container.clientWidth / Math.max(container.clientHeight, 1),
      1e-6,
      2000
    );
    this.camera.position.set(0, 28, 52);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    this.controls.maxDistance = 400;
    // Floor for OrbitControls' own dolly (touch pinch): updated to the
    // selected body's surface whenever the selection or scale changes.
    this.controls.minDistance = 1e-4;

    this.scene.add(new THREE.AmbientLight(0xffffff, 0.35));
    const sunLight = new THREE.PointLight(0xfff2d0, 900, 0, 2);
    this.scene.add(sunLight);

    this.addEclipticGrid();
    this.addStarfield();
    this.addBodies(bodies);

    this.selectionRing = new THREE.Mesh(
      new THREE.RingGeometry(1.35, 1.5, 48),
      new THREE.MeshBasicMaterial({ color: 0xffffff, side: THREE.DoubleSide, transparent: true, opacity: 0.8 })
    );
    this.selectionRing.visible = false;
    this.scene.add(this.selectionRing);

    this.renderer.domElement.addEventListener("pointerdown", (e) => {
      this.pointerDownAt = { x: e.clientX, y: e.clientY };
    });
    this.renderer.domElement.addEventListener("pointerup", (e) => {
      const down = this.pointerDownAt;
      this.pointerDownAt = null;
      if (!down) return;
      const drag = Math.hypot(e.clientX - down.x, e.clientY - down.y);
      if (drag > 6) return; // it was a drag, not a tap
      const hit = this.pick(e.clientX, e.clientY);
      if (hit) this.onPick?.(hit);
    });

    // Proportional wheel zoom, anchored on the selected body when there is
    // one (else the orbit target): each tick scales the distance by a
    // constant factor, so it is fast when far and slow when near, and it
    // stops just above the body's surface. Captured before OrbitControls'
    // own wheel handler; touch pinch keeps using OrbitControls.
    container.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        e.stopPropagation();
        this.zoomBy(e.deltaY, e.deltaMode);
      },
      { capture: true, passive: false }
    );

    window.addEventListener("resize", () => {
      this.resize(container);
    });

    this.animate();
  }

  private addEclipticGrid(): void {
    const grid = new THREE.PolarGridHelper(50, 12, 10, 96, 0x3a3a3a, 0x262626);
    const material = grid.material as THREE.Material;
    material.transparent = true;
    material.opacity = 0.5;
    this.scene.add(grid);
    const axes = new THREE.AxesHelper(3);
    this.scene.add(axes);
  }

  private addStarfield(): void {
    const count = 4000;
    const positions = new Float32Array(count * 3);
    for (let i = 0; i < count; i++) {
      const u = Math.random() * 2 - 1;
      const theta = Math.random() * Math.PI * 2;
      const r = Math.sqrt(1 - u * u);
      const R = 600;
      positions[i * 3] = R * r * Math.cos(theta);
      positions[i * 3 + 1] = R * u;
      positions[i * 3 + 2] = R * r * Math.sin(theta);
    }
    const geom = new THREE.BufferGeometry();
    geom.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    const stars = new THREE.Points(
      geom,
      new THREE.PointsMaterial({ color: 0xc8c8c8, size: 1.2, sizeAttenuation: false, transparent: true, opacity: 0.8 })
    );
    this.scene.add(stars);
  }

  private makeLabel(text: string, color: THREE.Color): CSS2DObject {
    const el = document.createElement("div");
    el.className = "planet-label";
    el.style.color = `#${color.getHexString()}`;
    el.textContent = text;
    const label = new CSS2DObject(el);
    // Anchor the label above the body's screen position (the renderer
    // centers on `center`; 1.8 lifts it clear of the body).
    label.center.set(0.5, 1.8);
    return label;
  }

  private addBodies(bodies: Body[]): void {
    for (const body of bodies) {
      const color = bodyColor(body);
      const radius = this.visualRadius(body);
      const mesh: THREE.Mesh = new THREE.Mesh(
        new THREE.SphereGeometry(radius, 32, 24),
        new THREE.MeshStandardMaterial({ color, roughness: 0.85, metalness: 0.05 })
      );
      mesh.userData.name = body.name;
      if (body.name === "sun") {
        // Shader Sun: fBm surface + glow halo + Fresnel rim. The shells
        // share the surface geometry and scale relative to it, so they
        // follow the mesh across scale modes.
        const sun = makeSunShaders();
        mesh.material = sun.surface;
        this.sunSurface = sun.surface;
        const glow = new THREE.Mesh(mesh.geometry, sun.glow);
        glow.scale.setScalar(1.35);
        mesh.add(glow);
        const rim = new THREE.Mesh(mesh.geometry, sun.fresnel);
        rim.scale.setScalar(1.02);
        mesh.add(rim);
      }
      this.scene.add(mesh);

      const label = this.makeLabel(body.display_name, color);
      // Labels are clickable, with the same tap-vs-drag rule as the canvas.
      label.element.addEventListener("pointerdown", (e) => {
        this.pointerDownAt = { x: e.clientX, y: e.clientY };
      });
      label.element.addEventListener("pointerup", (e) => {
        const down = this.pointerDownAt;
        this.pointerDownAt = null;
        if (!down) return;
        const drag = Math.hypot(e.clientX - down.x, e.clientY - down.y);
        if (drag > 6) return; // it was a drag, not a tap
        this.onPick?.(body.name);
      });
      mesh.add(label);

      this.visuals.set(body.name, {
        body,
        mesh,
        label,
        trail: null,
        trailEt: Number.NaN,
        visible: true,
        // Hidden until the first ephemeris response names the body; moons
        // outside their satellite kernel's span never become present.
        present: false,
      });
      this.applyVisibility(this.visuals.get(body.name)!);
    }
  }

  /**
   * Effective visibility: the user checkbox, the presence of ephemeris
   * data, and — for moons — the realistic scale mode (in compressed mode
   * moon orbits collapse onto their planet, so they are hidden).
   */
  private applyVisibility(visual: BodyVisual): void {
    const show =
      visual.visible &&
      visual.present &&
      (visual.body.parent === null || this.options.scaleMode === "realistic");
    visual.mesh.visible = show;
    visual.label.visible = show && this.options.showLabels;
    if (visual.trail) visual.trail.visible = show && this.options.showTrails;
  }

  private visualRadius(body: Body): number {
    if (this.options.scaleMode === "realistic") {
      return body.diameter_m / 2 / AU_M;
    }
    return compressedRadius(body);
  }

  private scenePosition(positionAu: readonly [number, number, number]): THREE.Vector3 {
    const v = toScene(positionAu);
    return this.options.scaleMode === "compressed" ? compressVec(v) : v;
  }

  /** Applies a fresh set of positions (AU, ECLIPJ2000) keyed by body name. */
  setPositions(positions: Map<string, [number, number, number]>): void {
    for (const [name, visual] of this.visuals) {
      const p = positions.get(name);
      // A body absent from the response has no ephemeris at this epoch
      // (e.g. Phobos before 1995): hide it instead of freezing it.
      const present = p !== undefined;
      if (present !== visual.present) {
        visual.present = present;
        this.applyVisibility(visual);
      }
      if (!p) continue;
      visual.mesh.position.copy(this.scenePosition(p));
    }
  }

  /**
   * Sets (or replaces) a body's orbit trail. Points are AU, ECLIPJ2000:
   * heliocentric for planets, parent-relative for moons. A moon trail is
   * attached to its parent's mesh so the ring follows the planet around
   * the Sun without resampling.
   */
  setOrbitPath(name: string, points: [number, number, number][], et: number): void {
    const visual = this.visuals.get(name);
    if (!visual) return;
    if (visual.trail) {
      visual.trail.removeFromParent();
      visual.trail.geometry.dispose();
      visual.trail = null;
    }
    if (points.length < 2) return;
    const parentVisual = visual.body.parent
      ? this.visuals.get(visual.body.parent)
      : undefined;
    const geom = new THREE.BufferGeometry().setFromPoints(
      points.map((p) => this.scenePosition(p))
    );
    const trail = new THREE.Line(
      geom,
      new THREE.LineBasicMaterial({
        color: bodyColor(visual.body),
        transparent: true,
        opacity: 0.55,
      })
    );
    (parentVisual ? parentVisual.mesh : this.scene).add(trail);
    visual.trail = trail;
    visual.trailEt = et;
    this.applyVisibility(visual);
  }

  /**
   * Draws the Hohmann transfer arc (dashed). Points are AU, ECLIPJ2000,
   * relative to `primary` ("sun" for interplanetary transfers); the arc
   * is attached to the primary's mesh so moon-system transfers follow
   * their planet.
   */
  setTransferPath(points: [number, number, number][], primary: string): void {
    this.clearTransferPath();
    if (points.length < 2) return;
    this.transferPoints = points;
    this.transferPrimary = primary;
    this.transferLine = this.buildTransferLine();
  }

  clearTransferPath(): void {
    if (this.transferLine) {
      this.transferLine.removeFromParent();
      this.transferLine.geometry.dispose();
      (this.transferLine.material as THREE.Material).dispose();
      this.transferLine = null;
    }
    this.transferPoints = [];
    this.transferPrimary = null;
  }

  /** Builds the dashed arc from the stored raw points in the current
   * scale mode; re-run whenever the scale mode changes. */
  private buildTransferLine(): THREE.Line | null {
    if (this.transferPoints.length < 2 || !this.transferPrimary) return null;
    const parent = this.visuals.get(this.transferPrimary);
    if (!parent) return null;
    const positions = this.transferPoints.map((p) => this.scenePosition(p));
    const geom = new THREE.BufferGeometry().setFromPoints(positions);
    // Dash sizes relative to the arc's own length.
    let length = 0;
    for (let i = 1; i < positions.length; i++) {
      length += positions[i].distanceTo(positions[i - 1]);
    }
    const line = new THREE.Line(
      geom,
      new THREE.LineDashedMaterial({
        color: 0xdcdcdc,
        transparent: true,
        opacity: 0.9,
        dashSize: Math.max(length / 30, 1e-9),
        gapSize: Math.max(length / 60, 1e-9),
      })
    );
    line.computeLineDistances();
    parent.mesh.add(line);
    return line;
  }

  /** True when the trail should be resampled (seek or stale by > 1/24 period). */
  trailStale(name: string, et: number): boolean {    const visual = this.visuals.get(name);
    if (!visual || visual.body.orbital_period_days <= 0) return false;
    if (!visual.trail) return true;
    const periodSec = visual.body.orbital_period_days * SEC_PER_DAY;
    return Math.abs(et - visual.trailEt) > periodSec / 24;
  }

  isOrbiting(name: string): boolean {
    return (this.visuals.get(name)?.body.orbital_period_days ?? 0) > 0;
  }

  setSelected(name: string | null): void {
    if (!name) {
      this.selectionRing.visible = false;
      return;
    }
    this.selectionRing.visible = this.visuals.has(name);
  }

  setVisible(name: string, visible: boolean): void {
    const visual = this.visuals.get(name);
    if (!visual) return;
    visual.visible = visible;
    this.applyVisibility(visual);
  }

  setOptions(options: SceneOptions): void {
    const scaleChanged = options.scaleMode !== this.options.scaleMode;
    this.options = { ...options };
    for (const visual of this.visuals.values()) {
      this.applyVisibility(visual);
    }
    if (scaleChanged) {
      // Trails were sampled in the old scale mapping; drop them so the
      // next refresh recreates them (trailStale is true once the trail is
      // gone). Radii depend on the mode too; positions get re-applied by
      // the next frame. The Sun's glow/rim shells share the surface
      // geometry, so they follow the swap.
      for (const visual of this.visuals.values()) {
        if (visual.trail) {
          visual.trail.removeFromParent();
          visual.trail.geometry.dispose();
          visual.trail = null;
        }
        visual.mesh.geometry.dispose();
        visual.mesh.geometry = new THREE.SphereGeometry(this.visualRadius(visual.body), 32, 24);
        for (const child of visual.mesh.children) {
          if (child instanceof THREE.Mesh) child.geometry = visual.mesh.geometry;
        }
      }
      // Radii changed, so the dolly floor must follow.
      this.updateMinDistance();
      // The transfer arc was mapped in the old scale; rebuild it.
      if (this.transferPoints.length >= 2) {
        this.transferLine?.removeFromParent();
        this.transferLine?.geometry.dispose();
        this.transferLine = this.buildTransferLine();
      }
    }
  }

  /**
   * Wheel zoom: the distance to the anchor (selected body, else the orbit
   * target) scales by exp(delta * ZOOM_RATE) -- proportional, so fast when
   * far and slow when near. Zooming in stops just above the body's
   * surface; the orbit pivot follows so orbiting stays centered on the
   * body as you approach it.
   */
  private zoomBy(deltaY: number, deltaMode: number): void {
    const delta = deltaMode === 1 ? deltaY * 33 : deltaY;
    let factor = Math.exp(delta * ZOOM_RATE);
    if (factor === 1) return;

    const name = this.selectionRing.userData.name as string | undefined;
    const visual = name ? this.visuals.get(name) : undefined;
    // Clone when the anchor is the target: target.sub(anchor) below would
    // otherwise zero the shared vector before add(anchor) runs.
    const anchor = visual ? visual.mesh.position : this.controls.target.clone();
    const floor = visual ? this.visualRadius(visual.body) * 1.05 : 1e-4;
    const dist = this.camera.position.distanceTo(anchor);
    if (factor < 1) {
      factor = Math.max(factor, floor / Math.max(dist, 1e-12));
    }
    if (factor === 1) return;

    this.camera.position.sub(anchor).multiplyScalar(factor).add(anchor);
    // A no-op when the anchor is the target itself.
    this.controls.target.sub(anchor).multiplyScalar(factor).add(anchor);
  }

  private pick(clientX: number, clientY: number): string | null {
    const rect = this.renderer.domElement.getBoundingClientRect();
    const ndc = new THREE.Vector2(
      ((clientX - rect.left) / rect.width) * 2 - 1,
      -((clientY - rect.top) / rect.height) * 2 + 1
    );
    this.raycaster.setFromCamera(ndc, this.camera);
    const meshes: THREE.Object3D[] = [];
    for (const visual of this.visuals.values()) {
      if (visual.mesh.visible) meshes.push(visual.mesh);
    }
    const hits = this.raycaster.intersectObjects(meshes, false);
    return hits.length > 0 ? (hits[0].object.userData.name as string) : null;
  }

  private resize(container: HTMLElement): void {
    const w = container.clientWidth;
    const h = Math.max(container.clientHeight, 1);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h);
    this.labelRenderer.setSize(w, h);
  }

  private animate = (): void => {
    if (this.disposed) return;
    requestAnimationFrame(this.animate);

    const camDist = this.camera.position.distanceTo(this.controls.target);

    // The Sun's surface noise boils in real time.
    if (this.sunSurface) {
      this.sunSurface.uniforms.u_time.value = performance.now() / 1000;
    }

    // Keep the selection ring around the selected body, billboarded.
    if (this.selectionRing.visible) {
      const name = this.selectionRing.userData.name as string | undefined;
      const visual = name ? this.visuals.get(name) : undefined;
      if (visual) {
        const r = this.visualRadius(visual.body);
        // True radii can be sub-pixel on screen; keep the ring visible.
        const ringR =
          this.options.scaleMode === "realistic" ? Math.max(r, camDist * 0.015) : r;
        this.selectionRing.position.copy(visual.mesh.position);
        this.selectionRing.scale.setScalar(ringR);
        this.selectionRing.lookAt(this.camera.position);
        if (this.options.followSelected) {
          this.controls.target.lerp(visual.mesh.position, 0.2);
        }
      }
    }

    // Moon labels only when the camera is close enough to the parent that
    // the orbit is resolvable on screen: the threshold scales with the
    // moon's current orbital radius, so far views show a clean sky.
    if (this.options.scaleMode === "realistic") {
      for (const visual of this.visuals.values()) {
        const parentName = visual.body.parent;
        if (!parentName) continue;
        const parent = this.visuals.get(parentName);
        if (!parent) continue;
        const orbitR = visual.mesh.position.distanceTo(parent.mesh.position);
        const camToParent = this.camera.position.distanceTo(parent.mesh.position);
        const close = camToParent < orbitR * MOON_LABEL_RANGE;
        visual.label.visible =
          visual.mesh.visible && this.options.showLabels && close;
      }
    }

    this.controls.update();
    this.renderer.render(this.scene, this.camera);
    this.labelRenderer.render(this.scene, this.camera);
  };

  /** Records which body the ring should track. */
  setSelectedName(name: string | null): void {
    this.selectionRing.userData.name = name ?? undefined;
    this.setSelected(name);
    this.updateMinDistance();
  }

  /**
   * Keeps OrbitControls' dolly (touch pinch) from entering the selected
   * body: the floor sits just above its surface. Without a selection the
   * floor is a small absolute distance from the orbit target.
   */
  private updateMinDistance(): void {
    const name = this.selectionRing.userData.name as string | undefined;
    const visual = name ? this.visuals.get(name) : undefined;
    this.controls.minDistance = visual
      ? this.visualRadius(visual.body) * 1.05
      : 1e-4;
  }

  dispose(): void {
    this.disposed = true;
    this.clearTransferPath();
    this.renderer.dispose();
    this.labelRenderer.domElement.remove();
  }
}
