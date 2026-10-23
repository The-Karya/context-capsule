#!/usr/bin/env node
// context-capsule — icons: the Karya panther as the Windows and Linux app icon, made from Karya's brand art (the
// same steps as kchat's brand/make-icons.mjs): a white mark on a black rounded tile; the brush-detail mark at
// 256 px and over, the heavier "bold" mark below that (it survives 16–48 px). Writes:
//   capture-rs/windows/icon.ico                     16, 20, 24, 32, 40, 48, 64, 256 (PNG entries; build.rs puts it in the exe)
//   capture-rs/linux/com.thekarya.contextcapsule.png 256 px (the .desktop entry and the AppImage)
// The tray keeps its drawn ring, dot and mic (state at a glance); at 16–24 px the bare mark is a blur.
// Usage: node capture-rs/icons/make-icons.mjs <brand folder with kchat-desktop-icon.png and mark-bold.png>
// (sharp from SHARP=<path to node_modules/sharp>, or from where node finds it).
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

console.log('icons written');
