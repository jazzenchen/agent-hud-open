/* Settings the HUD shares on every screen: bot logos, when the panel
 * opens and closes, and how long a message waits in the dock's tail. */
import { Slide } from "../SettingsControls";
import { Section } from "../SettingsPrimitives";
import { displayControls, type DisplayProps } from "./controls";

/** The fade slider's last stop, past 12 seconds: messages never go by
 * themselves (saved as 0). */
const NEVER = 13;

export function AllScreensSection({ config, update, t }: DisplayProps) {
  const { slider, toggle, ms } = displayControls({ config, update, t });
  const fade = config.messageFadeSeconds;
  return (
    <Section title={t("All screens", "所有屏幕")}>
      {toggle(
        "botLogos",
        t("Bot logos", "Bot 形象"),
        t("Agents show as bots while they work.", "智能体工作时显示为 Bot。"),
      )}
      {toggle(
        "requiresOptionToOpen",
        t("Press Option to open", "按 Option 才展开"),
        t("Hovering alone leaves the panel closed.", "仅悬停不会展开面板。"),
      )}
      {toggle(
        "openImmediatelyAtTop",
        t("Open immediately at the top edge", "鼠标触顶立即展开"),
        t("Skips the hover delay.", "跳过展开延迟。"),
      )}
      {slider("hoverDelayMs", t("Hover delay", "展开延迟"), 0, 1500, 50, ms)}
      {slider(
        "collapseDelayMs",
        t("Collapse delay", "收起延迟"),
        0,
        1500,
        50,
        ms,
      )}
      <Slide
        title={t("Messages fade every", "消息消失间隔")}
        value={fade > 0 ? Math.min(fade, NEVER - 1) : NEVER}
        min={3}
        max={NEVER}
        step={1}
        format={(value) =>
          value >= NEVER
            ? t("Never", "不消失")
            : t(`${Math.round(value)} s`, `${Math.round(value)} 秒`)
        }
        change={(value) =>
          update({ messageFadeSeconds: value >= NEVER ? 0 : value })
        }
      />
    </Section>
  );
}
