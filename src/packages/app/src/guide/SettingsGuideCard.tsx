// The tour's card in Settings: right under (or over) the row it is about,
// with an arrow at it, the rest of the window dimmed. The last step, the
// Star, has no row and sits in the middle.
import type { CSSProperties } from "react";
import { Check, Star, X } from "lucide-react";
import { Button } from "@agent-hud/ui/components/button";
import type { Snapshot } from "@agent-hud/protocol";
import type { Text } from "./copy";
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
  t: Text;
  next: () => void;
  /** To the step before, which may be beside the HUD. */
  back: () => void;
  close: () => void;
  star: () => void;
}) {
  const { card, arrow } = placement(step ? box : null);
  const done = step?.done?.(snapshot) ?? false;
  return (
    <>
      {step && box ? <Ring box={box} dim /> : <div className="guide-scrim" />}
      <aside
        className="guide-card"
        aria-label={t("Guide", "引导")}
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
            {t(
              `Getting started · ${count[0]} / ${count[1]}`,
              `上手引导 · ${count[0]} / ${count[1]}`,
            )}
          </span>
          <Button
            size="icon-xs"
            variant="ghost"
            aria-label={t("Close the guide", "关闭引导")}
            onClick={close}
          >
            <X />
          </Button>
        </header>
        {step ? (
          <>
            <h2>{t(...step.title)}</h2>
            <p>{t(...step.body)}</p>
            <footer>
              <Button
                size="sm"
                variant="ghost"
                onClick={back}
                className="mr-auto"
              >
                {t("Back", "上一步")}
              </Button>
              {done && (
                <span className="guide-done">
                  <Check size={13} strokeWidth={2.6} />
                  {t("On", "已开启")}
                </span>
              )}
              <Button size="sm" onClick={next}>
                {t("Next", "下一步")}
              </Button>
            </footer>
          </>
        ) : (
          <>
            <h2>{t("Thanks for your support", "谢谢你的支持")}</h2>
            <p>
              {t(
                "Agent HUD is open source. If it helps you, a Star on GitHub helps more people find it.",
                "Agent HUD 是开源项目。如果它帮到了你，可以去 GitHub 点个 Star，让更多人看到它。",
              )}
            </p>
            <footer>
              <Button
                size="sm"
                variant="ghost"
                onClick={back}
                className="mr-auto"
              >
                {t("Back", "上一步")}
              </Button>
              <Button size="sm" variant="outline" onClick={close}>
                <Check />
                {t("Done", "完成")}
              </Button>
              <Button size="sm" onClick={star}>
                <Star />
                {t("Star on GitHub", "去 GitHub 点个 Star")}
              </Button>
            </footer>
          </>
        )}
      </aside>
    </>
  );
}
