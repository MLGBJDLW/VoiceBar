#!/usr/bin/env node
// Builds the Tauri updater manifest (`latest.json`) for a released tag.
//
// Usage:
//   TAURI_SIGNING_PRIVATE_KEY="<base64 key>" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=... \
//     node scripts/make-latest-json.mjs --tag v0.2.0 --repo owner/repo \
//       --bundle target/release/bundle/nsis/VoiceBar_0.2.0_x64-setup.exe \
//       --out latest.json [--notes notes.md]
//
// The updater verifies each artifact with minisign, so the manifest carries the
// *contents* of the `*.sig` file (base64), not a path. Only the NSIS installer is
// offered: it installs per-user under %LOCALAPPDATA%, where a non-elevated
// update can replace files. The MSI needs elevation and is download-only.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));

/** Updater target keys follow `<os>-<arch>[-<installer>]`. */
export const TARGET = 'windows-x86_64-nsis';
export const PLATFORM_KEY = 'windows-x86_64';

export function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    if (!key.startsWith('--')) throw new Error(`unexpected argument: ${key}`);
    const value = argv[i + 1];
    if (value === undefined) throw new Error(`missing value for ${key}`);
    out[key.slice(2)] = value;
  }
  return out;
}

/** `v1.2.3` / `1.2.3` / `1.2.3-beta.1` -> clean semver, or throws. */
export function normalizeVersion(tag) {
  const version = String(tag).trim().replace(/^v/i, '');
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`tag "${tag}" is not a semver tag (expected v1.2.3)`);
  }
  return version;
}

export function installerName(version) {
  const product = process.env.VOICEBAR_PRODUCT_NAME ?? 'VoiceBar';
  return `${product}_${version}_x64-setup.exe`;
}

/**
 * Assembles the manifest body. Kept pure so `tests/update-manifest.test.mjs`
 * can pin the exact shape the Rust side expects.
 */
export function buildManifest({ version, repo, installerName: name, signature, sha256, notes, pubDate }) {
  return {
    version,
    notes: notes ?? '',
    pub_date: pubDate ?? new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
    platforms: {
      [PLATFORM_KEY]: {
        signature,
        url: `https://github.com/${repo}/releases/download/v${version}/${name}`,
        sha256,
      },
    },
    // Extra, ignored by Tauri: lets CI and humans cross-check the artifact.
    [TARGET]: { installer: name },
  };
}

// Signature output must be base64 (the updater decodes it with base64 -> minisign).
export function isSignature(text) {
  return /^[A-Za-z0-9+/=\s]{80,}$/.test(text);
}

// Signs with the project's tauri CLI and returns the .sig contents (base64).
// --app-version binds the version into the signature's trusted comment, so a
// tampered manifest cannot pair a newer version number with an older installer.
export function signFile(bundle, version, tauriCli = join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js')) {
  const key = process.env.TAURI_SIGNING_PRIVATE_KEY;
  if (!key) throw new Error('TAURI_SIGNING_PRIVATE_KEY is required to sign the updater bundle');
  // The CLI takes the key literally (not a path) and writes `<bundle>.sig` next
  // to the bundle; it also prints the signature, so read the file it names.
  const output = execFileSync(
    process.execPath,
    [tauriCli, 'signer', 'sign', '--private-key', key, '--app-version', version, bundle],
    { encoding: 'utf8', env: process.env },
  );
  const sigPath = /signature here:\s*\r?\n\s*(.+\.sig)/.exec(output)?.[1]?.trim() ?? `${bundle}.sig`;
  const signature = readFileSync(sigPath, 'utf8').trim();
  if (!isSignature(signature)) throw new Error(`unexpected signature in ${sigPath}`);
  return signature;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  for (const required of ['tag', 'repo', 'bundle', 'out']) {
    if (!args[required]) throw new Error(`--${required} is required`);
  }
  const version = normalizeVersion(args.tag);
  const expected = installerName(version);
  if (basename(args.bundle) !== expected) {
    throw new Error(`bundle must be named ${expected}, got ${basename(args.bundle)}`);
  }
  const bytes = readFileSync(args.bundle);
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  const signature = signFile(args.bundle, version);
  const manifest = buildManifest({
    version,
    repo: args.repo,
    installerName: expected,
    signature,
    sha256,
    notes: args.notes ? readFileSync(args.notes, 'utf8').trim() : undefined,
    pubDate: args['pub-date'],
  });
  writeFileSync(args.out, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`latest.json: ${version} -> ${manifest.platforms[PLATFORM_KEY].url} (${bytes.length} bytes, sha256 ${sha256.slice(0, 12)}…)`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    main();
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }
}
