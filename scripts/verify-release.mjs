#!/usr/bin/env node
// Validates a release before it is published: every asset Tauri needs must be
// present, and `latest.json` must point at *this* release's installer with a
// matching sha256. Used by .github/workflows/release.yml (publish job) and
// covered by tests/update-manifest.test.mjs.
//
// Usage:
//   node scripts/verify-release.mjs --manifest latest.json --assets-dir dist-assets \
//     --tag v0.2.0 --repo owner/repo
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from './make-latest-json.mjs';

export const REQUIRED_SUFFIXES = ['x64-setup.exe', 'x64_en-US.msi'];
export const TARGET = 'windows-x86_64';

/**
 * Pure checks over the manifest plus the byte content found in `assets`.
 * `assets` is a Map of file name -> Buffer (read by the caller), so the same
 * function works against a downloaded directory or an in-memory test fixture.
 */
export function verifyRelease({ manifest, assets, tag, repo }) {
  const problems = [];
  for (const suffix of REQUIRED_SUFFIXES) {
    if (![...assets.keys()].some((name) => name.endsWith(suffix))) problems.push(`缺少资产：*${suffix}`);
  }
  if (!manifest || typeof manifest !== 'object') {
    problems.push('latest.json 为空或不是 JSON 对象');
    return problems;
  }
  const version = String(tag).replace(/^v/i, '');
  if (manifest.version !== version) problems.push(`latest.json version ${manifest.version} != 标签 ${tag}`);
  const platform = manifest.platforms?.[TARGET];
  if (!platform) {
    problems.push(`latest.json 缺少平台 ${TARGET}（应用内更新在此平台会找不到更新包）`);
    return problems;
  }
  if (!platform.signature || !/^[A-Za-z0-9+/=\s]{80,}$/.test(platform.signature)) {
    problems.push('latest.json 的 signature 不是有效的 base64 签名');
  }
  const expectedPrefix = `https://github.com/${repo}/releases/download/v${version}/`;
  if (!String(platform.url ?? '').startsWith(expectedPrefix)) {
    problems.push(`latest.json 的下载地址应指向 ${expectedPrefix}`);
    return problems;
  }
  // A freshly published release must not leave older assets behind: the URL has
  // to resolve to a file in *this* release.
  const name = decodeURIComponent(String(platform.url).split('/').pop());
  const bytes = assets.get(name);
  if (!bytes) {
    problems.push(`latest.json 指向的安装包不在本次发布中：${name}`);
    return problems;
  }
  const actual = createHash('sha256').update(bytes).digest('hex');
  if (platform.sha256 !== actual) problems.push(`sha256 不匹配：${actual} != ${platform.sha256}`);
  return problems;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  for (const required of ['manifest', 'assets-dir', 'tag', 'repo']) {
    if (!args[required]) throw new Error(`--${required} is required`);
  }
  const assets = new Map();
  for (const name of readdirSync(args['assets-dir'])) {
    if (/\.(exe|msi)$/i.test(name)) assets.set(name, readFileSync(join(args['assets-dir'], name)));
  }
  const manifest = JSON.parse(readFileSync(args.manifest, 'utf8'));
  const problems = verifyRelease({ manifest, assets, tag: args.tag, repo: args.repo });
  if (problems.length) {
    for (const problem of problems) console.error(`::error::${problem}`);
    process.exit(1);
  }
  console.log(`latest.json ok: ${manifest.version} -> ${manifest.platforms[TARGET].url}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    main();
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }
}
