// What each release adds, as the update card tells it: two or three items a
// release, written when it is cut; a release with nothing to tell has no
// entry, and no card opens. A product build adds its own (`AppProps.news`).
// A target names where "Show me" goes: a Settings page, and the element on
// it marked `data-guide`.
import type { NewsEntry } from "./plan";

/** Marks English text as a catalog key; the guide translates it where it
 * shows it. */
const t = (key: string) => key;

export const openNews: NewsEntry[] = [
  {
    version: "0.6.3",
    items: [
      {
        title: t("Nine languages"),
        body: t(
          "Japanese, Korean, Traditional Chinese, French, German, Spanish and Brazilian Portuguese join English and Simplified Chinese. Pick one in Settings → General.",
        ),
        target: { page: "general" },
      },
      {
        title: t("More accounts found on their own"),
        body: t(
          "Qoder, QoderWork, TRAE, WorkBuddy, CodeBuddy, ZCode and MiniMax Code accounts come from each app's own sign-in, each region on its own.",
        ),
      },
      {
        title: t("Agents can start DeepSeek Harness sessions"),
        body: t("Through Agent HUD's plugin in a running Harness."),
      },
    ],
  },
  {
    version: "0.6.2",
    items: [
      {
        title: t(
          "A guide beside the HUD on first launch, and this note after each update",
        ),
        body: t("Both open again from the ⋯ menu in Settings."),
        target: { anchor: "more-menu" },
      },
    ],
  },
];
