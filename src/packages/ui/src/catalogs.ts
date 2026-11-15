// Loads a language's catalog into the web view when it is first shown, so
// a window carries only the languages it shows.
import en from "@agent-hud/protocol/locales/en.json";
import { addCatalog, hasCatalog, type Language } from "./format/language.ts";

type Catalog = { default: Record<string, string> };

const loaders: Record<Exclude<Language, "en">, () => Promise<Catalog>> = {
  "zh-Hans": () => import("@agent-hud/protocol/locales/zh-Hans.json"),
  "zh-Hant": () => import("@agent-hud/protocol/locales/zh-Hant.json"),
  ja: () => import("@agent-hud/protocol/locales/ja.json"),
  ko: () => import("@agent-hud/protocol/locales/ko.json"),
  fr: () => import("@agent-hud/protocol/locales/fr.json"),
  de: () => import("@agent-hud/protocol/locales/de.json"),
  es: () => import("@agent-hud/protocol/locales/es.json"),
  "pt-BR": () => import("@agent-hud/protocol/locales/pt-BR.json"),
};

// English's catalog holds only its counted forms; the keys are its text.
addCatalog("en", en);

type Loaders = Partial<Record<Language, () => Promise<Catalog>>>;
const extra: Loaders[] = [];

/** Adds the catalogs an app built on this one carries for its own text;
 * register synchronously before any preload or window render. A key in both
 * is the shared one's. Registration after loading starts is not supported. */
export function addCatalogs(loaders: Loaders) {
  extra.push(loaders);
}

const loading = new Map<Language, Promise<void>>();
const ready = new Set<Language>();

/** Includes every registered extension and the shared catalog. */
export function catalogReady(language: Language): boolean {
  return ready.has(language);
}

/** Resolves once `language`'s catalog is in. The same promise every time,
 * so React's `use` can wait on it. */
export function loadCatalog(language: Language): Promise<void> {
  let promise = loading.get(language);
  if (!promise) {
    const own = extra.flatMap((loaders) => loaders[language] ?? []);
    const shared =
      language === "en" || hasCatalog(language) ? [] : [loaders[language]];
    // Added first, so the shared catalog's keys win.
    promise = Promise.all(own.map((load) => load())).then(async (modules) => {
      for (const module of modules) addCatalog(language, module.default);
      for (const load of shared) addCatalog(language, (await load()).default);
      ready.add(language);
    });
    loading.set(language, promise);
  }
  return promise;
}
