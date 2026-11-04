import { useContext, type CSSProperties } from "react";
import { motion, useReducedMotionConfig } from "motion/react";
import type { Quota } from "@agent-hud/protocol";
import { HUD_METRICS } from "../../model";
import { QuotaRingCard } from "../QuotaRingCard";
import { SourceTokenChart } from "../TokenChartSection";
import { UsageRing } from "../../usage-meter/UsageRing";
import { noButtonChrome, tabular } from "../theme";
import { fill, pick, valueOf, type Scope } from "./custom-data";
import { Clickable } from "./Clickable";
import {
  DrawingContext,
  levelColor,
  nearest,
  textColor,
  type Node,
} from "./drawing";

const LOGO_SIZES: Record<string, number> = { small: 14, medium: 20, large: 28 };
const TEXT: Record<string, CSSProperties> = {
  title: { fontSize: 13, fontWeight: 600, lineHeight: "18px" },
  body: { fontSize: 13, lineHeight: "17px" },
  caption: { fontSize: 11, lineHeight: "15px" },
  figure: {
    fontSize: 24,
    fontWeight: 600,
    lineHeight: "28px",
    letterSpacing: -0.4,
    ...tabular,
  },
};

/** A control that draws something itself: a text, a logo, a bar, rings, the
 * token chart, a divider or a picker. */
export function LeafControl({
  node,
  scope,
  onClick,
}: {
  node: Node;
  scope: Scope;
  onClick?: () => void;
}) {
  const drawing = useContext(DrawingContext)!;
  // The panel's own setting: pictures, which draw no frames, turn motion off.
  const reduceMotion = useReducedMotionConfig() ?? false;
  switch (node.type) {
    case "text": {
      const style = TEXT[String(node.style ?? "body")] ?? TEXT.body;
      const words = fill(String(node.text ?? ""), scope);
      return (
        <Clickable onClick={onClick} title={onClick ? words : undefined}>
          <motion.span
            key={words}
            initial={reduceMotion ? false : { opacity: 0.4 }}
            animate={{ opacity: 1 }}
            transition={{ duration: HUD_METRICS.duration / 2 }}
            style={{
              display: "block",
              ...style,
              color: textColor(node.color, scope, drawing),
              textAlign:
                node.align === "center"
                  ? "center"
                  : node.align === "end"
                    ? "right"
                    : "left",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: node.style === "caption" ? "normal" : "nowrap",
            }}
          >
            {words}
          </motion.span>
        </Clickable>
      );
    }
    case "logo": {
      const agent = String(valueOf(node.agent, scope) ?? "");
      const size = LOGO_SIZES[String(node.size ?? "small")] ?? 14;
      const { Logo } = drawing;
      return (
        <Clickable
          onClick={onClick}
          style={{ width: size, height: size, flex: `0 0 ${size}px` }}
        >
          {agent && <Logo source={agent} size={size} light={drawing.light} />}
        </Clickable>
      );
    }
    case "bar": {
      const raw = valueOf(node.value, scope);
      const share = typeof raw === "number" ? Math.max(0, Math.min(1, raw)) : 0;
      const level =
        valueOf(node.level, scope) ?? nearest(scope, "level")?.level;
      return (
        <Clickable onClick={onClick} style={{ flex: 1, minWidth: 40 }}>
          <div
            style={{
              height: 4,
              borderRadius: 2,
              background: drawing.colors.track,
              overflow: "hidden",
            }}
          >
            <div
              style={{
                width: `${share * 100}%`,
                height: "100%",
                borderRadius: 2,
                background: levelColor(level, drawing.colors, false),
                transition: reduceMotion
                  ? undefined
                  : `width ${HUD_METRICS.duration}s cubic-bezier(${HUD_METRICS.ease.join(",")})`,
              }}
            />
          </div>
        </Clickable>
      );
    }
    case "rings":
    case "segments": {
      const items = node.items as Record<string, unknown> | undefined;
      const rows = pick(
        drawing.all,
        "windows",
        items?.where as Record<string, unknown> | undefined,
        items?.sort as string | undefined,
        Math.min(Number(items?.limit ?? 4), 4),
        scope,
      )
        .map((item) => item.quota)
        .filter((quota): quota is Quota => Boolean(quota));
      const size = (["small", "medium", "large"] as const).find(
        (s) => s === node.size,
      );
      if (node.legend === false) {
        const ring = { small: 40, medium: 52, large: 60 }[size ?? "medium"];
        return (
          <Clickable onClick={onClick} style={{ width: ring, height: ring }}>
            <UsageRing
              style={node.type}
              items={rows.map((row) => ({
                percent: row.usedPercent,
                color: levelColor(row.status, drawing.colors, false),
              }))}
              spec={{
                size: ring,
                width: ring / 10,
                hole: ring * 0.56,
                gap: 3,
              }}
              track={drawing.colors.track}
              reduceMotion={reduceMotion}
            />
          </Clickable>
        );
      }
      return rows.length ? (
        <Clickable onClick={onClick}>
          <QuotaRingCard
            rows={rows}
            style={node.type}
            ringSize={size}
            measure="quota"
            now={scope.now}
            light={drawing.light}
            showReset={drawing.showReset}
            onStats={onClick ? undefined : drawing.onStats}
          />
        </Clickable>
      ) : null;
    }
    case "chart": {
      const agent = valueOf(node.agent, scope);
      return (
        <Clickable onClick={onClick}>
          <SourceTokenChart
            snapshot={drawing.snapshot}
            light={drawing.light}
            Logo={drawing.Logo}
            layout={{
              period: (["day", "week", "month"] as const).find(
                (p) => p === node.period,
              ),
              agents: typeof agent === "string" && agent ? [agent] : undefined,
            }}
            active={drawing.active}
          />
        </Clickable>
      );
    }
    case "divider":
      return (
        <div
          role="separator"
          style={{ height: 1, background: drawing.colors.divider }}
        />
      );
    case "picker": {
      const state = String(node.state ?? "");
      const options = (node.options as { value: string; text: string }[]) ?? [];
      return (
        <div role="group" style={{ display: "flex", gap: 2 }}>
          {options.map((option) => {
            const chosen = scope.state[state] === option.value;
            return (
              <button
                key={option.value}
                aria-pressed={chosen}
                onClick={(event) => {
                  event.stopPropagation();
                  drawing.setState(state, option.value);
                }}
                style={{
                  ...noButtonChrome,
                  fontSize: 11,
                  lineHeight: "18px",
                  padding: "0 7px",
                  borderRadius: 9,
                  cursor: "pointer",
                  color: chosen
                    ? drawing.colors.text
                    : drawing.colors.secondary,
                  background: chosen ? drawing.colors.track : "transparent",
                  transition: `background ${HUD_METRICS.duration / 2}s, color ${HUD_METRICS.duration / 2}s`,
                }}
              >
                {fill(option.text, scope)}
              </button>
            );
          })}
        </div>
      );
    }
    default:
      return null;
  }
}
