import { useRef, useState } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@agent-hud/ui/components/select";
import { TooltipProvider } from "@agent-hud/ui/components/tooltip";
import { defaultHUDConfig, type HUDConfig } from "@agent-hud/hud/model";
import type { NativeHUDMonitor } from "../lib/use-hud-geometry";
import {
  Divider,
  Menu,
  Row,
  secondary,
  Section,
  Segments,
  Slide,
  SmallButton,
  ToggleRow,
  useText,
  type SettingsPaneProps,
} from "./SettingsPrimitives";
import { PanelOrderRows, PartSample, SmallHUDPreview } from "./SettingsSamples";
import { CodexModelSettings } from "./CodexModelSettings";
import type { PanelLayout } from "@agent-hud/protocol";

const styleKeys = [
  "mode",
  "glowStyle",
  "glowEffect",
  "breathSeconds",
  "idleBreathSeconds",
  "breathAmplitude",
  "glowRange",
  "glowBlur",
  "glowBrightness",
  "glowOutwardOnly",
  "glowGridPitch",
  "glowGridCore",
  "glowGridFade",
  "glowGridDensity",
  "edge",
  "offset",
  "logoSize",
  "logoEdgeDistance",
  "gapScale",
  "showsLogos",
  "simpleSize",
  "simpleGapScale",
  "ringWidth",
  "showsRings",
  "showsNumbers",
  "separatesAgents",
  "appearance",
  "solidTransparency",
] as const;

const layoutDefault = {
  sessionList: true,
  quotaDetails: true,
  resetCountdown: true,
  tokenUsage: true,
  panelWidth: defaultHUDConfig.panelWidth,
  layoutDensity: defaultHUDConfig.layoutDensity,
  panelSections: defaultHUDConfig.panelSections,
  panelOrder: defaultHUDConfig.panelOrder,
};

export function DisplayPane({
  snapshot,
  action,
  active = true,
  config: supplied,
  update,
  monitors = [],
  selectedScreenId,
  onSelectScreen,
  layout,
  onLayout,
}: SettingsPaneProps & {
  monitors?: NativeHUDMonitor[];
  selectedScreenId: string;
  onSelectScreen: (id: string) => void;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
}) {
  const config = { ...defaultHUDConfig, ...supplied },
    t = useText(snapshot);
  const previousStyle = useRef<Partial<HUDConfig> | null>(null);
  const previousLayout = useRef<Partial<HUDConfig> | null>(null);
  const [revision, setRevision] = useState(0);
  const change = <K extends keyof HUDConfig>(key: K, value: HUDConfig[K]) =>
    update({ [key]: value });
  const resetStyle = () => {
    previousStyle.current = Object.fromEntries(
      styleKeys.map((key) => [key, config[key]]),
    );
    update({
      ...Object.fromEntries(
        styleKeys.map((key) => [key, defaultHUDConfig[key]]),
      ),
      mode: config.hasNotch ? "notch" : "logos",
    });
    setRevision(revision + 1);
  };
  const resetLayout = () => {
    previousLayout.current = {
      sessionList: config.sessionList,
      quotaDetails: config.quotaDetails,
      resetCountdown: config.resetCountdown,
      tokenUsage: config.tokenUsage,
      panelWidth: config.panelWidth,
      layoutDensity: config.layoutDensity,
      panelSections: [...config.panelSections],
      panelOrder: [...config.panelOrder],
    };
    update(layoutDefault);
    setRevision(revision + 1);
  };
  const revertStyle = () => {
    if (previousStyle.current) {
      const previous = previousStyle.current;
      previousStyle.current = null;
      update(previous);
      setRevision(revision + 1);
    }
  };
  const revertLayout = () => {
    if (previousLayout.current) {
      const previous = previousLayout.current;
      previousLayout.current = null;
      update(previous);
      setRevision(revision + 1);
    }
  };
  const pt = (v: number) => `${Math.round(v)} pt`,
    percent = (v: number) => `${Math.round(v * 100)}%`,
    seconds = (v: number) => t(`${v.toFixed(1)} s`, `${v.toFixed(1)} 秒`);
  const slider = (
    key: keyof HUDConfig,
    title: string,
    min: number,
    max: number,
    step: number,
    format: (v: number) => string,
  ) => (
    <Slide
      title={title}
      value={config[key] as number}
      min={min}
      max={max}
      step={step}
      format={format}
      change={(value) => update({ [key]: value })}
    />
  );
  const toggle = (key: keyof HUDConfig, title: string, subtitle?: string) => (
    <ToggleRow
      title={title}
      subtitle={subtitle}
      checked={config[key] as boolean}
      change={(value) => update({ [key]: value })}
    />
  );
  const layoutActions = (
    <>
      <SmallButton disabled={!previousLayout.current} onClick={revertLayout}>
        {t("Revert to Previous", "恢复之前")}
      </SmallButton>
      <SmallButton onClick={resetLayout}>
        {t("Reset to Default", "恢复默认")}
      </SmallButton>
    </>
  );
  const grid = config.glowStyle !== "blur";
  const modeSubtitle =
    config.mode === "simple"
      ? t(
          "A floating panel on a screen edge, with no glow behind it.",
          "悬浮在屏幕边缘的小面板，没有背景光晕。",
        )
      : config.mode === "logos"
        ? t(
            "Dock on a screen edge and open inward.",
            "停靠在屏幕边缘，向内展开。",
          )
        : config.hasNotch
          ? t("Use this display's physical notch.", "使用这块屏幕的真实刘海。")
          : t(
              "No notch here — the notch shape draws a stand-in bar instead.",
              "这块屏幕没有刘海，用刘海形状画一条替代条。",
            );
  return (
    <TooltipProvider delayDuration={300}>
      <div
        className="source-settings-pane"
        style={{
          width: "100%",
          maxWidth: 640,
          display: "flex",
          flexDirection: "column",
          gap: 28,
        }}
      >
        <Section
          title={t("Screens", "屏幕")}
          subtitle={t("Set each display on its own.", "每块屏幕单独设置。")}
        >
          {monitors.length > 1 && (
            <>
              <Row title={t("Display", "显示器")}>
                <Select value={selectedScreenId} onValueChange={onSelectScreen}>
                  <SelectTrigger
                    aria-label={t("Display", "显示器")}
                    style={{ width: 200 }}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {monitors.map((monitor) => (
                      <SelectItem
                        key={monitor.screenId}
                        value={monitor.screenId}
                      >
                        {monitor.name ||
                          `${t("Display", "显示器")} ${monitor.index + 1}`}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Row>
              <Divider />
            </>
          )}
          <Row title={t("HUD", "HUD 形态")} subtitle={modeSubtitle}>
            <Segments
              label="HUD"
              value={config.mode}
              options={[
                ["notch", t("Notch", "刘海")],
                ["simple", t("Simple", "简约")],
                ["logos", t("Dynamic", "动态")],
              ]}
              change={(value) =>
                update({
                  mode: value as HUDConfig["mode"],
                  ...(config.mode === "notch" &&
                  value !== "notch" &&
                  config.hasNotch &&
                  config.edge === "top" &&
                  config.offset === 0.5
                    ? { offset: 0 }
                    : {}),
                })
              }
            />
          </Row>
          <p
            style={{
              ...secondary,
              margin: 0,
              padding: "0 16px 12px",
              fontSize: 11,
            }}
            title={t(
              "Dragging is supported by the native HUD on this machine. This preview does not drag.",
              "拖动仅适用于本机的原生 HUD，此预览不支持拖动。",
            )}
          >
            {t(
              "Drag the visible HUD directly. Dragging the notch switches to Dynamic.",
              "直接拖动可见的 HUD。拖动刘海会切换为「动态」。",
            )}
          </p>
          <Divider />
          <Row
            title={t("HUD appearance", "HUD 外观")}
            subtitle={
              config.hasNotch && config.mode === "notch"
                ? t(
                    "The notch's row stays black; the panel and cards that open from it take this look.",
                    "刘海那一行始终是纯黑；展开的面板和卡片使用这里的外观。",
                  )
                : undefined
            }
          >
            <Segments
              label={t("HUD appearance", "灵动岛外观")}
              value={config.appearance}
              options={[
                ["dark", t("Dark", "深色")],
                ["light", t("Light", "浅色")],
                ["system", t("Match system", "跟随系统")],
              ]}
              change={(value) =>
                change("appearance", value as HUDConfig["appearance"])
              }
            />
          </Row>
          <Divider />
          {slider(
            "solidTransparency",
            t("Transparency", "透明度"),
            0,
            1,
            0.05,
            percent,
          )}
          <Divider />
          <Row
            title={t("Layout density", "布局密度")}
            subtitle={t(
              "Spacing of this display's expanded panel.",
              "调整这块屏幕展开面板的内容间距。",
            )}
          >
            <Segments
              label={t("Layout density", "布局密度")}
              value={config.layoutDensity}
              options={[
                ["comfortable", t("Comfortable", "宽松")],
                ["normal", t("Normal", "普通")],
                ["compact", t("Compact", "紧凑")],
              ]}
              change={(value) =>
                change("layoutDensity", value as HUDConfig["layoutDensity"])
              }
            />
          </Row>
          <Divider />
          <Row
            title={t("Position", "位置")}
            subtitle={t(
              "Return to a reachable position at the top of this display.",
              "回到这块屏幕的顶部安全位置。",
            )}
          >
            <SmallButton
              onClick={() =>
                update({
                  edge: "top",
                  offset: config.hasNotch && config.mode !== "notch" ? 0 : 0.5,
                })
              }
            >
              {t("Reset Position", "重置位置")}
            </SmallButton>
          </Row>
          <Divider />
          <Row
            title={t("HUD style", "HUD 样式")}
            subtitle={t(
              "Mode, position, sizes, appearance and glow go back to their defaults; Revert to Previous undoes it.",
              "形态、位置、大小、外观与光晕恢复默认；「恢复之前」可以撤销。",
            )}
          >
            <SmallButton
              disabled={!previousStyle.current}
              onClick={revertStyle}
            >
              {t("Revert to Previous", "恢复之前")}
            </SmallButton>
            <SmallButton onClick={resetStyle}>
              {t("Reset to Default", "恢复默认")}
            </SmallButton>
          </Row>
          <Divider />
          <Row
            title={t("Panel layout", "面板布局")}
            subtitle={t(
              "This display has its own layout. Reset and Revert affect only this display.",
              "这块屏幕使用独立布局；恢复默认和恢复之前只影响这块屏幕。",
            )}
          >
            {layoutActions}
          </Row>
          {config.mode !== "notch" && (
            <>
              <Divider />
              {toggle(
                "separatesAgents",
                t("Separate agents", "按 agent 分开"),
                t(
                  "Hovering a mark opens that agent's own panel instead of the panel for every agent.",
                  "悬停某个标志只展开该 agent 的面板，不再展开全部 agent 的面板。",
                ),
              )}
            </>
          )}
          {config.mode === "logos" && (
            <>
              <Divider />
              {slider("logoSize", t("Logo size", "Logo 大小"), 20, 60, 1, pt)}
              <Divider />
              {slider(
                "logoEdgeDistance",
                t("Logo edge distance", "Logo 离屏幕边缘"),
                0,
                20,
                1,
                pt,
              )}
              <Divider />
              {slider(
                "gapScale",
                t("Logo spacing", "Logo 间距"),
                0.1,
                0.6,
                0.05,
                (value) => pt(value * config.logoSize),
              )}
              <Divider />
              {toggle(
                "showsLogos",
                t("Show logos", "显示 Logo"),
                t(
                  "Off leaves the backdrop alone, still where the logos would have been.",
                  "关掉只留背景；背景仍占着这些 Logo 的位置和宽度。",
                ),
              )}
            </>
          )}
          {config.mode === "simple" && (
            <>
              <Divider />
              <SmallHUDPreview
                snapshot={snapshot}
                config={config}
                active={active}
              />
              {slider("simpleSize", t("Size", "大小"), 20, 60, 1, pt)}
              <Divider />
              {slider(
                "simpleGapScale",
                t("Logo spacing", "Logo 间距"),
                0.1,
                0.6,
                0.05,
                (value) => pt(value * config.simpleSize),
              )}
              <Divider />
              {toggle(
                "showsRings",
                t("Show rings", "显示圆环"),
                t(
                  "Each agent's most used quota window, as a ring round its mark.",
                  "每个 agent 用得最多的额度窗口，画成套在标志外的圆环。",
                ),
              )}
              {config.showsRings && (
                <>
                  <Divider />
                  {slider(
                    "ringWidth",
                    t("Ring thickness", "圆环粗细"),
                    1,
                    4,
                    0.5,
                    (value) => `${value.toFixed(1)} pt`,
                  )}
                </>
              )}
              <Divider />
              {toggle(
                "showsNumbers",
                t("Show numbers", "显示数字"),
                t(
                  "The quota percentage, using your Usage display setting.",
                  "按「用量显示」设置，在标志旁显示额度百分比。",
                ),
              )}
            </>
          )}
        </Section>
        {config.mode !== "simple" && (
          <Section
            title={
              config.mode === "logos"
                ? t("Backdrop glow", "背景光晕")
                : t("Notch glow", "灵动岛光晕")
            }
          >
            <SmallHUDPreview
              snapshot={snapshot}
              config={config}
              active={active}
            />
            <Row title={t("Glow style", "光晕样式")}>
              <Menu
                label={t("Glow style", "光晕样式")}
                value={config.glowStyle}
                options={[
                  ["blur", t("Soft", "柔光")],
                  ["dots", t("Dots", "点阵")],
                  ["ascii", t("ASCII  .:-=+*#%@", "字符  .:-=+*#%@")],
                  ["blocks", t("Blocks  ░▒▓█", "色块  ░▒▓█")],
                  ["braille", t("Braille  ⠁⠃⠇⡇⣿", "盲文  ⠁⠃⠇⡇⣿")],
                  ["binary", t("Binary  0 1", "二进制  0 1")],
                ]}
                change={(value) =>
                  change("glowStyle", value as HUDConfig["glowStyle"])
                }
                width={160}
              />
            </Row>
            <Divider />
            <Row
              title={t("Effect", "动效")}
              subtitle={t(
                "Always in motion: the working period while an agent runs, slower when idle.",
                "一直在动：工作时按工作周期，空闲时慢下来。",
              )}
            >
              <Menu
                label={t("Effect", "动效")}
                value={config.glowEffect}
                options={[
                  ["breathe", t("Breathe", "呼吸")],
                  ["flow", t("Flow", "流光")],
                  ["scan", t("Scan", "扫描")],
                  ["ripple", t("Ripple", "涟漪")],
                  ["shimmer", t("Shimmer", "闪烁")],
                  ["boot", t("Boot", "启动")],
                ]}
                change={(value) =>
                  change("glowEffect", value as HUDConfig["glowEffect"])
                }
              />
            </Row>
            <Divider />
            {grid && (
              <>
                {slider("glowGridPitch", t("Grid pitch", "点距"), 4, 12, 1, pt)}
                <Divider />
                {slider(
                  "glowGridDensity",
                  t("Density", "密度"),
                  0.5,
                  1.5,
                  0.05,
                  percent,
                )}
                <Divider />
                {slider("glowGridCore", t("Solid", "实心"), 0, 8, 1, (value) =>
                  t(`${Math.round(value)} rows`, `${Math.round(value)} 排`),
                )}
                <Divider />
                {slider("glowGridFade", t("Fade", "渐隐"), 0, 8, 1, (value) =>
                  t(`${Math.round(value)} rows`, `${Math.round(value)} 排`),
                )}
                <Divider />
              </>
            )}
            {slider(
              "glowBrightness",
              t("Brightness", "光晕亮度"),
              0.2,
              1,
              0.05,
              percent,
            )}
            {!grid && (
              <>
                <Divider />
                {slider(
                  "glowRange",
                  t("Glow range", "光晕范围"),
                  0,
                  36,
                  1,
                  (value) => `${Math.round(value)} px`,
                )}
                <Divider />
                {slider(
                  "glowBlur",
                  t("Feather", "羽化"),
                  0,
                  36,
                  1,
                  (value) => `${Math.round(value)} px`,
                )}
                <Divider />
                {toggle(
                  "glowOutwardOnly",
                  t("Outward only", "仅向外扩散"),
                  t(
                    "Keep the rim defined and fade gently outward.",
                    "贴近 HUD 的边缘更浓，向外逐渐变淡。",
                  ),
                )}
              </>
            )}
            <Divider />
            {slider(
              "breathSeconds",
              t("Working period", "工作时周期"),
              1,
              12,
              0.5,
              seconds,
            )}
            <Divider />
            {slider(
              "idleBreathSeconds",
              t("Idle period", "空闲时周期"),
              1,
              24,
              0.5,
              seconds,
            )}
            {config.glowEffect === "breathe" && (
              <>
                <Divider />
                {slider(
                  "breathAmplitude",
                  t("Breath depth", "呼吸幅度"),
                  0,
                  1,
                  0.05,
                  percent,
                )}
              </>
            )}
            <p
              style={{
                ...secondary,
                fontSize: 11,
                padding: "0 16px 12px",
                margin: 0,
              }}
            >
              {!grid && config.glowEffect === "breathe"
                ? t(
                    "The glow breathes at the working period while an agent runs and slows to the idle period otherwise; it rests only while detection is paused or no agent has a reading to show.",
                    "Agent 运行时按工作周期呼吸，空闲时按空闲周期放慢；只有暂停检测或还没有可显示的读数时才静止。",
                  )
                : (grid
                    ? t(
                        "Density sets how much of each cell a mark fills; raise it for smaller gaps, and past 100% marks overlap. Solid is how many rows keep full strength; fade is how many it dies away over. ",
                        "密度决定点和字符占满格子的程度，调高后空隙更小，超过 100% 会互相重叠。实心是满强度的排数，渐隐是它之后淡出的排数。",
                      )
                    : "") +
                  t(
                    "The preview always plays the selected effect; the HUD plays it at the working period while an agent runs and at the idle period otherwise.",
                    "上方预览会一直播放所选动效；HUD 上 Agent 运行时按工作周期播放，空闲时按空闲周期放慢。",
                  )}
            </p>
          </Section>
        )}
        <Section
          title={t("Expanded panel", "展开面板")}
          subtitle={t(
            "A small sample for each part. These switches change only the display selected above.",
            "每项展示一个简短示例，开关只修改上方所选屏幕的展开内容。",
          )}
        >
          {toggle("sessionList", t("Session list", "会话列表"))}
          <PartSample
            kind="sessions"
            enabled={config.sessionList}
            config={config}
            t={t}
          />
          <Divider />
          {toggle("quotaDetails", t("Quota details", "额度明细"))}
          <PartSample
            kind="quota"
            enabled={config.quotaDetails}
            config={config}
            t={t}
          />
          {toggle("resetCountdown", t("Reset countdown", "重置倒计时"))}
          <Divider />
          {toggle("tokenUsage", t("Token usage", "Token 消耗"))}
          <PartSample
            kind="tokens"
            enabled={config.tokenUsage}
            config={config}
            t={t}
          />
          <Divider />
          <CodexModelSettings
            snapshot={snapshot}
            action={action}
            active={active}
            layout={layout}
            onLayout={onLayout}
          />
          <PanelOrderRows
            snapshot={snapshot}
            config={config}
            update={update}
            t={t}
          />
          <Divider />
          <Row
            title={t("Display layout", "这块屏幕的布局")}
            subtitle={
              Object.entries(layoutDefault).every(
                ([key, value]) =>
                  JSON.stringify(config[key as keyof typeof layoutDefault]) ===
                  JSON.stringify(value),
              )
                ? t("Default layout · this display", "默认布局 · 当前屏幕")
                : t("Custom layout · this display", "自定义布局 · 当前屏幕")
            }
          >
            {layoutActions}
          </Row>
        </Section>
        <Section title={t("Logos", "Logo")}>
          {toggle(
            "botLogos",
            t("Bot logos", "Bot 形象"),
            t(
              "Working agents stay bots. Idle logos occasionally become bots for a few seconds. Bots also appear on hover, for approvals and replies.",
              "工作时保持 Bot；空闲时偶尔变成 Bot 活动几秒，再回到 Logo。悬停、等待批准或刚回复时也显示 Bot。",
            ),
          )}
        </Section>
        <Section title={t("Hover behavior", "悬停交互")}>
          {toggle(
            "requiresOptionToOpen",
            t("Press Option to open", "按 Option 才展开"),
            t(
              "Hovering alone leaves it closed; press Option while hovering to open the panel.",
              "指针经过不展开，悬停时按一下 Option 打开面板。",
            ),
          )}
          <Divider />
          {toggle(
            "openImmediatelyAtTop",
            t("Open immediately at the top edge", "鼠标触顶立即展开"),
            t(
              "Skip the hover delay when the pointer reaches the screen's top edge directly above the HUD.",
              "鼠标移到 HUD 正上方的屏幕顶边时，跳过展开延迟。",
            ),
          )}
          <Divider />
          {slider(
            "hoverDelayMs",
            t("Hover delay", "展开延迟"),
            0,
            1500,
            50,
            (value) => `${Math.round(value)} ms`,
          )}
          <Divider />
          {slider(
            "collapseDelayMs",
            t("Collapse delay", "收起延迟"),
            0,
            1500,
            50,
            (value) => `${Math.round(value)} ms`,
          )}
        </Section>
        <Section title={t("Menu bar", "菜单栏")}>
          <ToggleRow
            title={t("Show menu bar icon", "显示菜单栏图标")}
            subtitle={t(
              "Quick access to usage, refresh and settings.",
              "快速查看用量、刷新与打开设置。",
            )}
            checked={snapshot.settings.showMenuBarIcon !== false}
            change={(value) => {
              void action("set_setting", { key: "showMenuBarIcon", value });
            }}
          />
        </Section>
      </div>
    </TooltipProvider>
  );
}
