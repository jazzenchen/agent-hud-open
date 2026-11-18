/* Settings the HUD shares on every screen: bot logos, when the panel
 * opens and closes, and how long a message waits in the dock's tail. The
 * inspector's bubble is on Experimental. */
import { Section } from "../SettingsPrimitives";
import { displayControls, type DisplayProps } from "./controls";

export function AllScreensSection({ config, update, t }: DisplayProps) {
  const { slider, toggle, fadeSlide, ms } = displayControls({
    config,
    update,
    t,
  });
  return (
    <Section title={t("All screens")}>
      {toggle(
        "botLogos",
        t("Bot logos"),
        t("Agents show as bots while they work."),
      )}
      {toggle(
        "requiresOptionToOpen",
        t("Press Option to open"),
        t("Hovering alone leaves the panel closed."),
      )}
      {toggle(
        "openImmediatelyAtTop",
        t("Open immediately at the top edge"),
        t("Skips the hover delay."),
      )}
      {slider("hoverDelayMs", t("Hover delay"), 0, 1500, 50, ms)}
      {slider("collapseDelayMs", t("Collapse delay"), 0, 1500, 50, ms)}
      {fadeSlide("messageFadeSeconds", t("Messages fade every"))}
    </Section>
  );
}
