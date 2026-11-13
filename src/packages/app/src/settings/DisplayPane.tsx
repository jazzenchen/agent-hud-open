/* Settings → Display: the screen picked at the top and what its HUD shows at
 * all, the HUD and its panel there, then what every screen shares, then the
 * resets. Each section lives in ./display. */
import { defaultHUDConfig } from "@agent-hud/hud/model";
import type { NativeHUDMonitor } from "../lib/use-hud-geometry";
import { Menu, ToggleRow } from "./SettingsControls";
import {
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
  const several = monitors.length > 1;
  const shared = { config, update, t };
  return (
    <Page>
      <Section
        title={t("On this screen", "这块屏幕显示")}
        footer={
          config.hidden
            ? t(
                "Nothing shows here, cards included. The menu bar's Hide HUD still turns it off on every screen.",
                "这块屏幕什么都不显示，卡片也不弹。菜单栏的「隐藏 HUD」仍然管所有屏幕。",
              )
            : undefined
        }
      >
        {several && (
          <Row
            anchor="screen-picker"
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
        )}
        <ToggleRow
          title={t("Show the HUD on this screen", "在这块屏幕上显示 HUD")}
          checked={!config.hidden}
          change={(value) => update({ hidden: !value })}
        />
        {!config.hidden && (
          <>
            <ToggleRow
              title={t("Messages", "消息")}
              subtitle={t(
                "Cards for what agents say, and the tail that keeps them.",
                "智能体发来的消息卡片，和收着它们的尾部。",
              )}
              checked={!config.hidesMessages}
              change={(value) => update({ hidesMessages: !value })}
            />
            <ToggleRow
              title={t("Requests", "交互请求")}
              subtitle={t(
                "Cards that ask you to approve or answer, and the head that keeps them.",
                "等你批准或回答的卡片，和收着它们的头部。",
              )}
              checked={!config.hidesRequests}
              change={(value) => update({ hidesRequests: !value })}
            />
            <ToggleRow
              title={t("Agent messages in flight", "智能体互发消息")}
              subtitle={t(
                "The paper plane when one agent messages another.",
                "一个智能体给另一个发消息时飞过的纸飞机。",
              )}
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
