/* The resting HUD on the selected screen: a preview, its mode and that mode's
 * own rows, then its look. */
import type { Snapshot } from "@agent-hud/protocol";
import type { HUDConfig } from "@agent-hud/hud/model";
import { Row, Section } from "../SettingsPrimitives";
import { Segments } from "../SettingsControls";
import { SmallHUDPreview } from "../SmallHUDPreview";
import { displayControls, type DisplayProps } from "./controls";

/** What a Simple mark draws, in a sentence. */
function usageSubtitle(
  style: HUDConfig["simpleStyle"],
  t: DisplayProps["t"],
): string {
  switch (style) {
    case "rings":
      return t(
        "One ring per window round the logo, the first outermost.",
        "每项一圈，套在标志外，第一项在最外圈。",
      );
    case "segments":
      return t(
        "One ring round the logo, split into a segment per window.",
        "标志外一圈，按项分段，第一段在最上面。",
      );
    case "bars":
      return t(
        "One bar per window beside the logo, the first on top.",
        "标志旁每项一条，第一项在最上面。",
      );
    case "numbers":
      return t(
        "The most used window's percentage beside the logo.",
        "标志旁只写用得最多那项的百分比。",
      );
  }
}

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
      : config.hasNotch
        ? t("Uses this screen's notch.", "使用这块屏幕的刘海。")
        : t(
            "No notch here, so it draws a stand-in bar.",
            "这块屏幕没有刘海，画一条替代条。",
          );

  const dragHint =
    config.mode === "notch"
      ? t(
          "Drag the HUD on screen to move it. Dragging the notch switches to Simple.",
          "直接拖动屏幕上的 HUD 来移动它；拖动刘海会切换为「简约」。",
        )
      : t(
          "Drag the HUD on screen to move it.",
          "直接拖动屏幕上的 HUD 来移动它。",
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
          {toggle(
            "compactDock",
            t("Compact status bar", "精简状态栏"),
            t(
              "Shorten the bar and hide agents without readings or activity. Hover for details; turn off to restore your full layout.",
              "缩短栏目，收起无数据且空闲的智能体。悬停查看详情，关闭后恢复完整展示。",
            ),
          )}
          {!config.compactDock && (
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
              <Row
                title={t("Usage", "用量画法")}
                subtitle={usageSubtitle(config.simpleStyle, t)}
              >
                <Segments
                  label={t("Usage", "用量画法")}
                  value={config.simpleStyle}
                  options={[
                    ["rings", t("Rings", "年轮")],
                    ["segments", t("Segments", "分段环")],
                    ["bars", t("Bars", "进度条")],
                    ["numbers", t("Numbers", "数字")],
                  ]}
                  change={(value) =>
                    update({ simpleStyle: value as HUDConfig["simpleStyle"] })
                  }
                />
              </Row>
              {config.simpleStyle !== "numbers" &&
                slider(
                  "ringWidth",
                  config.simpleStyle === "bars"
                    ? t("Bar thickness", "进度条粗细")
                    : t("Ring thickness", "圆环粗细"),
                  1,
                  4,
                  0.5,
                  (value) =>
                    `${(config.simpleStyle === "bars" ? value + 0.5 : value).toFixed(1)} pt`,
                )}
              {config.simpleStyle !== "numbers" &&
                toggle(
                  "showsNumbers",
                  t("Show numbers", "显示数字"),
                  t(
                    "The most used window's percentage beside each logo.",
                    "标志旁写出用得最多那项的百分比。",
                  ),
                )}
            </>
          )}
          {toggle(
            "separatesAgents",
            t("Separate agents", "按智能体分开"),
            t(
              "Hovering a logo opens only its agent.",
              "悬停标志只展开该智能体。",
            ),
          )}
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
