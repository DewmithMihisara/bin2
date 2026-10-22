// Draws the bin2 unicorn icon as pixel art and writes assets/bin2.ico.
// Usage: node assets/make-icon.mjs [preview-dir]
// No dependencies. The unicorn is drawn on a 32x32 grid, with a hand-tuned 16x16
// version for small sizes. Every size is scaled with hard nearest-neighbour pixels.

import { writeFileSync, mkdirSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// ---------- palette ----------
const PALETTE = {
  K: [0x24, 0x16, 0x45], // outline
  W: [0xff, 0xff, 0xff], // face
  w: [0xdd, 0xe1, 0xf6], // face shade
  G: [0xff, 0xd2, 0x3f], // horn
  g: [0xe0, 0x9a, 0x00], // horn stripe
  C: [0xff, 0x8f, 0xc8], // pink: ear and cheek
  R: [0xff, 0x4d, 0x6d],
  O: [0xff, 0x9f, 0x1c],
  Y: [0xff, 0xe1, 0x4d],
  L: [0x3d, 0xdc, 0x97],
  B: [0x4c, 0xc9, 0xf0],
  P: [0x9b, 0x5d, 0xe5],
  D: [0x3a, 0x2a, 0x7c], // background, top
  d: [0x2a, 0x1e, 0x5c], // background, bottom
  S: [0xff, 0xf6, 0xb0], // star
  s: [0xff, 0xff, 0xff], // star centre
};
const RAINBOW = ["R", "O", "Y", "L", "B", "P"];

// ---------- the unicorn, as pixel spans ----------
// Each span table maps row -> [firstX, lastX]. The head faces right.
const BIG = {
  n: 32,
  head: {
  10: [15, 21], 11: [14, 22], 12: [13, 23], 13: [13, 24], 14: [13, 25], 15: [13, 26],
  16: [13, 27], 17: [14, 27], 18: [15, 27], 19: [15, 26], 20: [14, 19], 21: [13, 19],
  22: [13, 20], 23: [12, 20], 24: [12, 21], 25: [11, 21], 26: [11, 21], 27: [10, 22],
  28: [10, 22], 29: [10, 22], 30: [10, 22],
  },
  horn: { 3: [26, 26], 4: [25, 26], 5: [24, 25], 6: [23, 25], 7: [22, 24], 8: [21, 24], 9: [20, 23] },
  ear: { 7: [17, 17], 8: [16, 17], 9: [16, 18] },
  maneTop: { 8: [12, 13], 9: [11, 15], 10: [9, 14] }, // forelock rows above the neck line
  maneWidth: 6,
  maneFrom: 11,
  wave: (y) => Math.sin(y / 2.2) > 0.3,
  shadeFrom: 21,
  // Eye with a highlight, ear, cheek, nostril, smile.
  details: [[19, 13, "K"], [20, 13, "W"], [19, 14, "K"], [20, 14, "K"], [17, 9, "C"],
    [19, 17, "C"], [20, 17, "C"], [26, 16, "K"], [23, 18, "K"], [24, 18, "K"]],
  hornStripe: (x, y) => (x - y) % 3 === 0,
  stars: [[6, 5, true], [27, 25, true], [10, 2, false], [29, 9, false], [4, 15, false]],
};

// Hand-drawn 16px version: at this size every pixel has to be placed on purpose.
const SMALL = {
  n: 16,
  head: { 5: [7, 10], 6: [6, 11], 7: [6, 12], 8: [6, 13], 9: [7, 13], 10: [7, 11],
    11: [6, 9], 12: [6, 10], 13: [5, 10], 14: [5, 10], 15: [5, 11] },
  horn: { 1: [13, 13], 2: [12, 13], 3: [11, 12], 4: [10, 11] },
  ear: { 3: [8, 8], 4: [8, 8] },
  maneTop: { 4: [5, 7] },
  maneWidth: 5,
  maneFrom: 5,
  wave: (y) => y % 4 === 1,
  shadeFrom: 11,
  details: [[9, 6, "K"], [8, 4, "C"], [9, 8, "C"], [13, 8, "K"]],
  hornStripe: (x, y) => x === 11 && y === 3,
  stars: [[2, 3, false], [13, 12, false]],
};

const inSpan = (spans, x, y) => spans[y] && x >= spans[y][0] && x <= spans[y][1];

function drawUnicorn(spec) {
  const n = spec.n;
  const grid = Array.from({ length: n }, () => Array(n).fill(null));
  const set = (x, y, c) => { if (x >= 0 && x < n && y >= 0 && y < n) grid[y][x] = c; };

  // Mane: rainbow bands that follow the back of the neck, with a gentle wave.
  for (let y = spec.maneFrom; y < n; y++) {
    if (!spec.head[y]) continue;
    const back = spec.head[y][0];
    const width = spec.maneWidth + (spec.wave(y) ? 1 : 0);
    for (let d = 1; d <= width; d++) set(back - d, y, RAINBOW[Math.min(d - 1, RAINBOW.length - 1)]);
  }
  for (const [y, [x0, x1]] of Object.entries(spec.maneTop)) {
    for (let x = x0; x <= x1; x++) set(x, +y, RAINBOW[Math.min(x1 - x, RAINBOW.length - 1)]);
  }

  // Head and neck, with a soft shade along the throat.
  for (const [y, [x0, x1]] of Object.entries(spec.head)) {
    const shadeWidth = n === 32 ? 2 : 1;
    for (let x = x0; x <= x1; x++) set(x, +y, +y >= spec.shadeFrom && x > x1 - shadeWidth ? "w" : "W");
  }
  for (const [y, [x0, x1]] of Object.entries(spec.ear)) for (let x = x0; x <= x1; x++) set(x, +y, "W");

  // Horn with diagonal stripes.
  for (const [y, [x0, x1]] of Object.entries(spec.horn)) {
    for (let x = x0; x <= x1; x++) set(x, +y, spec.hornStripe(x, +y) ? "g" : "G");
  }

  for (const [x, y, c] of spec.details) set(x, y, c);
  return grid;
}

// ---------- tile, outline and stars ----------
function inTile(x, y, n) {
  const cut = [3, 2, 1].map((c) => Math.round((c * n) / 32));
  const edge = Math.min(y, n - 1 - y);
  const c = edge < 3 ? cut[edge] : 0;
  return x >= c && x <= n - 1 - c;
}

function compose(figure, spec) {
  const n = spec.n;
  const out = Array.from({ length: n }, (_, y) =>
    Array.from({ length: n }, (_, x) => (inTile(x, y, n) ? (y < n / 2 ? "D" : "d") : null)),
  );
  const star = (cx, cy, big) => {
    if (big) for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) out[cy + dy][cx + dx] = "S";
    out[cy][cx] = big ? "s" : "S";
  };
  for (const [x, y, big] of spec.stars) star(x, y, big);

  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (figure[y][x]) out[y][x] = figure[y][x];
  // One-pixel outline around the figure, kept inside the tile.
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      if (figure[y][x] || !inTile(x, y, n)) continue;
      const near = [[1, 0], [-1, 0], [0, 1], [0, -1]].some(([dx, dy]) => figure[y + dy]?.[x + dx]);
      if (near) out[y][x] = "K";
    }
  }
  return out;
}

function rasterize(grid, size) {
  const n = grid.length;
  const scale = size / n;
  const rgba = Buffer.alloc(size * size * 4);
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      const c = grid[Math.floor(py / scale)][Math.floor(px / scale)];
      if (!c) continue;
      const o = (py * size + px) * 4;
      [rgba[o], rgba[o + 1], rgba[o + 2]] = PALETTE[c];
      rgba[o + 3] = 255;
    }
  }
  return rgba;
}

// ---------- PNG and ICO encoding ----------
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

// ---------- build ----------
const grid32 = compose(drawUnicorn(BIG), BIG);
const grid16 = compose(drawUnicorn(SMALL), SMALL);
// Each size uses whichever grid divides it evenly, so pixels stay perfectly square.
const SIZES = [
  [16, grid16], [32, grid32], [48, grid16], [64, grid32], [128, grid32], [256, grid32],
];
const images = SIZES.map(([size, grid]) => ({ size, data: png(size, rasterize(grid, size)) }));

const here = dirname(fileURLToPath(import.meta.url));
writeFileSync(join(here, "bin2.ico"), ico(images));
if (process.argv[2]) {
  mkdirSync(process.argv[2], { recursive: true });
  for (const { size, data } of images) writeFileSync(join(process.argv[2], `icon-${size}.png`), data);
  // Big previews of the small grids, to judge them by eye.
  writeFileSync(join(process.argv[2], "preview-16.png"), png(256, rasterize(grid16, 256)));
}
console.log(`wrote bin2.ico (${SIZES.map(([s]) => s).join(", ")} px)`);
