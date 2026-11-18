import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

const script = readFileSync(
  new URL("../src/lib/startup-theme-bootstrap.mjs", import.meta.url),
  "utf8",
);

function start(appearance: unknown, light: boolean, delayed = false) {
  const classes = new Set<string>();
  const root = {
    dataset: {} as Record<string, string>,
    classList: {
      toggle(name: string, on: boolean) {
        if (on) classes.add(name);
        else classes.delete(name);
      },
    },
  };
  let mutation = () => {};
  let disconnected = false;
  const document = { documentElement: delayed ? null : root };
  runInNewContext(
    `${script}\ninitializeAppearance(${JSON.stringify(appearance)})`,
    {
      document,
      matchMedia: () => ({ matches: light }),
      MutationObserver: class {
        constructor(callback: () => void) {
          mutation = callback;
        }
        observe() {}
        disconnect() {
          disconnected = true;
        }
      },
    },
  );
  return {
    root,
    classes,
    insertRoot() {
      document.documentElement = root;
      mutation();
      return disconnected;
    },
  };
}

test("a new document uses the saved appearance before React can mount", () => {
  const light = start("light", false);
  assert.equal(light.root.dataset.theme, "light");
  assert.equal(light.root.dataset.initialAppearance, "light");
  assert.equal(light.classes.has("dark"), false);
  const dark = start("dark", true);
  assert.equal(dark.root.dataset.theme, "dark");
  assert.equal(dark.classes.has("dark"), true);
});

test("follow-system and unavailable preferences use the current system appearance", () => {
  for (const preference of ["system", null, "invalid"]) {
    assert.equal(start(preference, true).root.dataset.theme, "light");
    assert.equal(start(preference, false).root.dataset.theme, "dark");
  }
});

test("document-start initialization also works before the root element exists", () => {
  const page = start("light", false, true);
  assert.equal(page.insertRoot(), true);
  assert.equal(page.root.dataset.theme, "light");
});
