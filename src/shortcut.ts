// Keyboard shortcut recorder used by the settings page.
// Kept framework-free and side-effect free: the caller owns the DOM element,
// the module only translates KeyboardEvents into the shortcut strings the
// Rust global-shortcut backend accepts (e.g. "Ctrl+Alt+Space").
export interface Accel {
  mods: Array<'Ctrl' | 'Alt' | 'Shift' | 'Win'>;
  /** Canonical key name, already spelled the way the backend parses it. */
  key: string;
}

const MODIFIER_CODES = new Set(['ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'ShiftLeft', 'ShiftRight', 'MetaLeft', 'MetaRight', 'OSLeft', 'OSRight', 'CapsLock']);

/** Function keys and a few named keys work without modifiers; everything else needs at least one, otherwise the OS would swallow normal typing. */
const BARE_KEYS = new Set(['F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8', 'F9', 'F10', 'F11', 'F12', 'Pause', 'PrintScreen', 'ScrollLock']);

const NAMED: Record<string, string> = {
  Space: 'Space', Escape: 'Escape', Enter: 'Enter', NumpadEnter: 'Enter', Tab: 'Tab', Backspace: 'Backspace', Delete: 'Delete',
  Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', Insert: 'Insert', ArrowUp: 'Up', ArrowDown: 'Down',
  ArrowLeft: 'Left', ArrowRight: 'Right', PrintScreen: 'PrintScreen', ScrollLock: 'ScrollLock', Pause: 'Pause',
  Minus: '-', Equal: '=', Comma: ',', Period: '.', Slash: '/', Semicolon: ';', Quote: "'", Backquote: '`', BracketLeft: '[', BracketRight: ']', Backslash: '\\',
};

/** Maps a KeyboardEvent to an accelerator, or returns a reason it cannot be used. */
export function eventToAccel(event: KeyboardEvent): { accel: Accel } | { reason: string } {
  const code = event.code;
  if (!code) return { reason: '无法识别按键，请重试' };
  if (code === 'Escape') return { reason: '已取消录制' };
  if (MODIFIER_CODES.has(code)) return { reason: '请再按一个主键，例如字母、数字或 F 键' };

  const mods: Accel['mods'] = [];
  if (event.ctrlKey) mods.push('Ctrl');
  if (event.altKey) mods.push('Alt');
  if (event.shiftKey) mods.push('Shift');
  if (event.metaKey) mods.push('Win');

  let key: string | undefined;
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
  else if (/^Digit[0-9]$/.test(code)) key = code.slice(5);
  else if (/^Numpad[0-9]$/.test(code)) key = `Numpad${code.slice(6)}`;
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) key = code;
  else key = NAMED[code];
  if (!key) return { reason: `暂不支持 ${event.key || code}` };

  if (!mods.length && !BARE_KEYS.has(key)) return { reason: '请至少按下一个修饰键：Ctrl / Alt / Shift / Win' };
  return { accel: { mods, key } };
}

/** Backend spelling: modifiers in a fixed order, joined by '+'. */
export function formatAccel(accel: Accel): string {
  return [...accel.mods, accel.key].join('+');
}

/** Short, localized rendering for UI labels. */
export function displayAccel(accel: Accel): string {
  return [...accel.mods, accel.key === 'Space' ? 'Space' : accel.key].join(' + ');
}

/** Parses a stored shortcut back into an Accel (used for the initial label). */
export function parseAccel(text: string): Accel | null {
  const parts = text.split('+').map((part) => part.trim()).filter(Boolean);
  if (!parts.length) return null;
  const mods: Accel['mods'] = [];
  let key = '';
  for (const part of parts) {
    const lower = part.toLowerCase();
    if (lower === 'ctrl' || lower === 'control') mods.push('Ctrl');
    else if (lower === 'alt' || lower === 'option') mods.push('Alt');
    else if (lower === 'shift') mods.push('Shift');
    else if (lower === 'win' || lower === 'super' || lower === 'meta' || lower === 'cmd') mods.push('Win');
    else key = part.length === 1 ? part.toUpperCase() : part.replace(/^esc$/i, 'Escape').replace(/^\w/, (c) => c.toUpperCase());
  }
  return key ? { mods, key } : null;
}
