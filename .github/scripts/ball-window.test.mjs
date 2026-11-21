// Windows Actions: drive the built WebView2 app through its debug-only CDP endpoint.
// All settings and WebView data belong to a fresh temporary test directory.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";

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
  app = spawn(resolve("target/debug/sticker-manager.exe"), [], {
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
  await until(() => main.evaluate("!document.querySelector('dialog[open]') || !document.querySelector('input[aria-label=悬浮球大小]').disabled"), "size commit complete");
}
async function preferences() {
  await main.evaluate("[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '偏好设置').click()");
  await until(() => main.evaluate("!!document.querySelector('dialog[open]')"), "preferences open");
  await main.evaluate("[...document.querySelectorAll('[role=tab]')].find(b => b.textContent.trim() === '悬浮球与面板').click()");
  await until(() => main.evaluate("!!document.querySelector('input[aria-label=悬浮球大小]:not(:disabled)')"), "size slider enabled");
}
async function dragToMaximum() {
  const rect = await main.evaluate("(() => { const r = document.querySelector('input[aria-label=悬浮球大小]').getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; })()");
  const x = rect.x + rect.width / 2;
  const y = rect.y + rect.height / 2;
  await main.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 });
  await main.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: rect.x + rect.width - 1, y, button: "left", buttons: 1 });
  await until(async () => (await state()).size === 80, "live size preview");
  return { x: rect.x + rect.width + 50, y };
}
const release = ({ x, y }) => main.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1 });
const key = (type, key, code) => main.send("Input.dispatchKeyEvent", { type, key, code: key, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code });

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

  await main.evaluate("document.querySelector('input[aria-label=悬浮球大小]').focus()");
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
  await writeFile(join(artifacts, "result.json"), JSON.stringify({ passed: true, savedSize: 80, library }, null, 2));
  console.log("Native WebView2 preview, save, keyboard, reset, Escape, and restart checks passed.");
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
