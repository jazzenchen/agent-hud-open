/* Settings the HUD shares on every screen: bot logos, when the panel
 * opens and closes, how long a message waits in the dock's tail, and the
 * clipboard bubble. */
import { Slide } from "../SettingsControls";
import { Section } from "../SettingsPrimitives";
import { displayControls, type DisplayProps } from "./controls";

/** A fade slider's last stop, past 12 seconds: it never goes by itself
 * (saved as 0). */
const NEVER = 13;

export function AllScreensSection({ config, update, t }: DisplayProps) {
  const { slider, toggle, ms } = displayControls({ config, update, t });
  const fadeSlide = (
    key: "messageFadeSeconds" | "clipboardFadeSeconds",
    title: string,
  ) => (
    <Slide
      title={title}
      value={config[key] > 0 ? Math.min(config[key], NEVER - 1) : NEVER}
      min={3}
      max={NEVER}
      step={1}
      format={(value) =>
        value >= NEVER
          ? t("Never")
          : t("{{seconds}} s", { seconds: Math.round(value) })
      }
      change={(value) => update({ [key]: value >= NEVER ? 0 : value })}
    />
  );
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
      {toggle(
        "clipboardBubble",
        t("Clipboard bubble"),
        t("Each copy shows a bubble that sends it to an agent session."),
      )}
      {config.clipboardBubble &&
        fadeSlide("clipboardFadeSeconds", t("Clipboard bubble fades after"))}
    </Section>
  );
}
