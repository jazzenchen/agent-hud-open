// The interface language.

export type Language = "en" | "zh";

export const text = (language: Language, zh: string, en: string) =>
  language === "zh" ? zh : en;

/** The interface language for a setting: "zh…" is Chinese, "system" follows the system's first language. */
export function resolveLanguage(
  setting: string | null | undefined,
  system: string,
): Language {
  const language = !setting || setting === "system" ? system : setting;
  return language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

/** The interface language of a snapshot: the one Core resolved and writes
 * its own text in, else the setting as this web view resolves it. */
export function snapshotLanguage(
  snapshot:
    { language?: string; settings: { language: string } } | null | undefined,
): Language {
  if (snapshot?.language === "zh" || snapshot?.language === "en")
    return snapshot.language;
  return interfaceLanguage(snapshot?.settings.language);
}

/** The interface language for a setting on this machine. */
export function interfaceLanguage(
  setting: string | null | undefined,
): Language {
  return resolveLanguage(
    setting,
    typeof navigator === "undefined" ? "en-US" : navigator.language,
  );
}
