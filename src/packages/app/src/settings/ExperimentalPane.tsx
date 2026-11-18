/* Settings → Experimental: features still being tried out, each in a section
 * of its own. The inspector: the bubble each copy shows at the dock's end,
 * how long it stays, the shortcut that calls its window back, and the
 * browser extension that picks a part of a page for it. */
import { defaultHUDConfig } from "@agent-hud/hud/model";
import { useT } from "@agent-hud/ui/format-context";
import { ToggleRow } from "./SettingsControls";
import { Page, Section, type SettingsPaneProps } from "./SettingsPrimitives";
import { displayControls } from "./display/controls";
import { BrowserRow } from "./experimental/BrowserRow";
import { ShortcutRow } from "./experimental/ShortcutRow";

export function ExperimentalPane({
  config: supplied,
  update,
  active = true,
}: SettingsPaneProps) {
  const config = { ...defaultHUDConfig, ...supplied },
    t = useT();
  const { fadeSlide } = displayControls({ config, update, t });
  const bubble = config.clipboardBubble;
  return (
    <Page>
      <Section
        title={t("Inspector")}
        footer={t(
          "A pick from the browser extension opens the inspector even when Show after each copy is off.",
        )}
      >
        <ToggleRow
          title={t("Show after each copy")}
          subtitle={t(
            "Point at the bubble to send what you copied to an agent session.",
          )}
          checked={bubble}
          change={(value) => update({ clipboardBubble: value })}
        />
        {bubble && fadeSlide("clipboardFadeSeconds", t("Fades after"))}
        {bubble && (
          <ShortcutRow
            value={config.clipboardShortcut}
            change={(value) => update({ clipboardShortcut: value })}
            t={t}
          />
        )}
        <BrowserRow active={active} t={t} />
      </Section>
    </Page>
  );
}
