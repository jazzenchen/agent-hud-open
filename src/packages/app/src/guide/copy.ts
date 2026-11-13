// The words of the guide's window beside the HUD. The window sits right
// beside the HUD, with an arrow at it, so the words name no side.
import type { HUDConfig } from "@agent-hud/hud/model";

export type Text = (en: string, zh: string) => string;
export type Place = Pick<HUDConfig, "mode" | "edge" | "requiresOptionToOpen">;

/** The first step, before and after the panel has been looked at. */
export function hudStepText(
  place: Place,
  looked: boolean,
  t: Text,
): { title: string; body: string } {
  if (looked)
    return {
      title: t("That's it", "就是这样"),
      body: t(
        "Point at the HUD and it opens. Move away and it closes.",
        "鼠标放上去，HUD 就展开；移走，就收起。",
      ),
    };
  const reach = place.requiresOptionToOpen
    ? t(
        "Hold Option and rest the pointer on the HUD",
        "按住 Option，把鼠标移到 HUD 上",
      )
    : t("Rest the pointer on the HUD", "把鼠标移到 HUD 上");
  return {
    title: t("Agent HUD is here", "Agent HUD 在这里"),
    body: t(
      `${reach} and its quota panel opens. New messages from your agents, and requests waiting on you, show up here too.`,
      `${reach}，停一会儿，额度面板就会展开。智能体的新消息、等你点头的请求，也会出现在这里。`,
    ),
  };
}

export function modeStepText(
  screens: number,
  t: Text,
): { title: string; body: string } {
  return {
    title: t("Display mode", "显示模式"),
    body:
      t(
        "Pick the one that suits you, or drag the HUD to wherever feels right.",
        "按你的习惯挑一种，或者直接拖着 HUD，放到你顺手的位置。",
      ) +
      (screens > 1 ? t(" Each screen has its own.", "每块屏幕各选各的。") : ""),
  };
}
