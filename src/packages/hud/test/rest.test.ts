import { test } from "node:test";
import assert from "node:assert/strict";
import {
  REST_EDGE,
  barPaint,
  foldStyle,
  mayRest,
  restShape,
  restZone,
  type RestGate,
} from "../src/scene/rest.ts";
import { compactMark, restingDockStyle } from "../src/scene/rest-style.ts";
import type { HUDEdge } from "../src/model.ts";

const idle: RestGate = {
  open: false,
  pointerInside: false,
  cardVisible: false,
  alert: false,
  ends: false,
  dragging: false,
  guide: false,
  typing: false,
};
const screen = { width: 1440, height: 900 };
const metrics = { logo: 12, gap: 10, radius: 15 };
const bar = { restBarLength: 64, restBarThickness: 4 };
/** A dock's strip parked at the middle of each edge, as `parkedRect` puts it. */
const strips: Record<
  HUDEdge,
  { x: number; y: number; width: number; height: number }
> = {
  top: { x: 660, y: 0, width: 120, height: 33 },
  bottom: { x: 660, y: 867, width: 120, height: 33 },
  left: { x: 0, y: 390, width: 33, height: 120 },
  right: { x: 1407, y: 390, width: 33, height: 120 },
};
const edges = Object.keys(strips) as HUDEdge[];
const across = (edge: HUDEdge) => edge === "top" || edge === "bottom";

/** How far `rect` sits from the screen edge the strip is parked at. */
function offEdge(
  edge: HUDEdge,
  rect: { x: number; y: number; width: number; height: number },
) {
  const strip = strips[edge];
  switch (edge) {
    case "top":
      return rect.y - strip.y;
    case "bottom":
      return strip.y + strip.height - (rect.y + rect.height);
    case "left":
      return rect.x - strip.x;
    case "right":
      return strip.x + strip.width - (rect.x + rect.width);
  }
}
const middle = (
  edge: HUDEdge,
  rect: { x: number; y: number; width: number; height: number },
) => (across(edge) ? rect.x + rect.width / 2 : rect.y + rect.height / 2);

test("only a Simple dock with Auto-hide on, leaving logos, a bar or nothing, ever folds", () => {
  const on = { mode: "simple", autoHide: true } as const;
  assert.equal(foldStyle({ ...on, restStyle: "dock" }), null);
  assert.equal(foldStyle({ ...on, mode: "notch", restStyle: "bar" }), null);
  assert.equal(foldStyle({ ...on, restStyle: "sideways" as never }), null);
  for (const style of ["logos", "bar", "hidden"] as const) {
    assert.equal(foldStyle({ ...on, restStyle: style }), style);
    // Off keeps the whole dock, and the choice for when it is on again.
    assert.equal(foldStyle({ ...on, autoHide: false, restStyle: style }), null);
  }
});

test("the dock folds only when nothing is out, nothing waits and the pointer is away", () => {
  assert.equal(mayRest(idle), true);
  for (const reason of Object.keys(idle) as (keyof RestGate)[])
    assert.equal(mayRest({ ...idle, [reason]: true }), false, reason);
});

test("a waiting request keeps the dock out, on a card or parked at its head", () => {
  assert.equal(mayRest({ ...idle, alert: true }), false);
  assert.equal(mayRest({ ...idle, ends: true }), false);
});

test("a dock that never folds draws exactly as before", () => {
  assert.deepEqual(restingDockStyle(undefined), {});
});

test("a dock that folds changes only its transition while it is out", () => {
  for (const style of ["logos", "bar", "hidden"] as const) {
    const shape = restShape(
      style,
      bar,
      strips.right,
      "right",
      screen,
      metrics,
      3,
    );
    const rest = {
      style,
      edge: "right" as const,
      folded: false,
      shape,
      zone: restZone(shape, "right"),
      logos: { mark: 12, pad: 4, gap: 8, radius: 10 },
      bar: null,
      reduceMotion: false,
    };
    assert.deepEqual(Object.keys(restingDockStyle(rest)), ["transition"]);
    assert.equal(
      restingDockStyle({ ...rest, reduceMotion: true }).transition,
      undefined,
    );
  }
});

test("a compact mark keeps its logo's size and its ring's place while out", () => {
  const sizes = { mark: 22, logo: 12, labelGap: 5, bar: 0, label: 0, font: 9 };
  const out = compactMark(false, sizes, false);
  assert.equal(out.mark.width, 22);
  assert.equal(out.button.gap, 5);
  assert.equal(out.ring.opacity, 1);
  const folded = compactMark(true, sizes, false);
  assert.equal(folded.mark.width, 12);
  assert.equal(folded.figures.maxWidth, 0);
  assert.equal(folded.ring.opacity, 0);
  assert.equal(compactMark(true, sizes, true).mark.transition, undefined);
});

test("every resting shape lies flush against its edge, centred on the dock", () => {
  for (const edge of edges)
    for (const style of ["logos", "bar", "hidden"] as const) {
      const shape = restShape(
        style,
        bar,
        strips[edge],
        edge,
        screen,
        metrics,
        3,
      );
      assert.equal(offEdge(edge, shape), 0, `${style} on ${edge}`);
      assert.equal(
        middle(edge, shape),
        middle(edge, strips[edge]),
        `${style} on ${edge}`,
      );
    }
});

test("the logos' strip is as long as three logos and as deep as one with its pad", () => {
  const shape = restShape("logos", bar, strips.top, "top", screen, metrics, 3);
  // pad 4 (a third of 12, rounded), gap 8 (three quarters of 10)
  assert.deepEqual(shape, {
    x: 690,
    y: 0,
    width: 4 * 2 + 3 * 12 + 2 * 8,
    height: 20,
  });
  const side = restShape(
    "logos",
    bar,
    strips.right,
    "right",
    screen,
    metrics,
    3,
  );
  assert.deepEqual(side, { x: 1420, y: 420, width: 20, height: 60 });
});

test("the bar takes its length and thickness, and stays on screen at a corner", () => {
  const shape = restShape(
    "bar",
    { restBarLength: 100, restBarThickness: 6 },
    strips.left,
    "left",
    screen,
    metrics,
    3,
  );
  assert.deepEqual(shape, { x: 0, y: 400, width: 6, height: 100 });
  const corner = { x: 1407, y: 0, width: 33, height: 40 };
  const high = restShape(
    "bar",
    { restBarLength: 200, restBarThickness: 4 },
    corner,
    "right",
    screen,
    metrics,
    1,
  );
  assert.deepEqual(high, { x: 1436, y: 0, width: 4, height: 200 });
});

test("a hidden dock leaves a strip of the edge as long as the dock", () => {
  for (const edge of edges) {
    const shape = restShape(
      "hidden",
      bar,
      strips[edge],
      edge,
      screen,
      metrics,
      3,
    );
    assert.equal(across(edge) ? shape.width : shape.height, 120);
    assert.equal(across(edge) ? shape.height : shape.width, REST_EDGE);
  }
});

test("the pointer's zone is at least a few points deep and still flush", () => {
  for (const edge of edges) {
    const thin = restShape(
      "bar",
      { restBarLength: 64, restBarThickness: 2 },
      strips[edge],
      edge,
      screen,
      metrics,
      3,
    );
    const zone = restZone(thin, edge);
    assert.equal(across(edge) ? zone.height : zone.width, REST_EDGE);
    assert.equal(offEdge(edge, zone), 0, edge);
    assert.equal(middle(edge, zone), middle(edge, thin), edge);
    const deep = restShape(
      "logos",
      bar,
      strips[edge],
      edge,
      screen,
      metrics,
      3,
    );
    assert.deepEqual(restZone(deep, edge), deep);
  }
});

test("the bar wears a low window's level over any colour, else its own", () => {
  const look = { restBarColor: "#0a84ff", solidTransparency: 0 };
  assert.equal(
    barPaint(look, ["normal", "critical", "warning"], false).fill,
    "#ff453a",
  );
  assert.equal(barPaint(look, ["normal", "warning"], false).fill, "#ffd23f");
  assert.equal(barPaint(look, ["normal", "stale"], false).fill, "#0a84ff");
  const auto = { restBarColor: "auto", solidTransparency: 0 };
  assert.equal(barPaint(auto, [], false).fill, "rgba(242,242,247,0.80)");
  assert.equal(barPaint(auto, [], true).fill, "rgba(28,28,30,0.80)");
  // Anything that is not a colour is automatic; full transparency still leaves it seen.
  assert.equal(
    barPaint({ restBarColor: "url(x)", solidTransparency: 1 }, [], false).fill,
    "rgba(242,242,247,0.24)",
  );
});
