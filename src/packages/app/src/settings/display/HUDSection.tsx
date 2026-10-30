/* The resting HUD on the selected screen: a preview, its mode and that mode's
 * own rows, then its look. */
import type { Snapshot } from "@agent-hud/protocol";
import type { HUDConfig } from "@agent-hud/hud/model";
import { Row, Section, Segments } from "../SettingsPrimitives";
import { SmallHUDPreview } from "../SettingsSamples";
import { displayControls, type DisplayProps } from "./controls";

export function HUDSection({
  snapshot,
  active,
  config,
  update,
  t,
}: DisplayProps & { snapshot: Snapshot; active: boolean }) {
  const { slider, toggle, pt, percent } = displayControls({
    config,
    update,
    t,
  });

  const modeSubtitle =
    config.mode === "simple"
      ? t("A small floating panel, with no glow.", "悬浮的小面板，没有光晕。")
      : config.mode === "logos"
        ? t(
            "Docks on a screen edge and opens inward.",
            "停靠在屏幕边缘，向内展开。",
          )
        : config.hasNotch
          ? t("Uses this screen's notch.", "使用这块屏幕的刘海。")
          : t(
              "No notch here, so it draws a stand-in bar.",
              "这块屏幕没有刘海，画一条替代条。",
            );

  const dragHint =
    config.mode === "notch"
      ? t(
          "Drag the HUD on screen to move it. Dragging the notch switches to Dynamic.",
          "直接拖动屏幕上的 HUD 来移动它；拖动刘海会切换为「动态」。",
        )
      : t(
          "Drag the HUD on screen to move it.",
          "直接拖动屏幕上的 HUD 来移动它。",
        );

  const separateAgents = toggle(
    "separatesAgents",
    t("Separate agents", "按智能体分开"),
    t("Hovering a logo opens only its agent.", "悬停标志只展开该智能体。"),
  );

  return (
    <Section title="HUD" footer={dragHint}>
      <SmallHUDPreview snapshot={snapshot} config={config} active={active} />
      <Row title={t("Mode", "形态")} subtitle={modeSubtitle}>
        <Segments
          label={t("Mode", "形态")}
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
      {config.mode === "simple" && (
        <>
          {slider("simpleSize", t("Size", "大小"), 20, 60, 1, pt)}
          {slider(
            "simpleGapScale",
            t("Logo spacing", "Logo 间距"),
            0.1,
            0.6,
            0.05,
            (value) => pt(value * config.simpleSize),
          )}
          {toggle(
            "showsRings",
            t("Show rings", "显示圆环"),
            t(
              "Each agent's most used quota, round its logo.",
              "标志外圈显示用得最多的额度。",
            ),
          )}
          {config.showsRings &&
            slider(
              "ringWidth",
              t("Ring thickness", "圆环粗细"),
              1,
              4,
              0.5,
              (value) => `${value.toFixed(1)} pt`,
            )}
          {toggle(
            "showsNumbers",
            t("Show numbers", "显示数字"),
            t("Quota percentage beside each logo.", "标志旁显示额度百分比。"),
          )}
          {separateAgents}
        </>
      )}
      {config.mode === "logos" && (
        <>
          {slider("logoSize", t("Logo size", "Logo 大小"), 20, 60, 1, pt)}
          {slider(
            "logoEdgeDistance",
            t("Edge distance", "边缘距离"),
            0,
            20,
            1,
            pt,
          )}
          {slider(
            "gapScale",
            t("Logo spacing", "Logo 间距"),
            0.1,
            0.6,
            0.05,
            (value) => pt(value * config.logoSize),
          )}
          {toggle(
            "showsLogos",
            t("Show logos", "显示 Logo"),
            t("Off keeps only the backdrop.", "关闭后只留背景。"),
          )}
          {separateAgents}
        </>
      )}
      <Row
        title={t("Appearance", "外观")}
        subtitle={
          config.hasNotch && config.mode === "notch"
            ? t(
                "The notch stays black; its panel uses this.",
                "刘海保持纯黑，展开的面板用这个外观。",
              )
            : undefined
        }
      >
        <Segments
          label={t("HUD appearance", "HUD 外观")}
          value={config.appearance}
          options={[
            ["system", t("System", "跟随系统")],
            ["light", t("Light", "浅色")],
            ["dark", t("Dark", "深色")],
          ]}
          change={(value) =>
            update({ appearance: value as HUDConfig["appearance"] })
          }
        />
      </Row>
      {slider(
        "solidTransparency",
        t("Transparency", "透明度"),
        0,
        1,
        0.05,
        percent,
      )}
    </Section>
  );
}
