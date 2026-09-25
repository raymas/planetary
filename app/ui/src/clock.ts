/**
 * The simulation clock, owned by the frontend.
 *
 * Time is ephemeris seconds past J2000 (ET), the same axis the Rust core
 * queries SPICE on. The clock advances in the render loop; SPICE queries
 * are microseconds, so per-frame queries are fine.
 */

const SEC_PER_DAY = 86400;

export class SimClock {
  private et: number;
  private playing = false;
  private speed = 1; // sim seconds per real second (real time)
  // de440s kernel coverage; set once the backend is up.
  private min = Number.NEGATIVE_INFINITY;
  private max = Number.POSITIVE_INFINITY;

  /** Callback fired whenever the time is set or stepped (not per frame). */
  onSeek: (() => void) | null = null;

  constructor(et: number) {
    this.et = et;
  }

  /** Clamps future set/tick values into the ephemeris coverage window. */
  setLimits(min: number, max: number): void {
    this.min = min;
    this.max = max;
    this.set(this.et);
  }

  private clamp(et: number): number {
    return Math.min(Math.max(et, this.min), this.max);
  }

  get(): number {
    return this.et;
  }

  set(et: number): void {
    this.et = this.clamp(et);
    this.onSeek?.();
  }

  /** Advances by real elapsed time; returns the new ET. */
  tick(dtRealSeconds: number): number {
    if (this.playing) {
      this.et = this.clamp(this.et + dtRealSeconds * this.speed);
    }
    return this.et;
  }

  step(seconds: number): void {
    this.set(this.et + seconds);
  }

  stepDays(days: number): void {
    this.step(days * SEC_PER_DAY);
  }

  isPlaying(): boolean {
    return this.playing;
  }

  play(): void {
    this.playing = true;
  }

  pause(): void {
    this.playing = false;
  }

  toggle(): boolean {
    this.playing = !this.playing;
    return this.playing;
  }

  setSpeed(simSecondsPerRealSecond: number): void {
    this.speed = simSecondsPerRealSecond;
  }
}
