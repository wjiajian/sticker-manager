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
const root = await mkdtemp(join(tmpdir(), "sticker-shortcut-actions-"));
const library = join(root, "library");
const artifacts = resolve("target/panel-shortcut-results");
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
  await until(() => main.evaluate("[...document.querySelectorAll('button')].some(b => b.textContent.trim() === '偏好设置') && !!window.__TAURI_INTERNALS__"), "main app ready");
}
async function stop() {
  main?.close();
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

const state = () => invoke(main, "panel_shortcut_state");
const recorder = "[...document.querySelectorAll('dialog[open] [data-section=panel] button')].find(b => b.querySelector('kbd'))";
const clickText = (text) => main.evaluate(`(() => {
  const button = [...document.querySelectorAll('dialog[open] [data-section=panel] button')].find(b => b.textContent.trim() === ${JSON.stringify(text)});
  if (!button) throw new Error('Button not found: ' + ${JSON.stringify(text)});
  button.click();
})()`);
async function preferences() {
  await main.evaluate("[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '偏好设置').click()");
  await until(() => main.evaluate("!!document.querySelector('dialog[open]')"), "preferences open");
  await until(() => main.evaluate(`!!(${recorder}) && !(${recorder}).disabled`), "recorder enabled");
}
async function begin() {
  // Suppress the intentional 400 ms post-save click guard between independent gestures.
  await delay(450);
  await main.evaluate(`(${recorder}).click()`);
  await until(async () => (await state()).recordingId !== null, "recording started");
  await until(() => main.evaluate(`(${recorder}).textContent.includes('按下组合键')`), "recording UI");
}
const key = (type, key, code, vk, modifiers = 0) => main.send("Input.dispatchKeyEvent", {
  type, key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers,
});
async function chord(code, vk) {
  await key("keyDown", "Control", "ControlLeft", 17, 2);
  await key("keyDown", "Alt", "AltLeft", 18, 3);
  await key("keyDown", code.replace("Key", ""), code, vk, 3);
  await key("keyUp", code.replace("Key", ""), code, vk, 3);
  await key("keyUp", "Alt", "AltLeft", 18, 2);
  await key("keyUp", "Control", "ControlLeft", 17, 0);
}
async function shortcutIs(shortcut) {
  await until(async () => { const value = await state(); return value.shortcut === shortcut && value.recordingId === null && value.status === (shortcut ? "active" : "disabled"); }, `native shortcut ${shortcut}`);
  await until(async () => (await saved()).panelShortcut === shortcut, `saved shortcut ${shortcut}`);
  await until(() => main.evaluate(`(${recorder})?.getAttribute('aria-busy') === 'false'`), "recorder idle");
}
try {
  await start();
  await preferences();
  assert.equal((await state()).shortcut, "Ctrl+Alt+KeyE");
  await begin();
  await key("keyDown", "a", "KeyA", 65);
  await key("keyUp", "a", "KeyA", 65);
  await until(() => main.evaluate("[...document.querySelectorAll('dialog[open] [role=alert]')].some(el => el.textContent.includes('缺少修饰键'))"), "invalid bare letter rejected");
  await key("keyDown", "Escape", "Escape", 27);
  await key("keyUp", "Escape", "Escape", 27);
  await until(async () => (await state()).recordingId === null, "Escape cancels");
  assert.equal(await main.evaluate("!!document.querySelector('dialog[open]')"), true, "Escape must not close preferences");

  await begin();
  await chord("KeyK", 75);
  await shortcutIs("Ctrl+Alt+KeyK");
  await begin();
  await chord("KeyK", 75); // The current shortcut must be recordable again.
  await shortcutIs("Ctrl+Alt+KeyK");
  await begin();
  await main.evaluate(`(${recorder}).blur()`);
  await shortcutIs("Ctrl+Alt+KeyK");

  // Native success emits before the IPC Promise settles. UI must stay saving.
  await begin();
  await main.evaluate(`(() => {
    const original = window.fetch;
    window.shortcutTest = { release: null, restore: () => { window.fetch = original; } };
    window.fetch = async function(input, options) {
      const response = await original.call(this, input, options);
      if (new URL(String(input)).pathname === '/set_panel_shortcut') {
        await new Promise(resolve => { window.shortcutTest.release = resolve; });
      }
      return response;
    };
  })()`);
  await chord("KeyJ", 74);
  await until(() => main.evaluate("!!window.shortcutTest.release"), "save response held");
  assert.equal((await state()).recordingId, null);
  assert.equal(await main.evaluate(`(${recorder}).getAttribute('aria-busy')`), "true", "success event must not cancel saving");
  await key("keyDown", "Escape", "Escape", 27);
  await key("keyUp", "Escape", "Escape", 27);
  await main.evaluate("window.shortcutTest.release(); window.shortcutTest.restore()");
  await shortcutIs("Ctrl+Alt+KeyJ");
  assert.equal(await main.evaluate("!!document.querySelector('dialog[open]')"), true);

  await clickText("停用");
  await shortcutIs(null);
  await stop(); await start(); await preferences();
  await shortcutIs(null);
  await clickText("恢复默认");
  await shortcutIs("Ctrl+Alt+KeyE");
  await begin(); await chord("KeyK", 75); await shortcutIs("Ctrl+Alt+KeyK");
  await stop(); await start(); await preferences();
  await shortcutIs("Ctrl+Alt+KeyK");
  const screenshot = await main.send("Page.captureScreenshot", { format: "png" });
  await writeFile(join(artifacts, "preferences.png"), Buffer.from(screenshot.data, "base64"));
  await writeFile(join(artifacts, "result.json"), JSON.stringify({ passed: true, state: await state(), library }, null, 2));
  console.log("Shortcut recording, validation, cancellation, save race, disable, reset, and restart checks passed.");
} catch (error) {
  await writeFile(join(artifacts, "failure.txt"), String(error.stack ?? error));
  throw error;
} finally {
  await writeFile(join(artifacts, "app.log"), output);
  await stop();
}
