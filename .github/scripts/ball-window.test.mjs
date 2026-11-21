// Drive the built Windows WebView2 app through its debug-only CDP endpoint.
// All settings and WebView data belong to a fresh temporary test directory.
// --autostart keeps the main window hidden; CDP input never moves the user's mouse.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";

assert.equal(process.platform, "win32", "This test requires Windows and WebView2.");
const root = await mkdtemp(join(tmpdir(), "sticker-ball-actions-"));
const library = join(root, "library");
const artifacts = resolve("target/ball-window-results");
await mkdir(artifacts, { recursive: true });
const delay = (ms) => new Promise((done) => setTimeout(done, ms));
async function until(read, description, timeout = 30_000) {
  const start = Date.now();
  let last;
  while (Date.now() - start < timeout) {
    try { const value = await read(); if (value) return value; } catch (error) { last = error; }
    await delay(100);
  }
  throw Error(`Timed out: ${description}`, { cause: last });
}
async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const port = server.address().port;
  await new Promise((done) => server.close(done));
  return port;
}
async function connect(url) {
  const socket = new WebSocket(url);
  await new Promise((done, reject) => {
    socket.addEventListener("open", done, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let sequence = 0;
  const requests = new Map();
  socket.addEventListener("message", ({ data }) => {
    const result = JSON.parse(data);
    const pending = requests.get(result.id);
    if (!pending) return;
    requests.delete(result.id);
    clearTimeout(pending.timer);
    if (result.error) pending.reject(Error(JSON.stringify(result.error)));
    else pending.resolve(result.result);
  });
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => { requests.delete(id); reject(Error(`CDP timeout: ${method}`)); }, 10_000);
    requests.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params }));
  });
  return {
    send,
    close: () => socket.close(),
    async evaluate(expression) {
      const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
      return result.result.value;
    },
  };
}

let app;
let main;
let ball;
let output = "";
async function start() {
  const port = await freePort();
  app = spawn(resolve("target/debug/sticker-manager.exe"), ["--autostart"], {
    windowsHide: true,
    env: {
      ...process.env,
      STICKER_LIBRARY_DIR: library,
      WEBVIEW2_USER_DATA_FOLDER: join(root, `webview-${port}`),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --disable-background-timer-throttling`,
    },
  });
  app.stdout.on("data", (data) => { output += data; });
  app.stderr.on("data", (data) => { output += data; });
  const pages = await until(async () => {
    if (app.exitCode !== null) throw Error(`App exited ${app.exitCode}: ${output}`);
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    return targets.some((page) => page.url.includes("ball.html"))
      && targets.some((page) => /\/(?:index\.html)?$/.test(page.url)) && targets.length >= 4 && targets;
  }, "WebView2 pages", 60_000);
  const mainPage = pages.find((page) => /\/(?:index\.html)?$/.test(page.url));
  assert.ok(mainPage, JSON.stringify(pages));
  main = await connect(mainPage.webSocketDebuggerUrl);
  ball = await connect(pages.find((page) => page.url.includes("ball.html")).webSocketDebuggerUrl);
  await until(() => main.evaluate("[...document.querySelectorAll('button')].some(b => b.textContent.trim() === '偏好设置') && !!window.__TAURI_INTERNALS__"), "main app ready");
}
async function stop() {
  main?.close();
  ball?.close();
  if (app && app.exitCode === null) {
    const exited = new Promise((done) => app.once("exit", done));
    app.kill();
    await exited;
  }
}
const invoke = (client, command, args = {}) => client.evaluate(
  `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`,
);
const saved = async () => JSON.parse(await readFile(join(library, "local/settings.json"), "utf8"));
const state = () => invoke(ball, "ball_state");
async function sizeIs(size) {
  await until(async () => (await state()).size === size, `native ball size ${size}`);
  await until(async () => (await saved()).ballSize === size, `saved size ${size}`);
  await until(() => ball.evaluate(`document.querySelector('button')?.style.width === '${size}px'`), `rendered size ${size}`);
  await until(() => main.evaluate(`!document.querySelector('dialog[open]') || document.querySelector('dialog[open] [role=tabpanel]:not([hidden]) input[type=range]')?.value === '${size}'`), "size readout updated");
}
async function preferences() {
  await main.evaluate("[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '偏好设置').click()");
  await until(() => main.evaluate("!!document.querySelector('dialog[open]')"), "preferences open");
  await main.evaluate("[...document.querySelectorAll('[role=tab]')].find(b => b.textContent.trim() === '悬浮球与面板').click()");
  await until(() => main.evaluate("!!document.querySelector('dialog[open] [role=tabpanel]:not([hidden]) input[type=range]:not(:disabled)')"), "size slider enabled");
}
async function dragToMaximum() {
  const rect = await main.evaluate("(() => { const r = document.querySelector('dialog[open] [role=tabpanel]:not([hidden]) input[type=range]').getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; })()");
  const x = rect.x + rect.width / 2;
  const y = rect.y + rect.height / 2;
  await main.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 });
  await main.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: rect.x + rect.width - 1, y, button: "left", buttons: 1 });
  await until(async () => (await state()).size === 80, "live size preview");
  return { x: rect.x + rect.width + 50, y };
}
const release = ({ x, y }) => main.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1 });
const key = (type, key, code) => main.send("Input.dispatchKeyEvent", { type, key, code: key, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code });

async function rapidAdjustments(cancelSecond) {
  await main.evaluate("[...document.querySelectorAll('dialog[open] button')].find(b => b.textContent.trim() === '完成')?.click()");
  await until(() => main.evaluate("!document.querySelector('dialog[open]')"), "previous preferences closed");
  await invoke(main, "set_ball_size", { size: 56 });
  await sizeIs(56);
  await preferences();
  // Hold the first preview IPC request while real pointer/key events queue the
  // next gestures. The app's React handlers and native persistence remain intact.
  await main.evaluate(`(() => {
    const original = window.fetch;
    window.ballQueueTest = { calls: [], held: false, release: null, restore: () => { window.fetch = original; } };
    window.fetch = async function(input, options) {
      const command = new URL(String(input)).pathname.slice(1);
      if (command === 'preview_ball_size' || command === 'set_ball_size') {
        const args = JSON.parse(options.body);
        window.ballQueueTest.calls.push({ command, size: args.size });
        if (command === 'preview_ball_size' && args.size !== null && !window.ballQueueTest.held) {
          window.ballQueueTest.held = true;
          await new Promise(resolve => { window.ballQueueTest.release = resolve; });
        }
      }
      return original.call(this, input, options);
    };
  })()`);
  try {
    const rect = await main.evaluate("(() => { const r = document.querySelector('dialog[open] [role=tabpanel]:not([hidden]) input[type=range]').getBoundingClientRect(); return { left: r.x + 2, right: r.right - 2, y: r.y + r.height / 2 }; })()");
    const mouse = (type, x, buttons) => main.send("Input.dispatchMouseEvent", {
      type, x, y: rect.y, button: "left", buttons, clickCount: 1,
    });
    await mouse("mousePressed", rect.right, 1);
    await until(() => main.evaluate("!!window.ballQueueTest.release"), "first preview held");
    assert.equal((await saved()).ballSize, 56);
    await mouse("mouseReleased", rect.right, 0); // First gesture commits 80.
    await mouse("mousePressed", rect.left, 1); // Second gesture previews 40.
    if (cancelSecond) {
      await key("keyDown", "Escape", 27);
      await key("keyUp", "Escape", 27);
      await until(() => main.evaluate("!document.querySelector('dialog[open]')"), "second gesture cancelled");
    }
    await mouse("mouseReleased", rect.left, 0);
    await main.evaluate("window.ballQueueTest.release()");
    await sizeIs(cancelSecond ? 80 : 40);
    const calls = await main.evaluate("window.ballQueueTest.calls");
    assert.deepEqual(
      calls.filter(({ command }) => command === "set_ball_size").map(({ size }) => size),
      cancelSecond ? [80] : [80, 40],
      "released commits must survive later previews and cancellation, in order",
    );
    await writeFile(join(artifacts, `rapid-${cancelSecond ? "cancel" : "commit"}.json`), JSON.stringify({ calls, saved: (await saved()).ballSize }, null, 2));
  } finally {
    await main.evaluate("window.ballQueueTest.release?.(); window.ballQueueTest.restore()");
  }
}

try {
  await start();
  await invoke(main, "set_ball_size", { size: 56 });
  await preferences();
  let position = await dragToMaximum();
  assert.equal((await saved()).ballSize, 56, "drag must not persist");
  await delay(3500);
  assert.equal((await state()).collapsed, false, "preview prevents auto-hide");
  assert.equal((await saved()).ballSize, 56);
  await release(position); // outside the slider
  await sizeIs(80);

  await main.evaluate("document.querySelector('dialog[open] [role=tabpanel]:not([hidden]) input[type=range]').focus()");
  await key("keyDown", "Home", 36);
  await until(async () => (await state()).size === 40, "keyboard preview");
  assert.equal((await saved()).ballSize, 80, "held key must not persist");
  await key("keyUp", "Home", 36);
  await sizeIs(40);
  await main.evaluate("[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '恢复默认').click()");
  await sizeIs(56);

  position = await dragToMaximum();
  await key("keyDown", "Escape", 27);
  await key("keyUp", "Escape", 27);
  await until(() => main.evaluate("!document.querySelector('dialog[open]')"), "Escape closes preferences");
  await release(position);
  await sizeIs(56);
  assert.equal((await saved()).ballSize, 56, "Escape cancels held pointer preview");
  await preferences();
  position = await dragToMaximum();
  await release(position);
  await sizeIs(80);
  const screenshot = await main.send("Page.captureScreenshot", { format: "png" });
  await writeFile(join(artifacts, "preferences-80.png"), Buffer.from(screenshot.data, "base64"));
  await stop();
  await start();
  await sizeIs(80);
  await rapidAdjustments(false);
  await rapidAdjustments(true);
  await stop();
  await start();
  await sizeIs(80);
  await writeFile(join(artifacts, "result.json"), JSON.stringify({ passed: true, savedSize: 80, rapidCommit: true, rapidCancel: true, library }, null, 2));
  console.log("Native WebView2 preview, save, keyboard, reset, Escape, rapid gestures, and restart checks passed.");
} catch (error) {
  await writeFile(join(artifacts, "failure.txt"), String(error.stack ?? error));
  try {
    const screenshot = await main.send("Page.captureScreenshot", { format: "png" });
    await writeFile(join(artifacts, "failure.png"), Buffer.from(screenshot.data, "base64"));
  } catch { /* Startup failures may not expose a CDP target. */ }
  throw error;
} finally {
  await writeFile(join(artifacts, "app.log"), output);
  await stop();
}
