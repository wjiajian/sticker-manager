import assert from "node:assert/strict";
import test from "node:test";
import { dragDropClientPoint } from "./dragDropPosition.ts";

for (const ratio of [2, 1, 0, NaN, Infinity]) {
  test(`macOS keeps logical coordinates at DPR ${ratio}`, () => {
    assert.deepEqual(dragDropClientPoint({ x: 300, y: 200 }, "macos", ratio), { x: 300, y: 200 });
  });
}

for (const [ratio, x, y] of [
  [1, 300, 150],
  [1.5, 200, 100],
  [2, 150, 75],
  [0, 300, 150],
  [-1, 300, 150],
  [NaN, 300, 150],
  [Infinity, 300, 150],
]) {
  test(`Windows converts physical pixels to client coordinates at DPR ${ratio}`, () => {
    assert.deepEqual(dragDropClientPoint({ x: 300, y: 150 }, "windows", ratio), { x, y });
  });
}

test("returns a new point on both platforms", () => {
  const position = { x: 300, y: 200 };
  assert.notStrictEqual(dragDropClientPoint(position, "macos", 1), position);
  assert.notStrictEqual(dragDropClientPoint(position, "windows", 1), position);
});
