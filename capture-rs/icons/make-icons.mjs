#!/usr/bin/env node
// context-capsule — icons: the Karya panther as the Windows and Linux app icon, made from Karya's brand art (the
// same steps as kchat's brand/make-icons.mjs): a white mark on a black rounded tile; the brush-detail mark at
// 256 px and over, the heavier "bold" mark below that (it survives 16–48 px). Writes:
//   capture-rs/windows/icon.ico                     16, 20, 24, 32, 40, 48, 64, 256 (PNG entries; build.rs puts it in the exe)
//   capture-rs/linux/com.thekarya.contextcapsule.png 256 px (the .desktop entry and the AppImage)
//   capture-rs/core/src/mark_data.rs                the idle tray icon: the bare mark (no tile) as coverage masks at
//                                                   16, 20, 24, 32, 40, 48 px; the app colours it for the taskbar
// The tray's 16, 20 and 24 px marks are drawn by hand below (pure on/off pixels: two ears, the eye, the nose, the brush
// arc and the sweep — a scaled-down mark is a blur there); 32 px and up come from mark-bold.png, crisped.
// Recording (red dot) and listening (mic) stay drawn in code.
// Usage: node capture-rs/icons/make-icons.mjs <brand folder with kchat-desktop-icon.png and mark-bold.png>
// (sharp from SHARP=<path to node_modules/sharp>, or from where node finds it). SHEET=<file.png> also writes a
// sheet of the tray marks (1× and 4×, on a dark and a light taskbar) to look at.
import { writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ws = dirname(here);
const brand = process.argv[2];
if (!brand) throw new Error('usage: node make-icons.mjs <brand folder>');
const sharp = createRequire(import.meta.url)(process.env.SHARP || 'sharp');

// The bold mark: black on transparent, square. Trim to the drawing so it can be placed exactly.
const bold = await sharp(join(brand, 'mark-bold.png')).trim().png().toBuffer();
const full = join(brand, 'kchat-desktop-icon.png'); // 1024 px: the tile with the brush mark

const S = 1024;
const tile = Buffer.from(`<svg width="${S}" height="${S}"><rect width="${S}" height="${S}" rx="${Math.round(S * 0.22)}" fill="#0a0a0a"/></svg>`);
// White mark of `side` px (fit inside, centred) from the bold mark's alpha.
const whiteMark = async (side) => {
  const alpha = await sharp(bold).resize(side, side, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } }).extractChannel('alpha').toBuffer();
  return sharp({ create: { width: side, height: side, channels: 3, background: '#fafafa' } }).joinChannel(alpha).png().toBuffer();
};
const inner = 738; // 72% of the tile, even so it sits exactly in the middle
const boldTile = await sharp(tile).composite([{ input: await whiteMark(inner), left: (S - inner) / 2, top: (S - inner) / 2 }]).png().toBuffer();
const at = (size) => sharp(size >= 256 ? full : boldTile).resize(size, size, { kernel: 'lanczos3' }).png({ compressionLevel: 9 }).toBuffer();

// Windows .ico: PNG entries (Windows Vista and later read them everywhere).
const sizes = [16, 20, 24, 32, 40, 48, 64, 256];
const pngs = [];
for (const s of sizes) pngs.push(await at(s));
const head = Buffer.alloc(6 + 16 * sizes.length);
head.writeUInt16LE(0, 0);
head.writeUInt16LE(1, 2);
head.writeUInt16LE(sizes.length, 4);
let offset = head.length;
sizes.forEach((s, i) => {
  const o = 6 + i * 16;
  head.writeUInt8(s >= 256 ? 0 : s, o);
  head.writeUInt8(s >= 256 ? 0 : s, o + 1);
  head.writeUInt16LE(1, o + 4);
  head.writeUInt16LE(32, o + 6);
  head.writeUInt32LE(pngs[i].length, o + 8);
  head.writeUInt32LE(offset, o + 12);
  offset += pngs[i].length;
});
writeFileSync(join(ws, 'windows', 'icon.ico'), Buffer.concat([head, ...pngs]));

// Linux: the app icon.
writeFileSync(join(ws, 'linux', 'com.thekarya.contextcapsule.png'), await at(256));

// The tray mark. Hand-drawn at 16, 20 and 24 px ('#' on, '.' off), the panther facing right as in the brand mark.
const HAND = {
  16: [
    '........#...#...',
    '.......##.###...',
    '......#.#####...',
    '.....#..#####...',
    '....#..####.##..',
    '...#..########..',
    '...#..#########.',
    '..##.#####.##...',
    '..##.######.....',
    '..##.##.#####...',
    '..#..#.#######..',
    '......########..',
    '......####...#..',
    '.....####.......',
    '.....###........',
    '.....##.........',
  ],
  20: [
    '.........#....#.....',
    '.........##.####....',
    '.......#.#######....',
    '......##.########...',
    '.....##.#########...',
    '....##..#####.###...',
    '....#..###########..',
    '......############..',
    '...#..##########.#..',
    '..##..##########....',
    '..##.#####.####.....',
    '..##.####.#####.....',
    '..##.###.#######....',
    '...#.##.#######.....',
    '.....#..#########...',
    '.......#####....#...',
    '.......####.....#...',
    '.......###..........',
    '......###...........',
    '......##............',
  ],
  24: [
    '............#...........',
    '...........###...##.....',
    '.........##.###.###.....',
    '........###.###.###.....',
    '.......###.####..###....',
    '......###.##########....',
    '......##..#######.##....',
    '......#..###########....',
    '........#############...',
    '.....#..###########.....',
    '....##.#######.#####....',
    '...###.######.####......',
    '...##..#####.#####......',
    '...##..####.#######.....',
    '...##..###.########.....',
    '....#..##..#########....',
    '.......#..######.###....',
    '..........####....##....',
    '.........####......#....',
    '.........####......#....',
    '........####............',
    '........###.............',
    '........##..............',
    '........................',
  ],
};
const TRAY = [16, 20, 24, 32, 40, 48];
// Coverage 0…255, size×size, the drawing centred (bounding box; odd spare pixel on the right / bottom).
const centre = (cov, size) => {
  let [x0, y0, x1, y1] = [size, size, -1, -1];
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) if (cov[y * size + x]) {
    x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
  }
  const dx = Math.floor((size - (x1 - x0 + 1)) / 2) - x0, dy = Math.floor((size - (y1 - y0 + 1)) / 2) - y0;
  const out = new Uint8Array(size * size);
  for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) out[(y + dy) * size + x + dx] = cov[y * size + x];
  return out;
};
const hand = (size) => {
  const rows = HAND[size];
  if (rows.length !== size || rows.some((r) => r.length !== size || /[^#.]/.test(r))) throw new Error(`hand mark ${size}: ${size} rows of ${size}`);
  return centre(Uint8Array.from(rows.join(''), (c) => (c === '#' ? 255 : 0)), size);
};
// From the bold mark: drawn 16× larger, softened by a quarter pixel (drops the brush's specks), cut at half
// coverage, averaged down, then a steeper edge (10%…75% → 0…255) so it stays crisp with a little smoothing.
// A pixel of margin per 16 px.
const K = 16;
const fromArt = async (size) => {
  const h = size - Math.floor(size / 16);
  const H = h * K;
  const meta = await sharp(bold).metadata();
  const W = Math.round((meta.width * H) / meta.height);
  const a = await sharp(bold).resize(W, H, { kernel: 'lanczos3' }).extractChannel('alpha').blur(0.25 * K).raw().toBuffer();
  const sum = new Float64Array(size * size);
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) if (a[y * W + x] >= 128) sum[Math.floor(y / K) * size + Math.floor(x / K)] += 1 / (K * K);
  return centre(Uint8Array.from(sum, (c) => Math.round(255 * Math.min(1, Math.max(0, (c - 0.1) / 0.65)))), size);
};
const marks = [];
for (const s of TRAY) marks.push([s, HAND[s] ? hand(s) : await fromArt(s)]);
const hex = (v) => `0x${v.toString(16).padStart(2, '0')}`;
let rs = `// context-capsule — core: the Karya panther for the idle tray icon, made by capture-rs/icons/make-icons.mjs
// (don't edit): coverage 0…255 of a size×size icon, top row first, at the sizes the tray uses at 100–300 %.
// 16, 20 and 24 px are drawn by hand; 32 px and up come from Karya's bold mark. See \`mark\`.

pub(crate) const MARKS: [(usize, &[u8]); ${TRAY.length}] = [\n`;
for (const [s, m] of marks) {
  rs += `    (\n        ${s},\n        &[\n`;
  for (let y = 0; y < s; y++) rs += `            ${Array.from(m.subarray(y * s, (y + 1) * s), hex).join(', ')},\n`;
  rs += '        ],\n    ),\n';
}
rs += '];\n';
writeFileSync(join(ws, 'core', 'src', 'mark_data.rs'), rs);

// A sheet to look at: each size at 1× and 4× (nearest neighbour) on a dark and a light taskbar.
if (process.env.SHEET) {
  const Z = 4, pad = 12;
  const tiles = [];
  let y = pad, width = 0;
  for (const [s, m] of marks) {
    let x = pad;
    for (const [bg, fg] of [['#1f1f1f', 255], ['#f3f3f3', 0x20]]) {
      for (const z of [1, Z]) {
        const px = Buffer.alloc(s * s * 4);
        m.forEach((a, i) => px.set([fg, fg, fg, a], i * 4));
        const art = await sharp(px, { raw: { width: s, height: s, channels: 4 } }).resize(s * z, s * z, { kernel: 'nearest' }).png().toBuffer();
        const side = Math.max(z * s, 48) + 2 * pad;
        tiles.push({ input: await sharp({ create: { width: side, height: Z * s + 2 * pad, channels: 4, background: bg } }).composite([{ input: art, left: pad, top: pad }]).png().toBuffer(), left: x, top: y });
        x += side + pad;
      }
    }
    width = Math.max(width, x);
    y += Z * s + 3 * pad;
  }
  await sharp({ create: { width, height: y, channels: 4, background: '#808080' } }).composite(tiles).png().toFile(process.env.SHEET);
}

console.log('icons written');
