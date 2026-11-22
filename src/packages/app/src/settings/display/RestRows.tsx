/* Auto-hide for a Simple HUD, a section of its own: whether it steps back
 * against its edge while nothing needs it, what it leaves there, and, when
 * that is a bar, the bar's length, thickness and colour. The glow round the
 * bar is the Glow section's. */
import type { HUDConfig, RestStyle } from "@agent-hud/hud/model";
import { Row, Section } from "../SettingsPrimitives";
import { Segments, ToggleRow } from "../SettingsControls";
import { displayControls, type DisplayProps } from "./controls";

/** The automatic swatch: light on a dark HUD, dark on a light one. */
const AUTOMATIC = "linear-gradient(135deg, #f2f2f7 50%, #1c1c1e 50%)";

/** What each choice leaves at the edge, in a sentence. */
function restSubtitle(style: RestStyle, t: DisplayProps["t"]): string {
  switch (style) {
    case "bar":
      return t(
        "Only a bar stays, in the colour below, with the glow round it.",
      );
    case "hidden":
      return t("Nothing stays on screen.");
    default:
      return t("Only the logos stay.");
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

/** Auto-hide, for a Simple HUD only. */
export function AutoHideSection({ config, update, t }: DisplayProps) {
  if (config.mode !== "simple") return null;
  return (
    <Section title={t("Auto-hide")}>
      <RestRows config={config} update={update} t={t} />
    </Section>
  );
}

function RestRows({ config, update, t }: DisplayProps) {
  const { slider, pt } = displayControls({ config, update, t });
  // `dock`, saved by an earlier build, leaves the logos once Auto-hide is on.
  const style: RestStyle =
    config.restStyle === "dock" ? "logos" : config.restStyle;
  return (
    <>
      <ToggleRow
        title={t("Hide when idle")}
        subtitle={t(
          "While nothing needs it, the HUD steps back against its edge. Pointing at the edge, or a request or message, brings it back.",
        )}
        checked={config.autoHide}
        change={(autoHide) => update({ autoHide })}
      />
      {config.autoHide && (
        <Row title={t("At the edge")} subtitle={restSubtitle(style, t)}>
          <Segments
            label={t("At the edge")}
            value={style}
            options={[
              ["logos", t("Logos")],
              ["bar", t("Bar")],
              ["hidden", t("Nothing")],
            ]}
            change={(value) =>
              update({ restStyle: value as HUDConfig["restStyle"] })
            }
          />
        </Row>
      )}
      {config.autoHide && style === "bar" && (
        <>
          {slider("restBarLength", t("Length"), 24, 240, 4, pt)}
          {slider("restBarThickness", t("Thickness"), 0, 10, 1, pt)}
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
