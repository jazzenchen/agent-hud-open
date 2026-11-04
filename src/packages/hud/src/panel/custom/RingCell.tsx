/* A ring an agent places with no legend: its windows' rings alone, the most
 * used window's share inside if asked, as wide as its place when it fills it.
 * Pointing at it shows what the legend would: each window's name, share and
 * reset. */
import { useContext, useLayoutEffect, useRef, useState } from "react";
import { useReducedMotionConfig } from "motion/react";
import type { Quota } from "@agent-hud/protocol";
import { quotaPercent, quotaPercentLabel } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@agent-hud/ui/components/tooltip";
import { UsageRing } from "../../usage-meter/UsageRing";
import type { RingStyle } from "../../usage-meter/arcs";
import { tabular } from "../theme";
import { mostUsed } from "../QuotaRingCard";
import { sameAgent } from "./custom-collections";
import { formatted, type Scope } from "./custom-data";
import { Clickable } from "./Clickable";
import { DrawingContext, levelColor } from "./drawing";

const RING_PX = { small: 40, medium: 52, large: 60 } as const;
/** The largest ring that fills its place. */
const FILL_LIMIT = 200;

export type RingSize = keyof typeof RING_PX | "fill";

export function RingCell({
  rows,
  style,
  size,
  center,
  onClick,
  scope,
  centered,
}: {
  rows: Quota[];
  style: RingStyle;
  size: RingSize;
  center: string;
  onClick?: () => void;
  scope: Scope;
  /** It stands in a grid's cell, which it sits in the middle of. */
  centered: boolean;
}) {
  const drawing = useContext(DrawingContext)!;
  const format = useFormat();
  const reduceMotion = useReducedMotionConfig() ?? false;
  const box = useRef<HTMLDivElement>(null);
  const [room, setRoom] = useState<number>();
  useLayoutEffect(() => {
    const element = box.current;
    if (size !== "fill" || !element) return;
    const measure = () => setRoom(element.getBoundingClientRect().width);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [size]);
  const ring =
    size === "fill"
      ? Math.max(32, Math.min(FILL_LIMIT, Math.floor(room ?? 60)))
      : RING_PX[size];
  const most = mostUsed(rows);
  const agent = rows[0]?.vendor;
  const name =
    drawing.snapshot.providers.find((p) => sameAgent(p.id, agent))?.name ??
    agent ??
    "";
  const details = [
    name,
    ...rows.map((row) =>
      [
        row.shortLabel ?? row.name,
        quotaPercentLabel(row.usedPercent, format.quotaDisplay),
        row.resetAt
          ? formatted(row.resetAt, "window.resetsAt", "countdown", scope)
          : null,
      ]
        .filter(Boolean)
        .join("  "),
    ),
  ].join("\n");
  const cell = (
    <div
      ref={box}
      style={{
        display: "flex",
        justifyContent: centered || size === "fill" ? "center" : undefined,
        width: size === "fill" ? "100%" : undefined,
        minWidth: 0,
      }}
    >
      <Clickable
        onClick={onClick}
        style={{
          position: "relative",
          width: ring,
          height: ring,
          flex: `0 0 ${ring}px`,
        }}
      >
        <UsageRing
          style={style}
          items={rows.map((row) => ({
            percent: quotaPercent(row.usedPercent, format.quotaDisplay),
            color: levelColor(row.status, drawing.colors, false),
          }))}
          spec={{ size: ring, width: ring / 10, hole: ring * 0.56, gap: 3 }}
          track={drawing.colors.track}
          reduceMotion={reduceMotion}
          styles={{ position: "absolute", inset: 0 }}
        />
        {center === "percent" && (
          <span
            style={{
              position: "absolute",
              inset: 0,
              display: "grid",
              placeItems: "center",
              fontSize: ring >= 60 ? Math.round(ring * 0.22) : 11,
              fontWeight: 600,
              ...tabular,
              color: most
                ? levelColor(most.status, drawing.colors, true)
                : drawing.colors.secondary,
            }}
          >
            {most
              ? quotaPercentLabel(most.usedPercent, format.quotaDisplay)
              : "--"}
          </span>
        )}
      </Clickable>
    </div>
  );
  if (!rows.length) return cell;
  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>{cell}</TooltipTrigger>
        <TooltipContent side="top" style={{ whiteSpace: "pre-line" }}>
          {details}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
