// The interface language, and the text written in it.
//
// Text is written in English in the code and looked up, through i18next, in
// the catalog of the language: `@agent-hud/protocol/locales/<code>.json`,
// which Core reads too. A key missing from a catalog shows in English.
// Placeholders are `{{name}}`; a counted text passes `count` and has
// `_one`/`_other` (…) forms in the catalogs, English's in en.json.
import i18next, { type TOptions } from "i18next";

/** The languages the app is written in, with their own names. */
export const languages = [
  ["en", "English"],
  ["zh-Hans", "简体中文"], // l10n-ignore: own name
  ["zh-Hant", "繁體中文"], // l10n-ignore: own name
  ["ja", "日本語"], // l10n-ignore: own name
  ["ko", "한국어"],
  ["fr", "Français"],
  ["de", "Deutsch"],
  ["es", "Español"],
  ["pt-BR", "Português (Brasil)"],
] as const;

export type Language = (typeof languages)[number][0];

const codes = new Set<string>(languages.map(([code]) => code));

export const isLanguage = (value: unknown): value is Language =>
  typeof value === "string" && codes.has(value);

/** The language a BCP 47 tag or setting names ("zh-CN", "zh-Hant-TW",
 * "pt-PT", "ja-JP" …), if the app is written in it. */
export function languageOfTag(tag: string): Language | null {
  const lower = tag.trim().replace(/_/g, "-").toLowerCase();
  const base = lower.split(/[-.@]/)[0];
  if (base === "zh")
    return /hant|-tw|-hk|-mo/.test(lower) && !lower.includes("hans")
      ? "zh-Hant"
      : "zh-Hans";
  if (base === "pt") return "pt-BR";
  return isLanguage(base) ? base : null;
}

/** The interface language for a setting: "system" (or nothing) follows the
 * system's languages, the first the app is written in; else English. */
export function resolveLanguage(
  setting: string | null | undefined,
  system: string | readonly string[],
): Language {
  if (setting && setting !== "system") return languageOfTag(setting) ?? "en";
  const preferred = typeof system === "string" ? [system] : system;
  for (const tag of preferred) {
    const language = languageOfTag(tag);
    if (language) return language;
  }
  return "en";
}

/** The locale Intl formats dates and numbers in for a language. */
export function locale(language: Language): string {
  switch (language) {
    case "zh-Hans":
      return "zh-CN";
    case "zh-Hant":
      return "zh-TW";
    case "en":
      return "en-US";
    case "pt-BR":
      return "pt-BR";
    default:
      return language;
  }
}

export const isChinese = (language: Language) => language.startsWith("zh");

/** Whether words are written without spaces between them. */
export const unspaced = (language: Language) =>
  isChinese(language) || language === "ja";

/** The separator between items of a list ("a, b" or "a、b"). */
export const listSeparator = (language: Language) =>
  unspaced(language) ? "、" : ", ";

const i18n = i18next.createInstance();
void i18n.init({
  initAsync: false,
  lng: "en",
  fallbackLng: "en",
  resources: {},
  keySeparator: false,
  nsSeparator: false,
  returnNull: false,
  returnEmptyString: false,
  interpolation: { escapeValue: false },
});

/** Adds a language's catalog. */
export function addCatalog(
  language: Language,
  catalog: Record<string, string>,
) {
  i18n.addResourceBundle(language, "translation", catalog, true, true);
}

export const hasCatalog = (language: Language) =>
  i18n.hasResourceBundle(language, "translation");

export type Translate = (key: string, values?: TOptions) => string;

/** The English text `key` in `language`, placeholders filled from `values`. */
export function translate(
  language: Language,
  key: string,
  values?: TOptions,
): string {
  return i18n.t(key, { ...values, lng: language }) as string;
}

/** `translate` fixed to one language. */
export const translator =
  (language: Language): Translate =>
  (key, values) =>
    translate(language, key, values);

/** The interface language of a snapshot: the one Core resolved and writes
 * its own text in, else the setting as this web view resolves it. */
export function snapshotLanguage(
  snapshot:
    { language?: string; settings: { language: string } } | null | undefined,
): Language {
  if (isLanguage(snapshot?.language)) return snapshot.language;
  // Cores before 0.7 wrote "zh".
  if (snapshot?.language === "zh") return "zh-Hans";
  return interfaceLanguage(snapshot?.settings.language);
}

const systemLanguages = (): readonly string[] =>
  typeof navigator === "undefined"
    ? ["en-US"]
    : navigator.languages?.length
      ? navigator.languages
      : [navigator.language];

/** The interface language for a setting on this machine. */
export function interfaceLanguage(
  setting: string | null | undefined,
): Language {
  return resolveLanguage(setting, systemLanguages());
}

/** Transitional: the Chinese or English text. Being replaced by
 * `translate`; removed once no caller is left. */
export const text = (language: Language, zh: string, en: string) =>
  isChinese(language) ? zh : en;
