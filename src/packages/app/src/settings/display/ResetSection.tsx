/* Back to defaults for the selected screen: the HUD's style, the panel's
 * layout, or just the position. Undo brings back what the last reset
 * replaced, until the page closes or another screen is picked. */
import { useRef, useState } from "react";
import { defaultHUDConfig, type HUDConfig } from "@agent-hud/hud/model";
import { Row, Section, SmallButton } from "../SettingsPrimitives";
import type { DisplayProps } from "./controls";

/** Everything "HUD style" resets: mode, place, sizes, look and glow. */
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

/** What "Panel layout" resets to. */
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

export function ResetSection({
  config,
  update,
  t,
  screenName,
}: DisplayProps & {
  /** Named only when there are several screens to tell apart. */
  screenName?: string;
}) {
  const previousStyle = useRef<Partial<HUDConfig> | null>(null);
  const previousLayout = useRef<Partial<HUDConfig> | null>(null);
  // The refs don't render; this does, so Undo enables and disables.
  const [revision, setRevision] = useState(0);

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
  const revertStyle = () => {
    if (previousStyle.current) {
      const previous = previousStyle.current;
      previousStyle.current = null;
      update(previous);
      setRevision(revision + 1);
    }
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
  const revertLayout = () => {
    if (previousLayout.current) {
      const previous = previousLayout.current;
      previousLayout.current = null;
      update(previous);
      setRevision(revision + 1);
    }
  };
  const resetPosition = () =>
    update({
      edge: "top",
      offset: config.hasNotch && config.mode !== "notch" ? 0 : 0.5,
    });

  // Whether this screen's panel already matches what "Panel layout" resets to.
  const layoutIsDefault = Object.entries(layoutDefault).every(
    ([key, value]) =>
      JSON.stringify(config[key as keyof typeof layoutDefault]) ===
      JSON.stringify(value),
  );

  const undo = t("Undo", "撤销"),
    reset = t("Reset", "重置");
  return (
    <Section
      title={t("Reset", "恢复默认")}
      footer={
        screenName
          ? t(`These change ${screenName} only.`, `只影响「${screenName}」。`)
          : undefined
      }
    >
      <Row
        title={t("HUD style", "HUD 样式")}
        subtitle={t(
          "Mode, position, sizes, look and glow.",
          "形态、位置、大小、外观和光晕。",
        )}
      >
        <SmallButton disabled={!previousStyle.current} onClick={revertStyle}>
          {undo}
        </SmallButton>
        <SmallButton onClick={resetStyle}>{reset}</SmallButton>
      </Row>
      <Row
        title={t("Panel layout", "面板布局")}
        tag={layoutIsDefault ? undefined : t("Custom", "自定义")}
        subtitle={t("Sections, order and density.", "内容、顺序和密度。")}
      >
        <SmallButton disabled={!previousLayout.current} onClick={revertLayout}>
          {undo}
        </SmallButton>
        <SmallButton disabled={layoutIsDefault} onClick={resetLayout}>
          {reset}
        </SmallButton>
      </Row>
      <Row
        title={t("Position", "位置")}
        subtitle={t("Back to the top of the screen.", "回到屏幕顶部。")}
      >
        <SmallButton onClick={resetPosition}>{reset}</SmallButton>
      </Row>
    </Section>
  );
}
