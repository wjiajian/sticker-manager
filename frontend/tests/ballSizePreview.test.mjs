import assert from "node:assert/strict";
import { test } from "node:test";
import { ballSizePreview } from "../src/lib/ballSizePreview.ts";

function fixture() {
  const calls = [];
  const errors = [];
  const waiting = [];
  const queue = ballSizePreview((size) => {
    calls.push(size);
    return new Promise((resolve, reject) => waiting.push({ resolve, reject }));
  });
  const dispatcher = { ...queue, preview: (size) => queue.preview(size, (error) => errors.push(error)) };
  return { dispatcher, calls, errors, waiting };
}
const tick = () => new Promise((resolve) => setImmediate(resolve));

test("rapid changes coalesce while one preview is in flight", async () => {
  const f = fixture();
  void f.dispatcher.preview(40);
  await tick();
  void f.dispatcher.preview(44);
  const done = f.dispatcher.preview(80);
  assert.deepEqual(f.calls, [40]);
  f.waiting.shift().resolve();
  await tick();
  assert.deepEqual(f.calls, [40, 80]);
  f.waiting.shift().resolve();
  await done;
});

test("commit waits for in-flight preview and drops intermediate sizes", async () => {
  const f = fixture();
  void f.dispatcher.preview(40);
  await tick();
  void f.dispatcher.preview(64);
  const committed = f.dispatcher.commit(async () => { f.calls.push("save 80"); return 80; });
  assert.deepEqual(f.calls, [40]);
  f.waiting.shift().resolve();
  await tick();
  assert.deepEqual(f.calls, [40, "save 80", null]);
  f.waiting.shift().resolve();
  assert.equal(await committed, 80);
});

test("closing during a slow preview clears it without saving", async () => {
  const f = fixture();
  void f.dispatcher.preview(40);
  await tick();
  void f.dispatcher.preview(80);
  const closed = f.dispatcher.preview(null);
  f.waiting.shift().resolve();
  await tick();
  assert.deepEqual(f.calls, [40, null]);
  f.waiting.shift().resolve();
  await closed;
});

test("failed preview does not prevent clearing or later previews", async () => {
  const f = fixture();
  void f.dispatcher.preview(40);
  await tick();
  const cleared = f.dispatcher.preview(null);
  f.waiting.shift().reject("preview failed");
  await tick();
  f.waiting.shift().resolve();
  await cleared;
  assert.deepEqual(f.errors, ["preview failed"]);
  const next = f.dispatcher.preview(56);
  await tick();
  f.waiting.shift().resolve();
  await next;
  assert.deepEqual(f.calls, [40, null, 56]);
});

test("failed save still clears preview and subsequent saves can succeed", async () => {
  const calls = [];
  const dispatcher = ballSizePreview(async (size) => { calls.push(size); });
  await assert.rejects(dispatcher.commit(async () => { throw Error("disk full"); }), /disk full/);
  assert.deepEqual(calls, [null]);
  assert.equal(await dispatcher.commit(async () => 56), 56);
  assert.deepEqual(calls, [null, null]);
});

test("reopened dialog previews wait for the previous dialog's pending save", async () => {
  const f = fixture();
  void f.dispatcher.preview(40);
  await tick();
  const oldSave = f.dispatcher.commit(async () => { f.calls.push("old save"); });
  void f.dispatcher.preview(null); // old dialog unmounts
  const newPreview = f.dispatcher.preview(80); // new dialog changes slider
  f.waiting.shift().resolve();
  await tick();
  assert.deepEqual(f.calls, [40, "old save", null]);
  f.waiting.shift().resolve();
  await oldSave;
  await tick();
  assert.deepEqual(f.calls, [40, "old save", null, 80]);
  f.waiting.shift().resolve();
  await newPreview;
});

test("closing and reopening before preview settles keeps only the new preview", async () => {
  const f = fixture();
  void f.dispatcher.preview(40);
  await tick();
  void f.dispatcher.preview(null);
  const newPreview = f.dispatcher.preview(80);
  f.waiting.shift().resolve();
  await tick();
  assert.deepEqual(f.calls, [40, 80]);
  f.waiting.shift().resolve();
  await newPreview;
});
