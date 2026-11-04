import { Button } from "@agent-hud/ui/components/button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@agent-hud/ui/components/tooltip";
import { useFormat } from "@agent-hud/ui/format-context";
import type { quotaMetricAvailability } from "../quota-metrics";
import { useGround } from "../ground";
import { measures, palette, type Measure } from "./theme";

/** A block's measure buttons: quota, burn rate, reply rate, consumed and cost,
 * each with why it is unavailable where it is. */
export function MetricPicker({
  selected,
  choose,
  light,
  availability,
}: {
  selected: Measure;
  choose: (measure: Measure) => void;
  light: boolean;
  availability: Record<Measure, ReturnType<typeof quotaMetricAvailability>>;
}) {
  const zh = useFormat().language === "zh";
  const colors = palette(useGround(light));
  return (
    <TooltipProvider delayDuration={300}>
      <div
        className="hud-source-metric-picker"
        role="group"
        aria-label={zh ? "额度指标" : "Quota metric"}
        style={{ display: "flex", gap: 2, paddingRight: 2 }}
      >
        {measures.map(({ key, title, icon: Icon }) => {
          const status = availability[key];
          const hint = `${title} · ${status.hint}`;
          return (
            <Tooltip key={key}>
              <TooltipTrigger asChild>
                <span
                  title={hint}
                  tabIndex={status.selectable ? undefined : 0}
                  aria-label={status.selectable ? undefined : hint}
                  style={{ display: "inline-flex" }}
                >
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={title}
                    aria-pressed={selected === key}
                    disabled={!status.selectable}
                    onClick={() => choose(key)}
                    style={{
                      width: 20,
                      minWidth: 20,
                      height: 18,
                      minHeight: 18,
                      padding: 0,
                      borderRadius: 5,
                      color: selected === key ? colors.text : colors.tertiary,
                      background:
                        selected === key ? colors.selected : "transparent",
                      boxShadow: "none",
                    }}
                  >
                    <Icon
                      style={{ width: 10, height: 10 }}
                      strokeWidth={key === "burn" ? 2.6 : 2}
                      fill={key === "burn" ? "currentColor" : "none"}
                    />
                  </Button>
                </span>
              </TooltipTrigger>
              <TooltipContent
                side="top"
                style={{ maxWidth: 300, whiteSpace: "pre-line" }}
              >
                {hint}
              </TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </TooltipProvider>
  );
}
