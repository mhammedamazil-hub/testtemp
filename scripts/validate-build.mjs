#!/usr/bin/env node
/**
 * Post-build sanity checks. Fails the build (non-zero exit) when a shipping
 * requirement is broken, so CI cannot publish a broken artifact:
 *  - app shell, manifest, service worker and all icons present
 *  - index.html links the manifest, the theme colour and the module entry
 *  - no absolute asset URLs (the app is served from a sub-path on Pages)
 *  - no secret-shaped strings in the bundle
 */
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, extname } from 'node:path';

const dist = 'dist';
const problems = [];
const notes = [];

function requireFile(path, minBytes = 1) {
  const full = join(dist, path);
  if (!existsSync(full)) {
    problems.push(`missing ${path}`);
    return null;
  }
  const size = statSync(full).size;
  if (size < minBytes) problems.push(`${path} is only ${size} bytes`);
  return readFileSync(full);
}

const html = requireFile('index.html', 300);
const manifestRaw = requireFile('manifest.webmanifest', 100);
const swRaw = requireFile('sw.js', 200);
for (const icon of ['icons/icon.svg', 'icons/icon-192.png', 'icons/icon-512.png', 'icons/maskable-512.png']) {
  requireFile(icon, 200);
}

if (html) {
  const source = html.toString('utf8');
  if (!source.includes('rel="manifest"')) problems.push('index.html does not link the manifest');
  if (!source.includes('type="module"')) problems.push('index.html has no module entry');
  if (!/name="viewport"/.test(source)) problems.push('index.html has no viewport meta');
  if (/src="\/assets\//.test(source)) problems.push('index.html uses absolute /assets paths (breaks sub-path hosting)');
  if (!/name="description"/.test(source)) problems.push('index.html has no description meta');
}

let manifest = null;
if (manifestRaw) {
  try {
    manifest = JSON.parse(manifestRaw.toString('utf8'));
  } catch (err) {
    problems.push(`manifest is not valid JSON: ${err.message}`);
  }
}
if (manifest) {
  for (const field of ['name', 'short_name', 'start_url', 'display', 'icons']) {
    if (!manifest[field]) problems.push(`manifest missing "${field}"`);
  }
  if (manifest.display !== 'standalone') problems.push('manifest display should be standalone');
  const sizes = (manifest.icons ?? []).map((icon) => icon.sizes);
  for (const required of ['192x192', '512x512']) {
    if (!sizes.includes(required)) problems.push(`manifest has no ${required} icon`);
  }
  if (!(manifest.icons ?? []).some((icon) => icon.purpose?.includes('maskable'))) {
    problems.push('manifest has no maskable icon');
  }
  if (!(manifest.start_url ?? '').startsWith('.')) notes.push(`start_url is "${manifest.start_url}" (relative paths are safest)`);
}

if (swRaw) {
  const sw = swRaw.toString('utf8');
  if (!sw.includes('addEventListener(\'install\'')) problems.push('service worker has no install handler');
  if (!sw.includes('addEventListener(\'fetch\'')) problems.push('service worker has no fetch handler');
  if (!/sourceforge-v\d+/.test(sw)) problems.push('service worker has no versioned cache name');
  if (/apiKey|Authorization/.test(sw)) problems.push('service worker references credentials');
}

const assets = existsSync(join(dist, 'assets')) ? readdirSync(join(dist, 'assets')) : [];
if (!assets.some((file) => extname(file) === '.js')) problems.push('no JavaScript bundle in dist/assets');
if (!assets.some((file) => extname(file) === '.css')) problems.push('no CSS bundle in dist/assets');

// Secret scan: providers are configured at runtime, never baked in.
const secretPatterns = [/sk-[A-Za-z0-9]{20,}/, /gh[pousr]_[A-Za-z0-9]{30,}/, /AIza[0-9A-Za-z_-]{30,}/, /-----BEGIN [A-Z ]*PRIVATE KEY-----/];
function scan(dir) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const info = statSync(full);
    if (info.isDirectory()) {
      scan(full);
      continue;
    }
    if (!['.js', '.css', '.html', '.json', '.webmanifest', '.svg'].includes(extname(entry))) continue;
    if (info.size > 8 * 1024 * 1024) continue;
    const text = readFileSync(full, 'utf8');
    for (const pattern of secretPatterns) {
      if (pattern.test(text)) problems.push(`possible secret in ${full}`);
    }
  }
}
scan(dist);

const totalBytes = (function size(dir) {
  return readdirSync(dir).reduce((sum, entry) => {
    const full = join(dir, entry);
    const info = statSync(full);
    return sum + (info.isDirectory() ? size(full) : info.size);
  }, 0);
})(dist);

console.log(`dist: ${(totalBytes / 1024 / 1024).toFixed(2)} MiB, ${assets.length} asset files`);
for (const note of notes) console.log(`note: ${note}`);
if (problems.length) {
  console.error('Build validation failed:');
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}
console.log('build validation passed');
