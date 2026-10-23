// The fullscreen mode's focus moves (node --test app/test).
import { test } from "node:test";
import assert from "node:assert/strict";
import { carouselMove, gridMove, homeAction, keyAction, listMove, shelfMove } from "../src/fullscreen/fsnav.js";

test("the carousel wraps round", () => {
  assert.equal(carouselMove(0, 4, "left"), 3);
  assert.equal(carouselMove(3, 4, "right"), 0);
  assert.equal(carouselMove(2, 4, "up"), 2);
  assert.equal(carouselMove(2, 4, "last"), 3);
  assert.equal(carouselMove(0, 0, "right"), 0);
});

test("lists wrap round both ways", () => {
  assert.equal(listMove(0, 3, "up"), 2);
  assert.equal(listMove(2, 3, "down"), 0);
  assert.equal(listMove(1, 3, "right"), 2);
});

test("a grid moves by card, row, page and end", () => {
  // 10 cards in rows of 4: 0-3, 4-7, 8-9.
  const g = (index, action, rows = 2) => gridMove({ index, count: 10, cols: 4, rows }, action);
  assert.equal(g(0, "left"), 0);
  assert.equal(g(3, "right"), 4, "on to the next row");
  assert.equal(g(4, "left"), 3, "back to the row before");
  assert.equal(g(9, "right"), 9);
  assert.equal(g(1, "up"), "out-up", "the top row goes up to the carousel");
  assert.equal(g(5, "up"), 1);
  assert.equal(g(1, "down"), 5);
  assert.equal(g(6, "down"), 9, "onto the short last row");
  assert.equal(g(9, "down"), 9);
  assert.equal(g(9, "page_up"), 1);
  assert.equal(g(1, "page_down"), 9);
  assert.equal(g(5, "first"), 0);
  assert.equal(g(5, "last"), 9);
  assert.equal(gridMove({ index: 0, count: 0, cols: 4 }, "up"), "out-up");
});

test("the home screen: carousel, grid and the buttons", () => {
  const base = { platform: 1, platforms: 3, game: 5, count: 10, cols: 4, rows: 2 };
  const carousel = { ...base, focus: "carousel" };
  assert.deepEqual(homeAction(carousel, "right"), { platform: 2, game: 0 });
  assert.deepEqual(homeAction(carousel, "down"), { focus: "grid" });
  assert.deepEqual(homeAction(carousel, "back"), { intent: "leave" });
  assert.deepEqual(homeAction({ ...carousel, count: 0 }, "accept"), {}, "nothing to go down to");
  const grid = { ...base, focus: "grid" };
  assert.deepEqual(homeAction(grid, "accept"), { intent: "open" });
  assert.deepEqual(homeAction(grid, "north"), { intent: "options" });
  assert.deepEqual(homeAction(grid, "back"), { focus: "carousel" });
  assert.deepEqual(homeAction(grid, "right"), { game: 6 });
  assert.deepEqual(homeAction({ ...grid, game: 2 }, "up"), { focus: "carousel" });
  assert.deepEqual(homeAction(grid, "menu"), { intent: "menu" });
  assert.deepEqual(homeAction(grid, "context"), { intent: "context" });
});

test("the list layout is one column of games", () => {
  const list = { platform: 0, platforms: 2, game: 3, count: 10, cols: 5, rows: 8, focus: "grid", layout: "list" };
  assert.deepEqual(homeAction(list, "down"), { game: 4 });
  assert.deepEqual(homeAction(list, "up"), { game: 2 });
  assert.deepEqual(homeAction({ ...list, game: 0 }, "up"), { focus: "carousel" });
  assert.deepEqual(homeAction(list, "page_down"), { game: 9 }, "a page of rows");
  assert.deepEqual(homeAction(list, "accept"), { intent: "open" });
});

test("the cover flow layout is one row of games", () => {
  const flow = { platform: 0, platforms: 2, game: 3, count: 10, cols: 5, rows: 7, focus: "grid", layout: "cover-flow" };
  assert.deepEqual(homeAction(flow, "right"), { game: 4 });
  assert.deepEqual(homeAction(flow, "left"), { game: 2 });
  assert.deepEqual(homeAction({ ...flow, game: 0 }, "left"), { game: 0 }, "no wrap");
  assert.deepEqual(homeAction(flow, "up"), { focus: "carousel" }, "up leaves from any cover");
  assert.deepEqual(homeAction(flow, "down"), {});
  assert.deepEqual(homeAction(flow, "page_down"), { game: 9 }, "a page of covers");
  assert.deepEqual(homeAction(flow, "page_up"), { game: 0 });
  assert.deepEqual(homeAction(flow, "accept"), { intent: "open" });
});

test("the wheel moves like the list, the hero rows like cover flow", () => {
  const at = { platform: 0, platforms: 2, game: 3, count: 10, cols: 5, rows: 7, focus: "grid" };
  assert.deepEqual(homeAction({ ...at, layout: "wheel" }, "down"), { game: 4 });
  assert.deepEqual(homeAction({ ...at, layout: "wheel", game: 0 }, "up"), { focus: "carousel" });
  for (const layout of ["hero", "hero-bottom"]) {
    assert.deepEqual(homeAction({ ...at, layout }, "right"), { game: 4 }, layout);
    assert.deepEqual(homeAction({ ...at, layout }, "up"), { focus: "carousel" }, layout);
    assert.deepEqual(homeAction({ ...at, layout }, "down"), {}, layout);
  }
});

test("shelves move along a row and between rows", () => {
  const lengths = [3, 10, 1];
  assert.deepEqual(shelfMove({ row: 1, col: 5, lengths }, "up"), { row: 0, col: 2 }, "onto a shorter row's end");
  assert.deepEqual(shelfMove({ row: 1, col: 5, lengths }, "down"), { row: 2, col: 0 });
  assert.deepEqual(shelfMove({ row: 2, col: 0, lengths }, "down"), { row: 2, col: 0 }, "the last row stays");
  assert.equal(shelfMove({ row: 0, col: 1, lengths }, "up"), "out-up");
  assert.deepEqual(shelfMove({ row: 1, col: 9, lengths }, "right"), { row: 1, col: 9 });
  assert.deepEqual(shelfMove({ row: 1, col: 1, lengths, page: 6 }, "page_down"), { row: 1, col: 7 });
  assert.deepEqual(shelfMove({ row: 1, col: 4, lengths }, "last"), { row: 1, col: 9 });
  const at = { platform: 0, platforms: 2, game: 0, count: 14, cols: 5, rows: 6, focus: "grid", layout: "shelves" };
  assert.deepEqual(homeAction({ ...at, shelf: { row: 0, col: 0, lengths } }, "up"), { focus: "carousel" });
  assert.deepEqual(homeAction({ ...at, shelf: { row: 0, col: 0, lengths } }, "down"), { shelf: { row: 1, col: 0 } });
});

test("keys stand in for the pad", () => {
  assert.equal(keyAction("Enter"), "accept");
  assert.equal(keyAction("Escape"), "back");
  assert.equal(keyAction("Tab"), "menu");
  assert.equal(keyAction(" "), "context");
  assert.equal(keyAction("PageDown"), "page_down");
  assert.equal(keyAction("x"), null);
});
