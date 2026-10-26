import assert from 'node:assert/strict';
import test from 'node:test';
import { EffectRenderer } from '../src/effects.ts';

// Exercise the production renderer with a deterministic clock and canvas paths.
// The module is imported straight from source: Node strips the types, so the
// test no longer depends on the TypeScript compiler API. That API is no longer
// on the `typescript` root export (7.x ships it under `typescript/unstable/*`
// and dropped `transpileModule`), and `src/` is kept to erasable syntax so this
// stays a plain import.
// Node only strips erasable syntax, so `src/` must not use constructs the
// stripper rejects (enums, namespaces, parameter properties). Catching that here
// keeps every test a plain import instead of a compiler-API call.
test('src/ stays within erasable TypeScript syntax', async () => {
  const { readdir, readFile } = await import('node:fs/promises');
  const dir = new URL('../src/', import.meta.url);
  const offences = [];
  for (const name of (await readdir(dir)).filter(f => f.endsWith('.ts'))) {
    const body = await readFile(new URL(name, dir), 'utf8');
    for (const [line, content] of body.split('\n').entries()) {
      if (/^\s*(export\s+)?(declare\s+)?(const\s+)?enum\s/.test(content)) offences.push(`${name}:${line + 1} enum`);
      if (/^\s*(export\s+)?namespace\s/.test(content)) offences.push(`${name}:${line + 1} namespace`);
      if (/constructor\s*\([^)]*\b(?:public|private|protected|readonly)\b/.test(content)) offences.push(`${name}:${line + 1} parameter property`);
    }
  }
  assert.deepEqual(offences, [], 'non-erasable syntax breaks Node type stripping');
});

const effects = ['ribbon', 'spectrum', 'bars', 'orbit'];

function fixture(effect = 'ribbon', reduced = false, intensity = 1) {
  let now = 0, nextId = 0;
  const callbacks = new Map();
  const paths = [];
  let current = [];
  const context = {
    clearRect() { paths.length = 0; }, save() {}, restore() {},
    beginPath() { current = []; },
    moveTo(x, y) { current.push([x, y]); },
    lineTo(x, y) { current.push([x, y]); },
    closePath() {}, stroke() { paths.push(current); }
  };
  Object.assign(globalThis, {
    devicePixelRatio: 1,
    document: { hidden: false, addEventListener() {}, removeEventListener() {} },
    matchMedia: () => ({ matches: reduced, addEventListener() {}, removeEventListener() {} }),
    IntersectionObserver: class { observe() {} disconnect() {} },
    ResizeObserver: class { observe() {} disconnect() {} },
    requestAnimationFrame: callback => { const id = ++nextId; callbacks.set(id, callback); return id; },
    cancelAnimationFrame: id => callbacks.delete(id)
  });
  Object.defineProperty(globalThis, 'performance', { configurable: true, value: { now: () => now } });
  const canvas = { width: 240, height: 58, getContext: () => context, getBoundingClientRect: () => ({ width: 240, height: 58 }) };
  const renderer = new EffectRenderer(canvas);
  renderer.updateConfig({ effect, accent: '#a7e8cf', intensity, combine: false, motion: reduced ? 'reduced' : 'full', fps: 60 });
  renderer.setActive(true);
  function frame() {
    now += 1000 / 60;
    const pending = [...callbacks.entries()]; callbacks.clear();
    for (const [, callback] of pending) callback(now);
  }
  return {
    renderer, paths, callbacks,
    signal(rms, bands = Array(24).fill(rms)) { renderer.updateData(rms, bands); },
    advance(ms, rms) {
      for (let elapsed = 0; elapsed < ms; elapsed += 1000 / 60) {
        if (rms !== undefined) renderer.updateData(rms, Array(24).fill(rms));
        frame();
      }
    },
    height() {
      const points = paths.flat();
      assert.ok(points.every(([x, y]) => Number.isFinite(x) && Number.isFinite(y)), 'canvas coordinates must remain finite');
      return points.length ? Math.max(...points.map(p => p[1])) - Math.min(...points.map(p => p[1])) : 0;
    }
  };
}
function measured(effect, rms, reduced = false) {
  const f = fixture(effect, reduced); f.signal(rms); f.advance(450, rms); return f.height();
}

for (const effect of effects) {
  test(`${effect}: quiet, normal and loud speech are visibly distinct`, t => {
    const silence = measured(effect, 0);
    const quiet = measured(effect, 0.003);
    const normal = measured(effect, 0.02);
    const loud = measured(effect, 0.1);
    assert.ok(quiet > silence + 1, `quiet ${quiet} vs silence ${silence}`);
    assert.ok(normal > quiet + 3, `normal ${normal} vs quiet ${quiet}`);
    assert.ok(loud > normal + 3, `loud ${loud} vs normal ${normal}`);
    assert.ok(loud <= 56, `loud speech must stay inside the 58px canvas: ${loud}`);
    t.diagnostic(`58px canvas heights: ${JSON.stringify({ silence, quiet, normal, loud })}`);
  });
  test(`${effect}: reduced motion still responds to live audio`, () => {
    const quiet = measured(effect, 0.003, true);
    const loud = measured(effect, 0.1, true);
    assert.ok(loud > quiet + 6, `reduced mode loud ${loud} vs quiet ${quiet}`);
  });
  test(`${effect}: stop clears stale audio and ignores late packets`, () => {
    const f = fixture(effect); f.advance(450, 0.1);
    f.renderer.setActive(false);
    f.signal(0.1); f.advance(150);
    assert.ok(f.height() <= measured(effect, 0) + 0.5, `stopped height: ${f.height()}`);
    assert.equal(f.callbacks.size, 0, 'no idle animation');
  });
}

test('audio event loss releases the waveform instead of freezing it', () => {
  const f = fixture(); f.advance(450, 0.1);
  const speaking = f.height(); f.advance(1200);
  assert.ok(f.height() < speaking / 3, `stale audio should decay: ${f.height()} vs ${speaking}`);
});

test('zero-valued bands stay silent beside a strong band', () => {
  const f = fixture('spectrum');
  f.signal(0.1, [0.1, 0, 0.1]); f.advance(180, undefined);
  const side = f.paths[Math.floor(f.paths.length / 4)], center = f.paths[Math.floor(f.paths.length / 2)];
  assert.ok(Math.abs(center[1][1] - center[0][1]) <= 3.1, 'a zero band must not fall back to overall RMS');
  assert.ok(Math.abs(side[1][1] - side[0][1]) > 6);
});

test('high intensity cannot clip the waveform', () => {
  for (const effect of effects) {
    const f = fixture(effect, false, 3); f.advance(450, 0.8);
    const points = f.paths.flat(); f.height();
    assert.ok(points.every(([, y]) => y >= 1 && y <= 57), `${effect} clipped outside canvas`);
  }
});
