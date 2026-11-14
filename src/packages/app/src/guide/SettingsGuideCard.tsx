// The tour's card in Settings: right under (or over) the row it is about,
// with an arrow at it, the rest of the window dimmed. The last step, the
// Star, has no row and sits in the middle.
import type { CSSProperties } from "react";
import { Check, Star, X } from "lucide-react";
import { Button } from "@agent-hud/ui/components/button";
import type { Snapshot } from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { guideText } from "./copy";
import type { SettingsStep } from "./plan";
import { Ring, type Box } from "./Spotlight";

const width = 340;
const gap = 14;

/** Below the target when it fits, else above it, its right edge on the
 * target's, where a row's switch is; beside a tall target (the agent list)
 * when neither fits; in the window's corner as a last resort. */
function placement(box: Box | null): {
  card: CSSProperties;
  arrow: { side: "top" | "bottom" | "left"; style: CSSProperties } | null;
} {
  if (!box)
    return {
      card: { left: `calc(50% - ${width / 2}px)`, top: "calc(50% - 100px)" },
      arrow: null,
    };
  const tall = 200;
  const right = Math.max(16, window.innerWidth - (box.left + box.width));
  if (box.top + box.height + gap + tall < window.innerHeight)
    return {
      card: { right, top: box.top + box.height + gap },
      arrow: { side: "top", style: { top: -7, right: 36 } },
    };
  if (box.top - gap - tall > 0)
    return {
      card: { right, bottom: window.innerHeight - box.top + gap },
      arrow: { side: "bottom", style: { bottom: -7, right: 36 } },
    };
  const left = box.left + box.width + gap;
  if (left + width + 16 < window.innerWidth) {
    const top = Math.min(
      Math.max(16, box.top + 24),
      window.innerHeight - tall - 16,
    );
    return {
      card: { left, top },
      arrow: { side: "left", style: { left: -7, top: 28 } },
    };
  }
  return { card: { right: 24, bottom: 24 }, arrow: null };
}

export function SettingsGuideCard({
  step,
  box,
  snapshot,
  count,
  t,
  next,
  back,
  close,
  star,
}: {
  /** The row's step, or null for the Star. */
  step: SettingsStep | null;
  box: Box | null;
  snapshot: Snapshot;
  count: [number, number];
  t: Translate;
  next: () => void;
  /** To the step before, which may be beside the HUD. */
  back: () => void;
  close: () => void;
  star: () => void;
}) {
  const { language } = useFormat();
  const { card, arrow } = placement(step ? box : null);
  const done = step?.done?.(snapshot) ?? false;
  return (
    <>
      {step && box ? <Ring box={box} dim /> : <div className="guide-scrim" />}
      <aside
        className="guide-card"
        aria-label={t("Guide")}
        style={{ ...card, width }}
      >
        {arrow && (
          <span
            className="guide-arrow"
            data-side={arrow.side}
            style={arrow.style}
            aria-hidden
          />
        )}
        <header>
          <span>
            {t("Getting started · {{step}} / {{steps}}", {
              step: count[0],
              steps: count[1],
            })}
          </span>
          <Button
            size="icon-xs"
            variant="ghost"
            aria-label={t("Close the guide")}
            onClick={close}
          >
            <X />
          </Button>
        </header>
        {step ? (
          <>
            <h2>{guideText(t, language, step.title)}</h2>
            <p>{guideText(t, language, step.body)}</p>
            <footer>
              <Button
                size="sm"
                variant="ghost"
                onClick={back}
                className="mr-auto"
              >
                {t("Back [step]")}
              </Button>
              {done && (
                <span className="guide-done">
                  <Check size={13} strokeWidth={2.6} />
                  {t("On")}
                </span>
              )}
              <Button size="sm" onClick={next}>
                {t("Next")}
              </Button>
            </footer>
          </>
        ) : (
          <>
            <h2>{t("Thanks for your support")}</h2>
            <p>
              {t(
                "Agent HUD is open source. If it helps you, a Star on GitHub helps more people find it.",
              )}
            </p>
            <footer>
              <Button
                size="sm"
                variant="ghost"
                onClick={back}
                className="mr-auto"
              >
                {t("Back [step]")}
              </Button>
              <Button size="sm" variant="outline" onClick={close}>
                <Check />
                {t("Done")}
              </Button>
              <Button size="sm" onClick={star}>
                <Star />
                {t("Star on GitHub")}
              </Button>
            </footer>
          </>
        )}
      </aside>
    </>
  );
}
