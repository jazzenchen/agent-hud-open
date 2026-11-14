/* The resting HUD on the selected screen: a preview, its mode and that mode's
 * own rows, then its look. */
import { modePatch } from "../../lib/mode-patch";
import type { Snapshot } from "@agent-hud/protocol";
import type { HUDConfig } from "@agent-hud/hud/model";
import { Row, Section } from "../SettingsPrimitives";
import { Segments } from "../SettingsControls";
import { SmallHUDPreview } from "../SmallHUDPreview";
import { AgentSelection } from "./AgentSelection";
import { displayControls, type DisplayProps } from "./controls";

/** What a Simple mark draws, in a sentence. */
function usageSubtitle(
  style: HUDConfig["simpleStyle"],
  t: DisplayProps["t"],
): string {
  switch (style) {
    case "rings":
      return t("One ring per window round the logo, the first outermost.");
    case "segments":
      return t("One ring round the logo, split into a segment per window.");
    case "bars":
      return t("One bar per window beside the logo, the first on top.");
    case "numbers":
      return t("The most used window's percentage beside the logo.");
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
      ? t("A small floating panel, with no glow.")
      : config.hasNotch
        ? t("Uses this screen's notch.")
        : t("No notch here, so it draws a stand-in bar.");

  const dragHint =
    config.mode === "notch"
      ? t(
          "Drag the HUD on screen to move it. Dragging the notch switches to Simple.",
        )
      : t("Drag the HUD on screen to move it.");

  return (
    <Section title="HUD" footer={dragHint}>
      <SmallHUDPreview snapshot={snapshot} config={config} active={active} />
      <Row anchor="hud-mode" title={t("Mode")} subtitle={modeSubtitle}>
        <Segments
          label={t("Mode")}
          value={config.mode}
          options={[
            ["notch", t("Notch")],
            ["simple", t("Simple")],
          ]}
          change={(value) =>
            update(modePatch(config, value as HUDConfig["mode"]))
          }
        />
      </Row>
      {config.mode === "simple" && (
        <>
          {slider("simpleSize", t("Size"), 20, 60, 1, pt)}
          {slider(
            "simpleGapScale",
            t("Logo spacing"),
            0.1,
            0.6,
            0.05,
            (value) => pt(value * config.simpleSize),
          )}
          <Row
            title={t("Usage style")}
            subtitle={usageSubtitle(config.simpleStyle, t)}
          >
            <Segments
              label={t("Usage style")}
              value={config.simpleStyle}
              options={[
                ["rings", t("Rings")],
                ["segments", t("Segments")],
                ["bars", t("Bars")],
                ["numbers", t("Numbers")],
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
                ? t("Bar thickness")
                : t("Ring thickness"),
              1,
              4,
              0.5,
              (value) =>
                `${(config.simpleStyle === "bars" ? value + 0.5 : value).toFixed(1)} pt`,
            )}
          {config.simpleStyle !== "numbers" &&
            toggle(
              "showsNumbers",
              t("Show numbers"),
              t("The most used window's percentage beside each logo."),
            )}
          {toggle(
            "separatesAgents",
            t("Separate agents"),
            t("Hovering a logo opens only its agent."),
          )}
        </>
      )}
      <AgentSelection
        snapshot={snapshot}
        config={config}
        update={update}
        t={t}
      />
      <Row
        title={t("Appearance")}
        subtitle={
          config.hasNotch && config.mode === "notch"
            ? t("The notch stays black; its panel uses this.")
            : undefined
        }
      >
        <Segments
          label={t("HUD appearance")}
          value={config.appearance}
          options={[
            ["system", t("System")],
            ["light", t("Light")],
            ["dark", t("Dark")],
          ]}
          change={(value) =>
            update({ appearance: value as HUDConfig["appearance"] })
          }
        />
      </Row>
      {slider("solidTransparency", t("Transparency"), 0, 1, 0.05, percent)}
    </Section>
  );
}
