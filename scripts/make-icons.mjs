#!/usr/bin/env node
/**
 * Generate the PWA raster icons from code (no image dependencies).
 *
 * The mark is three stacked bars on a rounded navy tile: a forge/stack motif
 * that stays legible at 48px. Output: public/icons/icon-192.png,
 * icon-512.png, maskable-512.png and apple-touch-icon.png.
 */
import { deflateSync } from 'node:zlib';
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const icons = [
  { file: 'public/icons/icon-192.png', size: 192, maskable: false },
  { file: 'public/icons/icon-512.png', size: 512, maskable: false },
  { file: 'public/icons/maskable-512.png', size: 512, maskable: true },
  { file: 'public/icons/apple-touch-icon.png', size: 180, maskable: false },
];

const crcTable = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

function encodePng(width, height, rgba) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * (stride + 1)] = 0; // no filter
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const clamp01 = (n) => Math.max(0, Math.min(1, n));
const mix = (a, b, t) => a.map((value, i) => Math.round(value + (b[i] - value) * t));

function roundedRectAlpha(x, y, size, inset, radius) {
  const left = inset;
  const top = inset;
  const right = size - inset;
  const bottom = size - inset;
  if (x < left || y < top || x > right || y > bottom) return 0;
  const cx = Math.min(Math.max(x, left + radius), right - radius);
  const cy = Math.min(Math.max(y, top + radius), bottom - radius);
  const dist = Math.hypot(x - cx, y - cy);
  return clamp01(radius - dist + 0.5);
}

function drawIcon(size, maskable) {
  const px = Buffer.alloc(size * size * 4);
  const inset = maskable ? 0 : size * 0.055;
  const radius = maskable ? size * 0.5 : size * 0.22;
  const bgTop = [17, 26, 44];
  const bgBottom = [10, 15, 26];
  const barColors = [
    [86, 140, 255],
    [112, 164, 255],
    [150, 130, 255],
  ];

  // Bars: centred stack, widths 58%, 44%, 30% of the tile.
  const barHeight = size * 0.115;
  const gap = size * 0.075;
  const bars = [
    { width: size * 0.58, cy: size * 0.62 },
    { width: size * 0.44, cy: size * 0.62 - barHeight - gap },
    { width: size * 0.3, cy: size * 0.62 - 2 * (barHeight + gap) },
  ];

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const bgAlpha = roundedRectAlpha(x, y, size, inset, radius);
      if (bgAlpha <= 0) continue;
      const t = (x / size) * 0.35 + (y / size) * 0.65;
      let color = mix(bgTop, bgBottom, clamp01(t));

      // subtle diagonal sheen
      const sheen = clamp01(1 - Math.abs((x + y) / size - 0.9) * 2.2) * 0.1;
      color = mix(color, [46, 96, 210], sheen);

      for (const [index, bar] of bars.entries()) {
        const barRadius = barHeight / 2;
        const left = size / 2 - bar.width / 2;
        const right = size / 2 + bar.width / 2;
        const top = bar.cy - barHeight / 2;
        const bottom = bar.cy + barHeight / 2;
        if (x < left - 1 || x > right + 1 || y < top - 1 || y > bottom + 1) continue;
        const cx = Math.min(Math.max(x, left + barRadius), right - barRadius);
        const cy = Math.min(Math.max(y, top + barRadius), bottom - barRadius);
        const alpha = clamp01(barRadius - Math.hypot(x - cx, y - cy) + 0.5);
        if (alpha > 0) {
          const gradient = clamp01((x - left) / Math.max(1, bar.width));
          const barColor = mix(barColors[index], barColors[Math.min(index + 1, barColors.length - 1)], gradient);
          color = mix(color, barColor, alpha);
        }
      }

      const offset = (y * size + x) * 4;
      px[offset] = color[0];
      px[offset + 1] = color[1];
      px[offset + 2] = color[2];
      px[offset + 3] = Math.round(bgAlpha * 255);
    }
  }
  return encodePng(size, size, px);
}

for (const icon of icons) {
  mkdirSync(dirname(icon.file), { recursive: true });
  const png = drawIcon(icon.size, icon.maskable);
  writeFileSync(icon.file, png);
  console.log(`${icon.file} — ${icon.size}×${icon.size}, ${png.length} bytes`);
}
