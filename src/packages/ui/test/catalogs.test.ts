import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, test } from "node:test";
import { hasCatalog, translate } from "../src/format/language.ts";

let sharedLoadReadiness: boolean | undefined;
// Vite accepts JSON imports without attributes. Give Node that metadata while
// executing the real loader and catalogs rather than mocking their contents.
const hooks = registerHooks({
  load(url, context, nextLoad) {
    if (url.endsWith("/zh-Hans.json"))
      sharedLoadReadiness = catalogReady("zh-Hans");
    return nextLoad(
      url,
      url.endsWith(".json")
        ? { ...context, importAttributes: { type: "json" } }
        : context,
    );
  },
});
after(() => hooks.deregister());
const { addCatalogs, catalogReady, loadCatalog } =
  await import("../src/catalogs.ts");

function pendingCatalog() {
  let resolve!: (catalog: { default: Record<string, string> }) => void;
  const promise = new Promise<{ default: Record<string, string> }>(
    (complete) => (resolve = complete),
  );
  return { promise, resolve };
}

const english = pendingCatalog();
const secondEnglish = pendingCatalog();
const chinese = pendingCatalog();
const failure = new Error("Extension catalog unavailable");
let englishLoads = 0;
// Product catalogs register synchronously before the first preload or render.
addCatalogs({
  en: () => {
    englishLoads++;
    return english.promise;
  },
  "zh-Hans": () => chinese.promise,
  ja: () => Promise.reject(failure),
});
addCatalogs({ en: () => secondEnglish.promise });

test("preloading English waits for every extension and reuses its completed promise", async () => {
  assert.equal(hasCatalog("en"), true);
  const loading = loadCatalog("en");
  assert.equal(loadCatalog("en"), loading);
  assert.equal(catalogReady("en"), false);
  english.resolve({ default: { "First extension": "First loaded" } });
  await english.promise;
  assert.equal(catalogReady("en"), false);
  secondEnglish.resolve({ default: { "Second extension": "Second loaded" } });
  await loading;
  assert.equal(catalogReady("en"), true);
  assert.equal(loadCatalog("en"), loading);
  assert.equal(englishLoads, 1);
  assert.equal(translate("en", "First extension"), "First loaded");
  assert.equal(translate("en", "Second extension"), "Second loaded");
});

test("readiness includes the shared catalog with shared keys taking precedence", async () => {
  const loading = loadCatalog("zh-Hans");
  assert.equal(catalogReady("zh-Hans"), false);
  chinese.resolve({
    default: { Settings: "Extension override", "Extension page": "扩展页面" },
  });
  await loading;
  assert.equal(sharedLoadReadiness, false);
  assert.equal(catalogReady("zh-Hans"), true);
  assert.equal(translate("zh-Hans", "Settings"), "设置");
  assert.equal(translate("zh-Hans", "Extension page"), "扩展页面");
});

test("a rejected catalog stays unready and preserves its rejection for Suspense", async () => {
  const loading = loadCatalog("ja");
  await assert.rejects(loading, failure);
  assert.equal(catalogReady("ja"), false);
  assert.equal(loadCatalog("ja"), loading);
  await assert.rejects(loadCatalog("ja"), failure);
});
