// Generates app-icon.png (1024x1024) with no external dependencies.
// A rounded dark tile with a ">_" terminal prompt in the brand gradient.
// Feed the result to `npm run tauri icon ./app-icon.png` to emit the icon set.
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";

const S = 1024;

/* ---- PNG plumbing ---- */
const crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = crcTable[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}

/* ---- tiny vector helpers ---- */
const clamp = (x, a, b) => Math.max(a, Math.min(b, x));
const mix = (a, b, t) => a + (b - a) * t;
const smooth = (e0, e1, x) => {
  const t = clamp((x - e0) / (e1 - e0), 0, 1);
  return t * t * (3 - 2 * t);
};
function distSeg(px, py, ax, ay, bx, by) {
  const dx = bx - ax,
    dy = by - ay;
  const l2 = dx * dx + dy * dy;
  let t = l2 === 0 ? 0 : ((px - ax) * dx + (py - ay) * dy) / l2;
  t = clamp(t, 0, 1);
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}
// signed distance to a rounded rect centred in the canvas (negative inside)
function roundedRectSD(x, y, margin, r) {
  const halfw = (S - 2 * margin) / 2;
  const halfh = (S - 2 * margin) / 2;
  const cx = S / 2,
    cy = S / 2;
  const dx = Math.abs(x - cx) - (halfw - r);
  const dy = Math.abs(y - cy) - (halfh - r);
  const outside = Math.hypot(Math.max(dx, 0), Math.max(dy, 0));
  const inside = Math.min(Math.max(dx, dy), 0);
  return outside + inside - r;
}

const hex = (h) => [
  parseInt(h.slice(1, 3), 16),
  parseInt(h.slice(3, 5), 16),
  parseInt(h.slice(5, 7), 16),
];
const TILE_TOP = hex("#1b1f2b");
const TILE_BOT = hex("#0c0d12");
const ACC_A = hex("#8b5cff"); // indigo
const ACC_B = hex("#22d3ee"); // cyan

// glyph geometry (normalised -> px)
const P = (nx, ny) => [nx * S, ny * S];
const chevA = P(0.30, 0.30);
const chevM = P(0.52, 0.5);
const chevB = P(0.30, 0.70);
const underA = P(0.55, 0.665);
const underB = P(0.74, 0.665);
const STROKE = 46; // half-width

const raw = Buffer.alloc(S * (1 + S * 4));

for (let y = 0; y < S; y++) {
  const rowStart = y * (1 + S * 4);
  raw[rowStart] = 0; // filter: none
  for (let x = 0; x < S; x++) {
    // tile mask (with 1.5px AA edge)
    const sd = roundedRectSD(x + 0.5, y + 0.5, 40, 190);
    const tileA = 1 - smooth(-1.5, 1.5, sd);

    // base tile: vertical gradient + soft top sheen
    const vg = y / S;
    let r = mix(TILE_TOP[0], TILE_BOT[0], vg);
    let g = mix(TILE_TOP[1], TILE_BOT[1], vg);
    let b = mix(TILE_TOP[2], TILE_BOT[2], vg);
    const sheen = Math.max(0, 1 - Math.hypot((x - S * 0.5) / S, (y + S * 0.15) / S) * 1.6) * 18;
    r += sheen;
    g += sheen;
    b += sheen;

    // glyph distance (chevron + underscore cursor)
    const dGlyph = Math.min(
      distSeg(x, y, chevA[0], chevA[1], chevM[0], chevM[1]),
      distSeg(x, y, chevM[0], chevM[1], chevB[0], chevB[1]),
      distSeg(x, y, underA[0], underA[1], underB[0], underB[1])
    );
    const glyphA = 1 - smooth(STROKE - 1.5, STROKE + 1.5, dGlyph);

    if (glyphA > 0) {
      // accent gradient along the diagonal
      const t = clamp((x + y) / (S * 1.4), 0, 1);
      const gr = mix(ACC_A[0], ACC_B[0], t);
      const gg = mix(ACC_A[1], ACC_B[1], t);
      const gb = mix(ACC_A[2], ACC_B[2], t);
      r = mix(r, gr, glyphA);
      g = mix(g, gg, glyphA);
      b = mix(b, gb, glyphA);
    }

    const o = rowStart + 1 + x * 4;
    raw[o] = clamp(Math.round(r), 0, 255);
    raw[o + 1] = clamp(Math.round(g), 0, 255);
    raw[o + 2] = clamp(Math.round(b), 0, 255);
    raw[o + 3] = Math.round(clamp(tileA, 0, 1) * 255);
  }
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(S, 0);
ihdr.writeUInt32BE(S, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // colour type RGBA
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

writeFileSync(new URL("../app-icon.png", import.meta.url), png);
console.log(`wrote app-icon.png (${S}x${S}, ${png.length} bytes)`);
