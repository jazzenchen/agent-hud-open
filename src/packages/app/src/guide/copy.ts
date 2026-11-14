// The words of the guide's window beside the HUD. The window sits right
// beside the HUD, with an arrow at it, so the words name no side.
import type { HUDConfig } from "@agent-hud/hud/model";
import { isChinese, type Language, type Translate } from "@agent-hud/ui/format";

export type Place = Pick<HUDConfig, "mode" | "edge" | "requiresOptionToOpen">;

/** A step's or a news item's text: an English catalog key, or a product's
 * own English/Chinese pair (transitional, until products use catalogs). */
export type GuideText = string | readonly [en: string, zh: string];

export function guideText(
  t: Translate,
  language: Language,
  text: GuideText,
): string {
  if (typeof text === "string") return t(text);
  return isChinese(language) ? text[1] : text[0];
}

/** The first step, before and after the panel has been looked at. */
export function hudStepText(
  place: Place,
  looked: boolean,
  t: Translate,
): { title: string; body: string } {
  if (looked)
    return {
      title: t("That's it"),
      body: t("Point at the HUD and it opens. Move away and it closes."),
    };
  return {
    title: t("Agent HUD is here"),
    body: place.requiresOptionToOpen
      ? t(
          "Hold Option and rest the pointer on the HUD and its quota panel opens. New messages from your agents, and requests waiting on you, show up here too.",
        )
      : t(
          "Rest the pointer on the HUD and its quota panel opens. New messages from your agents, and requests waiting on you, show up here too.",
        ),
  };
}

export function modeStepText(
  screens: number,
  t: Translate,
): { title: string; body: string } {
  return {
    title: t("Display mode"),
    body:
      screens > 1
        ? t(
            "Pick the one that suits you, or drag the HUD to wherever feels right. Each screen has its own.",
          )
        : t(
            "Pick the one that suits you, or drag the HUD to wherever feels right.",
          ),
  };
}
