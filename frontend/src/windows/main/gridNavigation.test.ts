import assert from "node:assert/strict";
import test from "node:test";
import { gridNavigationTarget, isGridArrowKey, revealScrollTop, type GridArrowKey } from "./gridNavigation.ts";

function checkMoves(count: number, columns: number, key: GridArrowKey, moves: [number, number][]) {
  for (const [index, expected] of moves) {
    assert.equal(gridNavigationTarget(key, index, count, columns), expected,
      `${key} from index ${index} with ${count} stickers and ${columns} columns`);
  }
}

test("horizontal arrows cross rows and vertical arrows handle a partial final row", () => {
  checkMoves(10, 4, "ArrowRight", [[0, 1], [3, 4], [8, 9], [9, 9]]);
  checkMoves(10, 4, "ArrowLeft", [[1, 0], [4, 3], [8, 7], [0, 0]]);
  checkMoves(10, 4, "ArrowDown", [[0, 4], [4, 8], [5, 9], [6, 9], [7, 9], [8, 8], [9, 9]]);
  checkMoves(10, 4, "ArrowUp", [[0, 0], [3, 3], [4, 0], [8, 4], [9, 5]]);
});

test("navigation stops at a full final row", () => {
  checkMoves(8, 4, "ArrowDown", [[3, 7], [4, 4], [7, 7]]);
  checkMoves(8, 4, "ArrowRight", [[7, 7]]);
  checkMoves(8, 4, "ArrowUp", [[7, 3]]);
});

test("all four arrows work in a single column", () => {
  checkMoves(3, 1, "ArrowDown", [[0, 1], [1, 2], [2, 2]]);
  checkMoves(3, 1, "ArrowUp", [[1, 0], [0, 0]]);
  checkMoves(3, 1, "ArrowRight", [[0, 1]]);
  checkMoves(3, 1, "ArrowLeft", [[2, 1]]);
});

test("vertical arrows stay in place when all stickers share one row", () => {
  checkMoves(3, 6, "ArrowDown", [[0, 0], [2, 2]]);
  checkMoves(3, 6, "ArrowUp", [[1, 1]]);
  checkMoves(3, 6, "ArrowRight", [[2, 2]]);
  checkMoves(3, 6, "ArrowLeft", [[0, 0]]);
});

const arrows: GridArrowKey[] = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"];

test("every arrow keeps a single sticker selected", () => {
  for (const key of arrows) checkMoves(1, 4, key, [[0, 0]]);
});

test("column counts fall back to one or round down", () => {
  for (const columns of [0, -3, NaN, Infinity]) {
    checkMoves(10, columns, "ArrowDown", [[5, 6]]);
    checkMoves(10, columns, "ArrowUp", [[5, 4]]);
  }
  checkMoves(10, 2.7, "ArrowDown", [[5, 7]]);
  checkMoves(10, 2.7, "ArrowUp", [[5, 3]]);
});

test("invalid indexes and counts preserve the starting index", () => {
  checkMoves(10, 4, "ArrowRight", [[-1, -1], [10, 10]]);
  checkMoves(10, 4, "ArrowDown", [[1.5, 1.5]]);
  checkMoves(0, 4, "ArrowLeft", [[0, 0]]);
  checkMoves(NaN, 4, "ArrowDown", [[0, 0]]);
});

test("only the four arrow key names trigger navigation", () => {
  for (const key of arrows) assert.equal(isGridArrowKey(key), true);
  for (const key of ["Up", "Left", "arrowup", "Enter", "a", ""]) assert.equal(isGridArrowKey(key), false);
});

test("revealing a tile scrolls only as far as needed for its margins", () => {
  assert.equal(revealScrollTop(100, 500, 200, 300, 24), 100);
  assert.equal(revealScrollTop(400, 500, 200, 300, 24), 176);
  assert.equal(revealScrollTop(0, 500, 600, 700, 24), 224);
  assert.equal(revealScrollTop(0, 500, 376, 476, 24), 0);
  assert.equal(revealScrollTop(0, 100, 300, 600, 24), 276);
  assert.equal(revealScrollTop(50, 500, 10, 110, 24), 0);
});
