import type { AppConfig } from './types';
type Appearance = AppConfig['appearance'];

// Microphone RMS is linear; speech commonly occupies only a few hundredths.
// A fixed -60..-12 dBFS display range reveals quiet speech without normalising
// every utterance to the same height. This affects the visual only.
function visualLevel(rms: number) {
  if (!Number.isFinite(rms) || rms <= 0) return 0;
  const level = Math.max(0, Math.min(1, (20 * Math.log10(rms) + 60) / 48));
  return level ** 1.25;
}
function smooth(value: number, target: number, dt: number) {
  return value + (target - value) * (1 - Math.exp(-dt / (target > value ? 0.045 : 0.18)));
}
function strength(level: number, gain: number) {
  return Math.min(1, Math.max(0, level * (Number.isFinite(gain) ? gain : 1)));
}

/** A single canvas, one owned frame callback, no hidden or idle animation. */
export class EffectRenderer {
  private ctx: CanvasRenderingContext2D;
  private config: Appearance | null = null;
  private level = 0; private target = 0; private bands: number[] = []; private bandTargets: number[] = [];
  private lastInput = 0;
  private frame = 0; private last = 0; private phase = 0; private visible = true;
  private active = false; private demo = false; private observer: IntersectionObserver;
  private resize: ResizeObserver; private media = matchMedia('(prefers-reduced-motion: reduce)');
  private onVisibility = () => this.sync();
  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext('2d')!;
    this.observer = new IntersectionObserver(([entry]) => { this.visible = entry.isIntersecting; this.sync(); });
    this.observer.observe(canvas);
    this.resize = new ResizeObserver(() => { this.size(); this.draw(performance.now()); });
    this.resize.observe(canvas);
    document.addEventListener('visibilitychange', this.onVisibility);
    this.media.addEventListener('change', this.onVisibility);
    this.size();
  }
  updateConfig(config: Appearance) { this.config = { ...config }; this.draw(performance.now()); this.sync(); }
  updateData(rms: number, bands: number[]) {
    if (!this.active || this.demo) return;
    this.lastInput = performance.now();
    this.target = visualLevel(rms);
    this.bandTargets = Array.isArray(bands) ? bands.map(visualLevel) : [];
    if (this.media.matches || this.config?.motion === 'reduced') {
      // Reduced motion freezes movement, while live volume still changes shape.
      this.level = this.target; this.bands = [...this.bandTargets];
      this.draw(this.lastInput);
    }
  }
  setActive(active: boolean) {
    if (this.active === active) return;
    this.active = active;
    if (active) this.demo = false;
    this.clearSignal(); this.sync();
  }
  setDemo(demo: boolean) {
    this.demo = demo;
    if (demo) this.active = false;
    this.clearSignal(); this.sync();
  }
  private clearSignal() {
    this.level = this.target = 0; this.bands = []; this.bandTargets = [];
    this.lastInput = performance.now(); this.last = 0;
  }
  start() { this.setActive(true); } stop() { this.setActive(false); }
  destroy() {
    cancelAnimationFrame(this.frame); this.frame = 0; this.observer.disconnect(); this.resize.disconnect();
    document.removeEventListener('visibilitychange', this.onVisibility); this.media.removeEventListener('change', this.onVisibility);
  }
  private size() {
    const r = this.canvas.getBoundingClientRect(), d = Math.min(devicePixelRatio || 1, 2);
    this.canvas.width = Math.max(1, Math.round(r.width * d)); this.canvas.height = Math.max(1, Math.round(r.height * d));
  }
  private sync() {
    const animate = this.visible && !document.hidden && (this.active || this.demo)
      && !this.media.matches && this.config?.motion !== 'reduced';
    if (animate && !this.frame) { this.last = performance.now(); this.frame = requestAnimationFrame(this.tick); }
    if (!animate) {
      cancelAnimationFrame(this.frame); this.frame = 0;
      this.level = this.target; this.bands = [...this.bandTargets]; this.draw(performance.now());
    }
  }
  private tick = (time: number) => {
    this.frame = requestAnimationFrame(this.tick);
    if (time - this.last < 1000 / (this.config?.fps || 30)) return;
    const dt = Math.min(0.1, (time - this.last) / 1000 || 0.033); this.last = time;
    if (this.demo) {
      this.target = 0.12 + Math.pow(Math.sin(this.phase * 1.6) * 0.5 + 0.5, 2) * 0.72;
      this.bandTargets = Array.from({ length: 28 }, (_, i) => this.target * (0.25 + 0.75 * Math.sin(i * 0.48 + this.phase * 3) ** 2));
    } else if (time - this.lastInput > 240) {
      // Do not freeze on the last loud packet if capture events stop arriving.
      this.target = 0; this.bandTargets = this.bandTargets.map(() => 0);
    }
    this.level = smooth(this.level, this.target, dt);
    this.bands = this.bandTargets.map((target, i) => smooth(this.bands[i] ?? 0, target, dt));
    this.phase += dt * (0.65 + this.level * 2.4);
    this.draw(time);
  };
  private draw(_time: number) {
    const c = this.ctx, w = this.canvas.width, h = this.canvas.height;
    c.clearRect(0, 0, w, h); if (!this.config) return;
    const cfg = this.config, t = this.phase, energy = strength(this.level, cfg.intensity);
    const d = Math.min(devicePixelRatio || 1, 2);
    c.save(); c.lineCap = 'round'; c.lineJoin = 'round';
    c.strokeStyle = cfg.accent; c.fillStyle = cfg.accent;
    c.shadowColor = cfg.accent; c.shadowBlur = cfg.combine ? 12 * d : 0;
    const mid = h / 2;
    if (cfg.effect === 'ribbon') {
      for (let layer = 3; layer >= 0; layer--) {
        c.globalAlpha = [0.96, 0.50, 0.22, 0.12][layer]; c.lineWidth = (layer === 0 ? 1.8 : 1.2) * d;
        c.beginPath();
        for (let i = 0; i <= 150; i++) {
          const x = i / 150, envelope = Math.sin(x * Math.PI) ** 1.2;
          const a = (0.012 + energy * 0.34) * h * envelope;
          const wave = Math.sin(x * Math.PI * (3 + layer * 0.6) - t * (3 + layer * 0.5) + layer * 0.8);
          const y = mid + wave * a + Math.sin(x * 17 + t * 2.3) * a * 0.2;
          if (!i) c.moveTo(x * w, y); else c.lineTo(x * w, y);
        }
        c.stroke();
      }
    } else if (cfg.effect === 'spectrum' || cfg.effect === 'bars') {
      const count = cfg.effect === 'spectrum' ? 44 : 25, gap = w / count;
      for (let i = 0; i < count; i++) {
        const envelope = Math.sin((i + 0.5) / count * Math.PI) ** 0.7;
        const b = this.bands[Math.floor(i / count * this.bands.length)] ?? this.level;
        const height = Math.max(3 * d, (0.015 + strength(b, cfg.intensity) * 0.82) * h * envelope);
        c.globalAlpha = 0.35 + 0.65 * envelope; c.lineWidth = (cfg.effect === 'spectrum' ? 2 : 4) * d;
        c.beginPath(); c.moveTo(gap * (i + 0.5), mid - height / 2); c.lineTo(gap * (i + 0.5), mid + height / 2); c.stroke();
      }
    } else {
      const r = h * (0.08 + energy * 0.20);
      for (let ring = 0; ring < 3; ring++) {
        c.globalAlpha = 0.7 - ring * 0.23; c.lineWidth = 1.4 * d; c.beginPath();
        for (let i = 0; i <= 100; i++) {
          const a = i / 100 * Math.PI * 2, radius = r + Math.sin(a * 3 + t * 2 + ring) * energy * h * 0.07 + ring * h * 0.035;
          const x = w / 2 + Math.cos(a) * radius * 2.8, y = mid + Math.sin(a) * radius;
          if (!i) c.moveTo(x, y); else c.lineTo(x, y);
        }
        c.closePath(); c.stroke();
      }
    }
    c.restore();
  }
}
