// Draws the bin2 unicorn icon as pixel art and writes assets/bin2.ico.
// Usage: node assets/make-icon.mjs [preview-dir]
// No dependencies. The unicorn is drawn on a 32x32 grid, with a hand-tuned 16x16
// version for small sizes. Every size is scaled with hard nearest-neighbour pixels.
// The background is transparent, so the icon sits on any wallpaper or folder view.

import { writeFileSync, mkdirSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// ---------- palette ----------
const PALETTE = {
  K: [0x10, 0x0c, 0x18], // outline
  W: [0xff, 0xff, 0xff], // face
  c: [0xff, 0xe0, 0xf4], // face shade, next to the mane
  M: [0xff, 0xc8, 0xf0], // muzzle
  H: [0xf7, 0x8f, 0xe0], // horn
  h: [0xff, 0xc8, 0xf4], // horn highlight
  E: [0xf7, 0x8f, 0xe0], // inner ear
  // Mane, from the neck outwards.
  R: [0xff, 0x1f, 0x1f],
  O: [0xff, 0x8a, 0x00],
  Y: [0xff, 0xe8, 0x00],
  L: [0x22, 0xe0, 0x22],
  A: [0x14, 0xb4, 0xff],
  U: [0x22, 0x2c, 0xf0],
  V: [0x86, 0x22, 0xe8],
};
const RAINBOW = ["R", "O", "Y", "L", "A", "U", "V"];

// ---------- the unicorn ----------
// Span tables map row -> [firstX, lastX]. The head faces left, the mane sweeps round to the right.
const BIG = {
  n: 32,
  horn: { 2: [7, 7], 3: [7, 8], 4: [8, 9], 5: [8, 10], 6: [9, 11], 7: [9, 12] },
  hornHighlight: (x, y) => x === Math.floor((y + 13) / 2) && y >= 3,
  ear: { 4: [15, 15], 5: [15, 16], 6: [15, 16], 7: [15, 16] },
  innerEar: [[16, 6], [16, 7]],
  jawFrom: 17,
  head: {
    8: [10, 17], 9: [9, 17], 10: [8, 17], 11: [6, 17], 12: [5, 17], 13: [4, 17],
    14: [3, 16], 15: [3, 16], 16: [3, 15], 17: [4, 14], 18: [5, 12],
  },
  muzzle: (x, y) => y >= 12 && x <= (y >= 14 ? 9 : 8),
  shade: (x, y, x1) => x === x1 && y >= 9,
  details: [[12, 12, "K"], [13, 12, "K"], [12, 13, "K"], [13, 13, "K"], // eye
    [5, 14, "K"], // nostril
    [7, 17, "K"], [8, 17, "K"]], // smile
  mane: {
    top: { 1: 18, 2: 18, 3: 18, 4: 18, 5: 18, 6: 18, 7: 18 }, // inner edge above the head line, behind the ear
    neck: [19, 12, 30, 7], // inner edge below the jaw: from (x12, y19) down to (x7, y30)
    cx: 14, cy: 15, rx: 14, ryUp: 14, ryDown: 16, // outer edge
    cut: [7, 30, 26, 22], // bottom edge, a diagonal from the tail tip up to the right
    lastRow: 30,
  },
};

// Hand-placed 16px version. At this size every pixel is chosen on purpose.
// . transparent, K outline, W face, M muzzle, H horn, then the mane colours.
const SMALL_ART = [
  "..K.............",
  ".KHK......KKKK..",
  ".KHHK...KKROYAK.",
  "..KHHKKKWKROYAUK",
  "...KWWWWWROYLAUK",
  "..KWWWWWWROYLAUK",
  ".KMMWKWWWROYLAUK",
  "KMKMMWWWWROYLAUK",
  "KMMMMWWWROYLAUVK",
  ".KMMMWWKROYLAUVK",
  "..KKKKKROYLAUVK.",
  "......KROYLAUK..",
  ".....KROYLAKK...",
  "....KRROYKK.....",
  "...KRROKK.......",
  "...KRKK.........",
];
const fromArt = (rows) => rows.map((r) => [...r].map((c) => (c === "." ? null : c)));

const lerp = (a, b, t) => a + (b - a) * t;

function drawUnicorn(spec) {
  const { n, mane } = spec;
  const grid = Array.from({ length: n }, () => Array(n).fill(null));
  const set = (x, y, c) => { if (x >= 0 && x < n && y >= 0 && y < n) grid[y][x] = c; };

  // Mane: a rainbow crescent. Each row runs from an inner edge (the head, or the neck line
  // under the jaw) to an elliptical outer edge, cut off below by a diagonal tail line.
  const [ny0, nx0, ny1, nx1] = mane.neck;
  const [tx0, ty0, tx1, ty1] = mane.cut;
  for (let y = 0; y <= mane.lastRow; y++) {
    let inner;
    if (spec.head[y]) inner = spec.head[y][1] + 1;
    else if (mane.top[y] !== undefined) inner = mane.top[y];
    else if (y >= ny0) inner = Math.round(lerp(nx0, nx1, (y - ny0) / (ny1 - ny0)));
    else continue;
    const ry = y < mane.cy ? mane.ryUp : mane.ryDown;
    const k = 1 - ((y - mane.cy) / ry) ** 2;
    if (k <= 0) continue;
    const outer = Math.round(mane.cx + mane.rx * Math.sqrt(k));
    for (let x = inner; x <= outer; x++) {
      const maxY = ty0 + ((x - tx0) * (ty1 - ty0)) / (tx1 - tx0);
      if (y > maxY + 0.5) continue;
      const f = (x - inner) / Math.max(1, outer - inner + 1);
      set(x, y, RAINBOW[Math.min(RAINBOW.length - 1, Math.floor(f * RAINBOW.length))]);
    }
  }

  // Head, muzzle and a soft pink edge where the face meets the mane.
  for (const [ys, [x0, x1]] of Object.entries(spec.head)) {
    const y = +ys;
    for (let x = x0; x <= x1; x++) {
      set(x, y, spec.muzzle(x, y) ? "M" : spec.shade(x, y, x1) ? "c" : "W");
    }
  }
  for (const [y, [x0, x1]] of Object.entries(spec.ear)) for (let x = x0; x <= x1; x++) set(x, +y, "W");
  for (const [x, y] of spec.innerEar) set(x, y, "E");
  // Outline the back of the ear so it stands in front of the mane.
  for (const [y, [, x1]] of Object.entries(spec.ear)) set(x1 + 1, +y, "K");
  for (const [y, [x0, x1]] of Object.entries(spec.horn)) {
    for (let x = x0; x <= x1; x++) set(x, +y, spec.hornHighlight(x, +y) ? "h" : "H");
  }

  // The jaw line: a dark edge wherever the mane sits directly under the lower face.
  const isFace = (c) => c === "W" || c === "M" || c === "c";
  for (let y = n - 1; y >= spec.jawFrom; y--) {
    for (let x = 0; x < n; x++) {
      if (RAINBOW.includes(grid[y][x]) && isFace(grid[y - 1][x])) grid[y][x] = "K";
    }
  }

  for (const [x, y, c] of spec.details) set(x, y, c);
  return grid;
}

// ---------- outline ----------
function compose(figure) {
  const n = figure.length;
  const out = figure.map((row) => row.slice());
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      if (figure[y][x]) continue;
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
const grid32 = compose(drawUnicorn(BIG));
const grid16 = fromArt(SMALL_ART);
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
  // Big preview of the small grid, to judge it by eye.
  writeFileSync(join(process.argv[2], "preview-16.png"), png(256, rasterize(grid16, 256)));
}
console.log(`wrote bin2.ico (${SIZES.map(([s]) => s).join(", ")} px)`);
