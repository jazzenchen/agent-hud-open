/* Settings → Display: the HUD and its panel on the screen picked at the top,
 * then what every screen shares, then the resets. Each section lives in
 * ./display. */
import { defaultHUDConfig } from "@agent-hud/hud/model";
import type { NativeHUDMonitor } from "../lib/use-hud-geometry";
import {
  Menu,
  Page,
  Row,
  Section,
  useText,
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
    t = useText(snapshot);
  const screenName = (monitor: NativeHUDMonitor) =>
    monitor.name || `${t("Screen", "屏幕")} ${monitor.index + 1}`;
  const selected =
    monitors.length > 1
      ? monitors.find((monitor) => monitor.screenId === selectedScreenId)
      : undefined;
  const shared = { config, update, t };
  return (
    <Page>
      {monitors.length > 1 && (
        <Section>
          <Row
            title={t("Screen", "屏幕")}
            subtitle={t(
              "The screen whose HUD you're setting up.",
              "正在设置哪块屏幕上的 HUD。",
            )}
          >
            <Menu
              label={t("Screen", "屏幕")}
              value={selectedScreenId}
              options={monitors.map((monitor) => [
                monitor.screenId,
                screenName(monitor),
              ])}
              change={onSelectScreen}
              width={200}
            />
          </Row>
        </Section>
      )}
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
