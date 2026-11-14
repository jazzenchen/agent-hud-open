/* Settings → Display: the screen picked at the top and what its HUD shows at
 * all, the HUD and its panel there, then what every screen shares, then the
 * resets. Each section lives in ./display. */
import { defaultHUDConfig } from "@agent-hud/hud/model";
import { useT } from "@agent-hud/ui/format-context";
import type { NativeHUDMonitor } from "../lib/use-hud-geometry";
import { Menu, ToggleRow } from "./SettingsControls";
import {
  Page,
  Row,
  Section,
  type SettingsPaneProps,
} from "./SettingsPrimitives";
import { AllScreensSection } from "./display/AllScreensSection";
import { GlowSection } from "./display/GlowSection";
import { HUDSection } from "./display/HUDSection";
import { PanelSection } from "./display/PanelSection";
import { ResetSection } from "./display/ResetSection";

export function DisplayPane({
  snapshot,
  active = true,
  config: supplied,
  update,
  monitors = [],
  selectedScreenId,
  onSelectScreen,
}: SettingsPaneProps & {
  monitors?: NativeHUDMonitor[];
  selectedScreenId: string;
  onSelectScreen: (id: string) => void;
}) {
  const config = { ...defaultHUDConfig, ...supplied },
    t = useT();
  const screenName = (monitor: NativeHUDMonitor) =>
    monitor.name || t("Screen {{number}}", { number: monitor.index + 1 });
  const selected =
    monitors.length > 1
      ? monitors.find((monitor) => monitor.screenId === selectedScreenId)
      : undefined;
  const several = monitors.length > 1;
  const shared = { config, update, t };
  return (
    <Page>
      <Section
        title={t("On this screen")}
        footer={
          config.hidden
            ? t(
                "Nothing shows here, cards included. The menu bar's Hide HUD still turns it off on every screen.",
              )
            : undefined
        }
      >
        {several && (
          <Row
            anchor="screen-picker"
            title={t("Screen")}
            subtitle={t("The screen whose HUD you're setting up.")}
          >
            <Menu
              label={t("Screen")}
              value={selectedScreenId}
              options={monitors.map((monitor) => [
                monitor.screenId,
                screenName(monitor),
              ])}
              change={onSelectScreen}
              width={200}
            />
          </Row>
        )}
        <ToggleRow
          title={t("Show the HUD on this screen")}
          checked={!config.hidden}
          change={(value) => update({ hidden: !value })}
        />
        {!config.hidden && (
          <>
            <ToggleRow
              title={t("Messages")}
              subtitle={t(
                "Cards for what agents say, and the tail that keeps them.",
              )}
              checked={!config.hidesMessages}
              change={(value) => update({ hidesMessages: !value })}
            />
            <ToggleRow
              title={t("Requests")}
              subtitle={t(
                "Cards that ask you to approve or answer, and the head that keeps them.",
              )}
              checked={!config.hidesRequests}
              change={(value) => update({ hidesRequests: !value })}
            />
            <ToggleRow
              title={t("Agent messages in flight")}
              subtitle={t("The paper plane when one agent messages another.")}
              checked={!config.hidesFlights}
              change={(value) => update({ hidesFlights: !value })}
            />
          </>
        )}
      </Section>
      <HUDSection snapshot={snapshot} active={active} {...shared} />
      <GlowSection {...shared} />
      <PanelSection snapshot={snapshot} {...shared} />
      <AllScreensSection {...shared} />
      <ResetSection
        {...shared}
        screenName={selected ? screenName(selected) : undefined}
      />
    </Page>
  );
}
