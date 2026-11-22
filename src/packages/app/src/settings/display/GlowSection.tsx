/* The glow: round the notch, and round the bar a Simple HUD leaves when it
 * auto-hides. Either way its colours are the agents' usage and it breathes
 * faster while one works. The notch picks a style and an effect up front,
 * every slider behind Fine-tune; a bar's glow is always the soft one, so it
 * shows only what that draws. A Simple HUD without a bar has no glow. */
import type { HUDConfig } from "@agent-hud/hud/model";
import { Menu, ToggleRow } from "../SettingsControls";
import { MoreRows, Row, Section } from "../SettingsPrimitives";
import { displayControls, type DisplayProps } from "./controls";

export function GlowSection({ config, update, t }: DisplayProps) {
  const bar =
    config.mode === "simple" && config.autoHide && config.restStyle === "bar";
  if (config.mode === "simple" && !bar) return null;
  const { slider, toggle, pt, px, percent, seconds, rows } = displayControls({
    config,
    update,
    t,
  });
  // Every style but Soft draws a grid of marks, with its own sliders.
  const grid = !bar && config.glowStyle !== "blur";
  return (
    <Section title={t("Glow")}>
      <ToggleRow
        title={t("Show glow")}
        subtitle={
          bar
            ? t(
                "Round the bar while the HUD is hidden: each agent's colour by its usage, yellow from 70% and red from 90%, breathing faster while one works.",
              )
            : t(
                "Round the notch: each agent's colour by its usage, yellow from 70% and red from 90%, breathing faster while one works.",
              )
        }
        checked={config.glowEnabled}
        change={(glowEnabled) => update({ glowEnabled })}
      />
      {config.glowEnabled && (
        <>
          {!bar && (
            <Row title={t("Style")}>
              <Menu
                label={t("Glow style")}
                value={config.glowStyle}
                options={[
                  ["blur", t("Soft")],
                  ["dots", t("Dots")],
                  ["ascii", t("ASCII  .:-=+*#%@")],
                  ["blocks", t("Blocks  ░▒▓█")],
                  ["braille", t("Braille  ⠁⠃⠇⡇⣿")],
                  ["binary", t("Binary  0 1")],
                ]}
                change={(value) =>
                  update({ glowStyle: value as HUDConfig["glowStyle"] })
                }
                width={160}
              />
            </Row>
          )}
          <Row title={t("Effect")}>
            <Menu
              label={t("Glow effect")}
              value={config.glowEffect}
              options={[
                ["breathe", t("Breathe")],
                ["flow", t("Flow")],
                ["scan", t("Scan")],
                ["ripple", t("Ripple")],
                ["shimmer", t("Shimmer")],
                ["boot", t("Boot")],
              ]}
              change={(value) =>
                update({ glowEffect: value as HUDConfig["glowEffect"] })
              }
              width={160}
            />
          </Row>
          <MoreRows label={t("Fine-tune")}>
            {grid && (
              <>
                {slider("glowGridPitch", t("Grid pitch"), 4, 12, 1, pt)}
                {slider(
                  "glowGridDensity",
                  t("Density"),
                  0.5,
                  1.5,
                  0.05,
                  percent,
                )}
                {slider("glowGridCore", t("Solid"), 0, 8, 1, rows)}
                {slider("glowGridFade", t("Fade"), 0, 8, 1, rows)}
              </>
            )}
            {slider("glowBrightness", t("Brightness"), 0.2, 1, 0.05, percent)}
            {!grid && (
              <>
                {slider("glowRange", t("Range"), 0, 36, 1, px)}
                {slider("glowBlur", t("Feather"), 0, 36, 1, px)}
                {toggle("glowOutwardOnly", t("Outward only"))}
              </>
            )}
            {slider("breathSeconds", t("Working period"), 1, 12, 0.5, seconds)}
            {slider("idleBreathSeconds", t("Idle period"), 1, 24, 0.5, seconds)}
            {config.glowEffect === "breathe" &&
              slider("breathAmplitude", t("Breath depth"), 0, 1, 0.05, percent)}
          </MoreRows>
        </>
      )}
    </Section>
  );
}
