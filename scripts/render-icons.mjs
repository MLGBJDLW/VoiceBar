// Icon rasterizer used by .github/workflows/icons.yml (and local maintenance).
// Rasterizes crates/voicebar/icons/app-icon.svg with a tiny scanline renderer.
// It uses Node built-ins only; no browser or package installation is needed:
//   node scripts/render-icons.mjs      # writes PNG/ICO into crates/voicebar/icons/
// Only the primitives used by app-icon.svg are supported (rect + stroked lines);
// the script fails loudly on anything else instead of guessing.
import { mkdirSync, writeFileSync } from 'node:fs';
import { inflateSync, deflateSync } from 'node:zlib';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const icons = join(root, 'crates', 'voicebar', 'icons');
const SVG = join(icons, 'app-icon.svg');
const SIZES = [16, 32, 48, 64, 128, 256, 512];

// ---------- PNG encoder ----------
const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
};
export function encodePng(width, height, rgba) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0); header.writeUInt32BE(height, 4);
  header[8] = 8; header[9] = 6; header[10] = 0; header[11] = 0; header[12] = 0;
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0;
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header), chunk('IDAT', deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---------- minimal SVG scene ----------
const hex = (s) => {
  const short = /^#([0-9a-fA-F]{3})$/.exec(s);
  if (short) return [...short[1]].map((c) => parseInt(c + c, 16));
  const long = /^#([0-9a-fA-F]{6})$/.exec(s);
  if (!long) throw new Error(`unsupported color: ${s}`);
  return [1, 3, 5].map((i) => parseInt(long[1].slice(i - 1, i + 1), 16));
};

// Effective paint for one element, inheriting stroke/fill/fill-opacity from
// the elements that enclose it (rects, lines, groups, and the svg root).
function paintFor(chain) {
  const attrs = (name) => chain.map((el) => new RegExp(`\\b${name}="([^"]*)"`).exec(el)?.[1]).find((v) => v !== undefined);
  const strokeAttr = attrs('stroke');
  if (strokeAttr && strokeAttr !== 'none') {
    return { stroke: strokeAttr, opacity: Number(attrs('stroke-opacity') ?? 1) };
  }
  const fill = attrs('fill');
  if (fill && fill !== 'none') return { fill, opacity: Number(attrs('fill-opacity') ?? 1) };
  throw new Error(`shape has no visible paint: ${chain[chain.length - 1]}`);
}

export function parseSvg(text) {
  const scene = { background: [0, 0, 0], shapes: [] };
  const vb = /<svg[^>]*viewBox="([^"]+)"/.exec(text)?.[1]?.trim().split(/[\s,]+/).map(Number);
  if (!vb || vb.length !== 4 || vb[0] !== 0 || vb[1] !== 0 || vb[2] !== vb[3]) throw new Error('svg needs a square viewBox starting at 0,0');
  scene.size = vb[2];
  const markup = text.replace(/<!--[\s\S]*?-->/g, '');
  // Full-canvas background rect (no x/y, width/height == viewBox) becomes the canvas color.
  const bg = /<rect\b(?![^>]*\sx=")[^>]*width="(\d+)"\s+height="(\d+)"\s+fill="(#[0-9a-fA-F]{6})"[^>]*\/>/.exec(markup);
  if (bg && Number(bg[1]) === scene.size && Number(bg[2]) === scene.size) scene.background = hex(bg[3]);
  const offset = markup.indexOf(bg?.[0] ?? '') + (bg?.[0].length ?? 0);
  // Walk the remaining markup, tracking the open real elements as paint chain.
  // (Self-closing gradient <stop> elements are pushed but never closed, and are
  // simply not consulted for stroke/fill.)
  const open = [{ el: /<svg[^>]*>/.exec(markup)[0] }];
  const token = /<(\/?)([a-zA-Z]+)([^>]*?)(\/?)>/g;
  token.lastIndex = offset;
  for (let m; (m = token.exec(markup)); ) {
    const [raw, closing, name, rest, selfClosing] = m;
    if (closing) {
      const idx = open.map((o) => /^<([a-zA-Z]+)/.exec(o.el)[1]).lastIndexOf(name);
      if (idx >= 0) open.splice(idx);
      continue;
    }
    const el = raw;
    if (name === 'rect' || name === 'line') {
      const chain = [...open.map((o) => o.el), el];
      const attr = (q) => new RegExp(`\\b${q}="([^"]*)"`).exec(el)?.[1];
      const num = (q) => (attr(q) === undefined ? undefined : Number(attr(q)));
      const paint = paintFor(chain);
      if (name === 'rect') {
        const shape = { type: 'rect', x: num('x') ?? 0, y: num('y') ?? 0, w: num('width'), h: num('height'), r: num('rx') || 0, ...paint };
        if (shape.w === undefined || shape.h === undefined) throw new Error(`unsupported rect: ${el}`);
        scene.shapes.push(shape);
      } else {
        const widthAttr = chain.map((s) => /\bstroke-width="([\d.]+)"/.exec(s)?.[1]).find(Boolean);
        const shape = { type: 'line', x1: num('x1'), y1: num('y1'), x2: num('x2'), y2: num('y2'),
          strokeWidth: num('stroke-width') ?? Number(widthAttr ?? 0), ...paint };
        scene.shapes.push(shape);
      }
    }
    if (!selfClosing && name !== 'defs') open.push({ el });
    if (name === 'defs') { const close = markup.indexOf('</defs>', m.index); token.lastIndex = close < 0 ? markup.length : close + 7; }
  }
  if (!scene.shapes.length) throw new Error('no drawable shapes found');
  return scene;
}

// Signed distance (in svg units) from point to shape; <= r means inside.
function inside(shape, x, y, r) {
  if (shape.type === 'rect') {
    const cx = shape.x + shape.w / 2, cy = shape.y + shape.h / 2;
    const hx = shape.w / 2, hy = shape.h / 2, rad = Math.min(shape.r, hx, hy);
    const qx = Math.abs(x - cx) - (hx - rad), qy = Math.abs(y - cy) - (hy - rad);
    const dist = Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - rad;
    if (shape.strokeWidth > 0 && !shape.fill) return Math.abs(dist) <= shape.strokeWidth / 2;
    return dist <= r;
  }
  // capsule around the segment
  const dx = shape.x2 - shape.x1, dy = shape.y2 - shape.y1;
  const len2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((x - shape.x1) * dx + (y - shape.y1) * dy) / len2));
  const dist = Math.hypot(x - (shape.x1 + t * dx), y - (shape.y1 + t * dy));
  const half = (shape.strokeWidth || 0) / 2;
  return half > 0 && dist <= half + r;
}

const blend = (buf, i, color, alpha) => {
  if (alpha <= 0) return;
  for (let c = 0; c < 3; c++) buf[i + c] = Math.round(buf[i + c] * (1 - alpha) + color[c] * alpha);
  buf[i + 3] = Math.max(buf[i + 3], Math.round(255 * alpha));
};

export function render(scene, size) {
  const rgba = Buffer.alloc(size * size * 4);
  const scale = size / scene.size;
  const edge = 0.5 / scale; // half a target pixel in svg units
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      const i = (py * size + px) * 4;
      const x = (px + 0.5) / scale, y = (py + 0.5) / scale;
      rgba[i] = scene.background[0]; rgba[i + 1] = scene.background[1]; rgba[i + 2] = scene.background[2]; rgba[i + 3] = 255;
      for (const shape of scene.shapes) {
        const color = hex(shape.fill ?? shape.stroke);
        const opacity = Number.isFinite(shape.opacity) ? shape.opacity : 1;
        const hit = inside(shape, x, y, edge) ? 1 : inside(shape, x, y, -edge) ? 0.5 : 0;
        blend(rgba, i, color, hit * opacity);
      }
    }
  }
  return rgba;
}

// ---------- ICO container (PNG-compressed entries, Windows Vista+) ----------
export function encodeIco(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); header.writeUInt16LE(1, 2); header.writeUInt16LE(entries.length, 4);
  const dir = Buffer.alloc(entries.length * 16);
  let offset = 6 + dir.length;
  entries.forEach(({ size, png }, n) => {
    const e = n * 16;
    dir[e] = size >= 256 ? 0 : size; dir[e + 1] = size >= 256 ? 0 : size;
    dir[e + 2] = 0; dir[e + 3] = 0;
    dir.writeUInt16LE(1, e + 4); dir.writeUInt16LE(32, e + 6);
    dir.writeUInt32LE(png.length, e + 8); dir.writeUInt32LE(offset, e + 12);
    offset += png.length;
  });
  return Buffer.concat([header, dir, ...entries.map((e) => e.png)]);
}

async function main() {
  const { readFileSync } = await import('node:fs');
  const scene = parseSvg(readFileSync(SVG, 'utf8'));
  mkdirSync(icons, { recursive: true });
  // Native Windows icon set; retain the existing PNG sizes for source assets.
  const outputs = [
    ['32x32.png', 32], ['128x128.png', 128], ['128x128@2x.png', 256], ['icon.png', 512],
    ['icon_16x16.png', 16], ['icon_32x32.png', 32], ['icon_48x48.png', 48], ['icon_256x256.png', 256],
  ];
  for (const [name, size] of outputs) {
    writeFileSync(join(icons, name), encodePng(size, size, render(scene, size)));
    console.log(`wrote crates/voicebar/icons/${name} (${size}px)`);
  }
  const icoSizes = [16, 32, 48, 256];
  writeFileSync(join(icons, 'icon.ico'), encodeIco(icoSizes.map((size) => ({ size, png: encodePng(size, size, render(scene, size)) }))));
  console.log(`wrote crates/voicebar/icons/icon.ico (${icoSizes.join('/')}px)`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main().catch((error) => { console.error(error); process.exit(1); });
}

export { SIZES, SVG, icons };
