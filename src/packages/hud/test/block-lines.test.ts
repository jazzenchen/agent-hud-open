import { test } from "node:test";
import assert from "node:assert/strict";
import { blockLines, perLine } from "../src/panel/block-lines.ts";

const blocks = (...keys: string[]) => keys.map((key) => ({ key }));
const lines = (spans: Record<string, string>, keys: string[], width: number) =>
  blockLines(blocks(...keys), (key) => spans[key], width, 10).map((line) => [
    line.blocks.map((b) => b.key).join("+"),
    line.columns,
  ]);

test("a half needs 230 points, a third or a quarter 200", () => {
  assert.equal(perLine("half", 470, 10), 2);
  assert.equal(perLine("half", 469, 10), 1);
  assert.equal(perLine("third", 680, 10), 3);
  assert.equal(perLine("third", 500, 10), 2);
  assert.equal(perLine("quarter", 920, 10), 4);
  assert.equal(perLine("quarter", 680, 10), 3);
  assert.equal(perLine(undefined, 920, 10), 1);
  // A compact block keeps 130 points: six at 960 points, four at 720, three at 540.
  assert.equal(perLine("sixth", 920, 10, 130), 6);
  assert.equal(perLine("sixth", 680, 10, 130), 4);
  assert.equal(perLine("sixth", 500, 10, 130), 3);
});

test("blocks asking the same share fill lines, the rest keep their cells", () => {
  const thirds = { a: "third", b: "third", c: "third", d: "third" };
  assert.deepEqual(lines(thirds, ["a", "b", "c", "d", "e"], 680), [
    ["a+b+c", 3],
    ["d", 3],
    ["e", 1],
  ]);
  // A half left alone takes its line, as it always has.
  assert.deepEqual(
    lines({ a: "half", b: "half", c: "half" }, ["a", "b", "c"], 500),
    [
      ["a+b", 2],
      ["c", 1],
    ],
  );
  // Different shares do not mix on a line.
  assert.deepEqual(lines({ a: "half", b: "third" }, ["a", "b"], 900), [
    ["a", 1],
    ["b", 3],
  ]);
});
