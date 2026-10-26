import assert from 'node:assert/strict';
import test from 'node:test';
import { eventToAccel, formatAccel, displayAccel, parseAccel } from '../src/shortcut.ts';

// Exercises the real shortcut recorder from the settings page, imported straight
// from source: Node strips the types, so the test needs no TypeScript compiler
// API (7.x no longer exposes `transpileModule` on the `typescript` root export).

const key = (code, extra = {}) => ({ code, key: extra.key ?? code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...extra });

test('records modifier combinations in a stable spelling', () => {
  const { accel } = eventToAccel(key('Space', { ctrlKey: true, altKey: true }));
  assert.equal(formatAccel(accel), 'Ctrl+Alt+Space');
  assert.equal(displayAccel(accel), 'Ctrl + Alt + Space');
});

test('accepts function keys without modifiers but not bare letters', () => {
  assert.equal(formatAccel(eventToAccel(key('F8')).accel), 'F8');
  assert.match(eventToAccel(key('KeyQ')).reason, /修饰键/);
  assert.match(eventToAccel(key('Digit1')).reason, /修饰键/);
});

test('maps digits, punctuation, arrows and numpad to backend names', () => {
  assert.equal(formatAccel(eventToAccel(key('Digit5', { ctrlKey: true, shiftKey: true })).accel), 'Ctrl+Shift+5');
  assert.equal(formatAccel(eventToAccel(key('Slash', { altKey: true })).accel), 'Alt+/');
  assert.equal(formatAccel(eventToAccel(key('ArrowLeft', { ctrlKey: true, altKey: true })).accel), 'Ctrl+Alt+Left');
  assert.equal(formatAccel(eventToAccel(key('Numpad3', { ctrlKey: true })).accel), 'Ctrl+Numpad3');
  assert.equal(formatAccel(eventToAccel(key('KeyA', { metaKey: true, key: 'a' })).accel), 'Win+A');
});

test('modifier-only presses ask for a main key, Escape cancels', () => {
  assert.match(eventToAccel(key('ControlLeft', { ctrlKey: true })).reason, /主键/);
  assert.match(eventToAccel(key('Escape')).reason, /已取消/);
  assert.match(eventToAccel(key('MediaPlayPause', { ctrlKey: true })).reason, /暂不支持/);
});

test('stored shortcuts parse back to a readable label', () => {
  const accel = parseAccel('Ctrl + Shift + Space');
  assert.deepEqual(accel, { mods: ['Ctrl', 'Shift'], key: 'Space' });
  assert.equal(displayAccel(accel), 'Ctrl + Shift + Space');
  assert.deepEqual(parseAccel('Control+Escape'), { mods: ['Ctrl'], key: 'Escape' });
  assert.equal(parseAccel(''), null);
});
