/* Settings the HUD shares on every screen: bot logos and when the panel
 * opens and closes. */
import { Section } from "../SettingsPrimitives";
import { displayControls, type DisplayProps } from "./controls";

export function AllScreensSection({ config, update, t }: DisplayProps) {
  const { slider, toggle, ms } = displayControls({ config, update, t });
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
    </Section>
  );
}
