#!/usr/bin/env node
// Generates the T-Car mark (three ascending signal bars) and the platform icon
// sources, with no image-library dependency.
//
//   node scripts/build-icons.mjs
//   cd apps/desktop && cargo tauri icon icons/icon.png
//
// The mark itself is the supplied artwork, redrawn from its measured geometry
// so it stays sharp at icon sizes (the originals are 250 px:
// assets/logo/tcar-mark-{dark,light}-250.png).
//
// Writes:
//   assets/logo/tcar-mark.svg        transparent, uses currentColor (docs, UI)
//   assets/logo/tcar-mark-light.png  black bars on white, 1024 px
//   assets/logo/tcar-mark-dark.png   white bars on black, 1024 px
//   apps/desktop/icons/icon.png      the source `cargo tauri icon` expands
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// ---- mark geometry, as fractions of the canvas ----------------------------
// Three bars, equal width, ascending left to right, sharing a baseline.
// Measured off the supplied 250 px artwork (assets/logo/*-250.png) so the
// generated 1024 px icons are the same mark, just resolution-independent:
//   bar width 23px, gap 2px, heights 23/37/62px, baseline y=149, corner r=7px.
const BAR_W = 23 / 250;
const GAP = 2 / 250;
const HEIGHTS = [23 / 250, 37 / 250, 62 / 250]; // short, middle, tall
const RADIUS = (7 / 23) * BAR_W; // corner rounding, relative to canvas
const BASELINE = 149.5 / 250; // bottom edge of every bar

function bars(size) {
  const groupW = 3 * BAR_W + 2 * GAP;
  const x0 = (1 - groupW) / 2;
  const bottom = BASELINE * size;
  return HEIGHTS.map((h, i) => {
    const x = (x0 + i * (BAR_W + GAP)) * size;
    const w = BAR_W * size;
    const hh = h * size;
    return { x, y: bottom - hh, w, h: hh, r: RADIUS * size };
  });
}

// ---- SVG ------------------------------------------------------------------
// Tightly cropped to the bars: the padding in the PNG icons is there for the
// platform icon grids, but in a UI the mark should fill the box it is given.
function svg(size = 512) {
  const shapes = bars(size);
  const minX = Math.min(...shapes.map((b) => b.x));
  const maxX = Math.max(...shapes.map((b) => b.x + b.w));
  const minY = Math.min(...shapes.map((b) => b.y));
  const maxY = Math.max(...shapes.map((b) => b.y + b.h));
  const rects = shapes
    .map((b) => ({ ...b, x: b.x - minX, y: b.y - minY }))
    .map(
      (b) =>
        `  <rect x="${b.x.toFixed(1)}" y="${b.y.toFixed(1)}" width="${b.w.toFixed(1)}" ` +
        `height="${b.h.toFixed(1)}" rx="${b.r.toFixed(1)}" />`,
    )
    .join("\n");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${(maxX - minX).toFixed(1)} ${(maxY - minY).toFixed(1)}" fill="currentColor" role="img" aria-label="T-Car">
${rects}
</svg>
`;
}

// ---- PNG ------------------------------------------------------------------
function crc32(buf) {
  let c, crc = 0xffffffff;
  for (let n = 0; n < buf.length; n++) {
    c = (crc ^ buf[n]) & 0xff;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc = c ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

/** Coverage of one pixel by a rounded rectangle, sampled 3x3 for smooth edges. */
function coverage(px, py, b) {
  let hits = 0;
  for (let sy = 0; sy < 3; sy++) {
    for (let sx = 0; sx < 3; sx++) {
      const x = px + (sx + 0.5) / 3;
      const y = py + (sy + 0.5) / 3;
      if (x < b.x || x > b.x + b.w || y < b.y || y > b.y + b.h) continue;
      const dx = Math.min(x - b.x, b.x + b.w - x);
      const dy = Math.min(y - b.y, b.y + b.h - y);
      if (dx < b.r && dy < b.r) {
        const ox = b.r - dx;
        const oy = b.r - dy;
        if (ox * ox + oy * oy > b.r * b.r) continue;
      }
      hits++;
    }
  }
  return hits / 9;
}

/**
 * @param size    canvas edge in pixels
 * @param bg      [r,g,b,a] background, alpha 0 for transparent
 * @param fg      [r,g,b] bar color
 */
function png(size, bg, fg) {
  const shapes = bars(size);
  const rows = [];
  for (let y = 0; y < size; y++) {
    const row = Buffer.alloc(1 + size * 4);
    row[0] = 0; // no filter
    for (let x = 0; x < size; x++) {
      let a = 0;
      for (const b of shapes) {
        if (y + 1 < b.y || y > b.y + b.h || x + 1 < b.x || x > b.x + b.w) continue;
        a = Math.max(a, coverage(x, y, b));
        if (a === 1) break;
      }
      const o = 1 + x * 4;
      // bar color composited over the background
      row[o] = Math.round(bg[0] * (1 - a) + fg[0] * a);
      row[o + 1] = Math.round(bg[1] * (1 - a) + fg[1] * a);
      row[o + 2] = Math.round(bg[2] * (1 - a) + fg[2] * a);
      row[o + 3] = Math.round(bg[3] * (1 - a) + 255 * a);
    }
    rows.push(row);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(Buffer.concat(rows), { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const BLACK = [0, 0, 0];
const WHITE = [255, 255, 255];
const SIZE = 1024;

const out = (...p) => join(root, ...p);
mkdirSync(out("assets", "logo"), { recursive: true });

writeFileSync(out("assets", "logo", "tcar-mark.svg"), svg());
writeFileSync(out("assets", "logo", "tcar-mark-light.png"), png(SIZE, [...WHITE, 255], BLACK));
writeFileSync(out("assets", "logo", "tcar-mark-dark.png"), png(SIZE, [...BLACK, 255], WHITE));
// The platform icon source: white bars on black, matching the dark mark.
writeFileSync(out("apps", "desktop", "icons", "icon.png"), png(SIZE, [...BLACK, 255], WHITE));

console.log("wrote assets/logo/{tcar-mark.svg,tcar-mark-light.png,tcar-mark-dark.png}");
console.log("wrote apps/desktop/icons/icon.png  (run: cd apps/desktop && cargo tauri icon icons/icon.png)");
