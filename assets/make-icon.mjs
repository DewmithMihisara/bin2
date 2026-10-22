// Draws the bin2 icon and writes assets/bin2.ico (plus an optional PNG preview).
// Usage: node assets/make-icon.mjs [preview.png]
// No dependencies: shapes are signed-distance functions, supersampled 4x4 per pixel.

import { writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SIZES = [16, 24, 32, 48, 256];
const TOP = [0x5b, 0x72, 0xff]; // background gradient, top
const BOTTOM = [0x3d, 0x5a, 0xfe]; // background gradient, bottom
const LETTER_B = [0xff, 0xff, 0xff];
const LETTER_2 = [0xd6, 0xdc, 0xff];
const DX = -0.05; // optical centring of the glyphs
const DY = 0.02;

const segment = (px, py, ax, ay, bx, by) => {
  const vx = bx - ax, vy = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)));
  return Math.hypot(px - (ax + t * vx), py - (ay + t * vy));
};
const ring = (px, py, cx, cy, r) => Math.abs(Math.hypot(px - cx, py - cy) - r);
// Arc from angle a0 to a1 in degrees, clockwise on screen, where 180 is the left point.
const arc = (px, py, cx, cy, r, a0, a1) => {
  let a = (Math.atan2(py - cy, px - cx) * 180) / Math.PI;
  if (a < 180) a += 360;
  if (a >= a0 && a <= a1) return ring(px, py, cx, cy, r);
  const end = (deg) => [cx + r * Math.cos((deg * Math.PI) / 180), cy + r * Math.sin((deg * Math.PI) / 180)];
  const [x0, y0] = end(a0), [x1, y1] = end(a1);
  return Math.min(Math.hypot(px - x0, py - y0), Math.hypot(px - x1, py - y1));
};
const roundedSquare = (px, py, inset, radius) => {
  const half = 0.5 - inset;
  const qx = Math.abs(px - 0.5) - (half - radius);
  const qy = Math.abs(py - 0.5) - (half - radius);
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - radius;
};

const B_STROKE = 0.11, TWO_STROKE = 0.075;
const TWO_X = 0.05; // gap between the two letters
const inB = (x, y) =>
  Math.min(segment(x, y, 0.27, 0.22, 0.27, 0.74), ring(x, y, 0.405, 0.6, 0.135)) <= B_STROKE / 2;
const inTwo = (x0, y) => {
  const x = x0 - TWO_X;
  const ex = 0.68 + 0.09 * Math.cos(Math.PI / 6), ey = 0.53 + 0.09 * Math.sin(Math.PI / 6);
  const d = Math.min(
    arc(x, y, 0.68, 0.53, 0.09, 180, 390),
    segment(x, y, ex, ey, 0.585, 0.735),
    segment(x, y, 0.585, 0.735, 0.8, 0.735),
  );
  return d <= TWO_STROKE / 2;
};

function sample(x, y) {
  if (roundedSquare(x, y, 0.03, 0.22) > 0) return null;
  const gx = x - DX, gy = y - DY;
  if (inB(gx, gy)) return LETTER_B;
  if (inTwo(gx, gy)) return LETTER_2;
  return TOP.map((c, i) => c + (BOTTOM[i] - c) * y);
}

function render(size) {
  const ss = 4;
  const rgba = Buffer.alloc(size * size * 4);
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let r = 0, g = 0, b = 0, hits = 0;
      for (let sy = 0; sy < ss; sy++) {
        for (let sx = 0; sx < ss; sx++) {
          const c = sample((px + (sx + 0.5) / ss) / size, (py + (sy + 0.5) / ss) / size);
          if (c) { r += c[0]; g += c[1]; b += c[2]; hits++; }
        }
      }
      const o = (py * size + px) * 4;
      if (hits) {
        rgba[o] = Math.round(r / hits);
        rgba[o + 1] = Math.round(g / hits);
        rgba[o + 2] = Math.round(b / hits);
        rgba[o + 3] = Math.round((255 * hits) / (ss * ss));
      }
    }
  }
  return rgba;
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const x of buf) c = CRC_TABLE[(c ^ x) & 255] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
};

function png(size, rgba) {
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) rgba.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function ico(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(images.length, 4);
  let offset = 6 + 16 * images.length;
  const entries = images.map(({ size, data }) => {
    const e = Buffer.alloc(16);
    e[0] = size >= 256 ? 0 : size;
    e[1] = size >= 256 ? 0 : size;
    e.writeUInt16LE(1, 4); // colour planes
    e.writeUInt16LE(32, 6); // bits per pixel
    e.writeUInt32LE(data.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += data.length;
    return e;
  });
  return Buffer.concat([header, ...entries, ...images.map((i) => i.data)]);
}

const here = dirname(fileURLToPath(import.meta.url));
const images = SIZES.map((size) => ({ size, data: png(size, render(size)) }));
writeFileSync(join(here, "bin2.ico"), ico(images));
if (process.argv[2]) writeFileSync(process.argv[2], images.at(-1).data);
console.log(`wrote bin2.ico (${SIZES.join(", ")} px)`);
