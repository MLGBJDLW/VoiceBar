// CDP-driven browser automation for the local Vite preview (no dependencies).
// Usage: node scripts/cdp-preview.mjs <ws-endpoint> <output.png>
// Connects to an existing headless Chromium started by the caller, clicks the
// "输入与启动" tab, starts the shortcut recorder, and captures the viewport.
const [, , wsUrl, output] = process.argv;
if (!wsUrl || !output) {
  console.error('usage: node scripts/cdp-preview.mjs <ws-endpoint> <output.png>');
  process.exit(2);
}
const { writeFileSync } = await import('node:fs');

const socket = new WebSocket(wsUrl);
let nextId = 1;
const pending = new Map();
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
socket.addEventListener('message', (event) => {
  const message = JSON.parse(event.data);
  if (!message.id || !pending.has(message.id)) return;
  const { resolve, reject } = pending.get(message.id);
  pending.delete(message.id);
  if (message.error) reject(new Error(`${message.error.message}`));
  else resolve(message.result);
});
await new Promise((resolve, reject) => {
  socket.addEventListener('open', resolve, { once: true });
  socket.addEventListener('error', reject, { once: true });
});

await send('Page.enable');
await send('Runtime.enable');
await send('Page.navigate', { url: process.env.PAGE_URL });
const evaluate = async (expression) => {
  const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(`${result.exceptionDetails.text} ${result.exceptionDetails.exception?.description ?? ''}`);
  return result.result.value;
};

// Wait for the settings UI, switch to the shortcut tab, start recording.
const log = [];
for (let attempt = 0; attempt < 60; attempt++) {
  const probe = await evaluate(`({ ready: Boolean(document.querySelector('[data-tab="input"]')), url: location.href, body: (document.body && document.body.innerText || '').slice(0, 120), scripts: [...document.scripts].map(s => s.src).filter(Boolean).length })`);
  log.push(`${attempt}:${probe.ready}:${probe.url}`);
  if (probe.ready) break;
  await new Promise((r) => setTimeout(r, 250));
  if (attempt === 59) throw new Error(`settings UI never rendered: ${JSON.stringify(probe)} log=${log.slice(-5).join(' ')}`);
}
await evaluate(`document.querySelector('[data-tab="input"]').click(); true`);
await new Promise((r) => setTimeout(r, 300));
await evaluate(`document.querySelector('[data-shortcut="hotkey"]').click(); true`);
await new Promise((r) => setTimeout(r, 400));
const state = await evaluate(`({
  title: document.querySelector('.shortcut-input kbd')?.textContent,
  width: innerWidth, height: innerHeight,
  overflow: document.documentElement.scrollWidth > innerWidth,
  version: document.querySelector('.version')?.textContent
})`);
await evaluate(`window.scrollTo(0, 0); true`);
const shot = await send('Page.captureScreenshot', { format: 'png' });
writeFileSync(output, Buffer.from(shot.data, 'base64'));
console.log(JSON.stringify({ ...state, output }));
socket.close();
process.exit(0);
