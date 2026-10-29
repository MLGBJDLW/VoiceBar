import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { normalizeVersion, installerName, buildManifest, parseArgs, isSignature } from '../scripts/make-latest-json.mjs';
import { verifyRelease } from '../scripts/verify-release.mjs';

test('versions normalize from tags and reject non-semver', () => {
  assert.equal(normalizeVersion('v0.2.0'), '0.2.0');
  assert.equal(normalizeVersion(' 1.0.0 '), '1.0.0');
  assert.equal(normalizeVersion('v1.2.3-beta.1'), '1.2.3-beta.1');
  assert.throws(() => normalizeVersion('release-1'), /semver/);
  assert.throws(() => normalizeVersion('v1.2'), /semver/);
});

test('installer name matches the native NSIS bundle naming', () => {
  assert.equal(installerName('0.2.0'), 'VoiceBar_0.2.0_x64-setup.exe');
});

test('manifest has the keys the Rust updater reads', () => {
  const m = buildManifest({
    version: '0.2.0',
    repo: 'MLGBJDLW/VoiceBar',
    installerName: 'VoiceBar_0.2.0_x64-setup.exe',
    signature: 'SIG',
    sha256: 'abc',
    notes: '- fixed',
    pubDate: '2026-10-26T00:00:00Z',
  });
  assert.equal(m.version, '0.2.0');
  assert.equal(m.pub_date, '2026-10-26T00:00:00Z');
  const platform = m.platforms['windows-x86_64'];
  assert.equal(
    platform.url,
    'https://github.com/MLGBJDLW/VoiceBar/releases/download/v0.2.0/VoiceBar_0.2.0_x64-setup.exe',
  );
  assert.equal(platform.signature, 'SIG');
  assert.equal(platform.sha256, 'abc');
  // Keep the signed update manifest limited to the fields the native updater reads.
  assert.deepEqual(Object.keys(platform).sort(), ['sha256', 'signature', 'url']);
});

test('args parse as --key value pairs', () => {
  assert.deepEqual(parseArgs(['--tag', 'v1.0.0', '--repo', 'a/b']), { tag: 'v1.0.0', repo: 'a/b' });
  assert.throws(() => parseArgs(['tag', 'v1.0.0']), /unexpected/);
  assert.throws(() => parseArgs(['--tag']), /missing value/);
});

test('signature sanity check accepts base64 only', () => {
  assert.equal(isSignature('A'.repeat(88)), true);
  assert.equal(isSignature('-----BEGIN SIGNATURE-----'), false);
  assert.equal(isSignature('short'), false);
});

// --- release verification ---------------------------------------------------

function fixture({ version = '0.2.0', tamperHash = false, dropPlatform = false, urlRepo = 'MLGBJDLW/VoiceBar' } = {}) {
  const bundle = Buffer.from('fake nsis installer bytes '.repeat(10));
  const name = installerName(version);
  const manifest = buildManifest({
    version,
    repo: urlRepo,
    installerName: name,
    signature: 'A'.repeat(88),
    sha256: createHash('sha256').update(tamperHash ? Buffer.from('other') : bundle).digest('hex'),
    pubDate: '2026-10-26T00:00:00Z',
  });
  if (dropPlatform) delete manifest.platforms['windows-x86_64'];
  const assets = new Map([
    [name, bundle],
    [`VoiceBar_${version}_x64_en-US.msi`, Buffer.from('msi')],
  ]);
  return { manifest, assets, tag: `v${version}`, repo: 'MLGBJDLW/VoiceBar' };
}

test('a consistent release passes verification', () => {
  const f = fixture();
  assert.deepEqual(verifyRelease(f), []);
});

test('verification catches a hash that does not match the uploaded installer', () => {
  const problems = verifyRelease(fixture({ tamperHash: true }));
  assert.equal(problems.length, 1);
  assert.match(problems[0], /sha256 不匹配/);
});

test('verification refuses a manifest without the windows-x86_64 entry', () => {
  const problems = verifyRelease(fixture({ dropPlatform: true }));
  assert.equal(problems.length, 1);
  assert.match(problems[0], /windows-x86_64/);
});

test('verification refuses a URL pointing at another repo', () => {
  const problems = verifyRelease(fixture({ urlRepo: 'someone/fork' }));
  assert.equal(problems.length, 1);
  assert.match(problems[0], /下载地址应指向/);
});

test('verification refuses a version that differs from the tag', () => {
  const f = fixture();
  f.tag = 'v0.3.0';
  assert.match(verifyRelease(f)[0], /version 0\.2\.0 != 标签 v0\.3\.0/);
});

test('native release requires the NSIS installer and does not require MSI', () => {
  const f = fixture();
  for (const name of [...f.assets.keys()]) if (name.endsWith('.msi')) f.assets.delete(name);
  assert.deepEqual(verifyRelease(f), []);
  const name = installerName(f.manifest.version);
  f.assets.delete(name);
  assert.ok(verifyRelease(f).some(problem => problem.includes('缺少资产')));
});
