/* What a Simple HUD leaves at its edge while nothing needs it, and, when that
 * is a bar, the bar's length, thickness and colour. */
import type { HUDConfig, RestStyle } from "@agent-hud/hud/model";
import { Row } from "../SettingsPrimitives";
import { Segments } from "../SettingsControls";
import { displayControls, type DisplayProps } from "./controls";

/** The automatic swatch: light on a dark HUD, dark on a light one. */
const AUTOMATIC = "linear-gradient(135deg, #f2f2f7 50%, #1c1c1e 50%)";

/** What each choice does, in a sentence or two. */
function restSubtitle(style: RestStyle, t: DisplayProps["t"]): string {
  switch (style) {
    case "logos":
      return t(
        "Only the logos stay at the edge. Pointing at them, or a request or message, brings the HUD back.",
      );
    case "bar":
      return t(
        "Only a bar stays at the edge; it turns yellow or red as a usage limit nears. Pointing at it, or a request or message, brings the HUD back.",
      );
    case "hidden":
      return t(
        "Nothing stays on screen. Pointing at the edge where the HUD was, or a request or message, brings it back.",
      );
    default:
      return t("The whole HUD stays on screen.");
  }
}

/** The bar's colours: automatic, then system colours that no usage level
 * wears (yellow and red mean a limit is near). */
function barColours(t: DisplayProps["t"]): [string, string][] {
  return [
    ["auto", t("Automatic")],
    ["#ffffff", t("White")],
    ["#8e8e93", t("Graphite")],
    ["#0a84ff", t("Blue")],
    ["#bf5af2", t("Purple")],
    ["#ff375f", t("Pink")],
    ["#30d158", t("Green")],
  ];
}

export function RestRows({ config, update, t }: DisplayProps) {
  const { slider, pt } = displayControls({ config, update, t });
  return (
    <>
      <Row title={t("When idle")} subtitle={restSubtitle(config.restStyle, t)}>
        <Segments
          label={t("When idle")}
          value={config.restStyle}
          options={[
            ["dock", t("Full")],
            ["logos", t("Logos")],
            ["bar", t("Bar")],
            ["hidden", t("Hidden")],
          ]}
          change={(value) =>
            update({ restStyle: value as HUDConfig["restStyle"] })
          }
        />
      </Row>
      {config.restStyle === "bar" && (
        <>
          {slider("restBarLength", t("Length"), 24, 240, 4, pt)}
          {slider("restBarThickness", t("Thickness"), 2, 10, 1, pt)}
          <Row title={t("Colour")}>
            <div
              role="radiogroup"
              aria-label={t("Colour")}
              className="settings-swatches"
            >
              {barColours(t).map(([value, name]) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={config.restBarColor === value}
                  aria-label={name}
                  title={name}
                  className="settings-swatch"
                  style={{ background: value === "auto" ? AUTOMATIC : value }}
                  onClick={() => update({ restBarColor: value })}
                />
              ))}
            </div>
          </Row>
        </>
      )}
    </>
  );
}
