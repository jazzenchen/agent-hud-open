// The interface language and calendar every formatter takes, provided once at
// the root of each window from the app's language setting.
import { createContext, useContext, useMemo, type ReactNode } from "react";
import { resolveLanguage, systemCalendar, type FormatSettings } from "./format";

const systemLocale = () =>
  typeof navigator === "undefined" ? "en-US" : navigator.language;

const FormatContext = createContext<FormatSettings>({
  quotaDisplay: "remaining",
  language: resolveLanguage("system", systemLocale()),
  calendar: systemCalendar(systemLocale()),
});

/** Provides the language for `setting` ("system", "en", "zh-Hans" …) and this machine's calendar. */
export function FormatProvider({
  setting,
  quotaDisplay = "remaining",
  children,
}: {
  setting: string | null | undefined;
  quotaDisplay?: FormatSettings["quotaDisplay"];
  children: ReactNode;
}) {
  const value = useMemo<FormatSettings>(() => {
    const locale = systemLocale();
    return {
      language: resolveLanguage(setting, locale),
      quotaDisplay,
      calendar: systemCalendar(locale),
    };
  }, [setting, quotaDisplay]);
  return (
    <FormatContext.Provider value={value}>{children}</FormatContext.Provider>
  );
}

export function useFormat(): FormatSettings {
  return useContext(FormatContext);
}
