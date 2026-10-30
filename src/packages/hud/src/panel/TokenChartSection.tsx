import { useEffect, useRef, useState } from "react";
import {
  AnimatePresence,
  animate,
  motion,
  useMotionValue,
  useReducedMotionConfig,
  useTransform,
  type Transition,
} from "motion/react";
import type {
  PanelLayout,
  Snapshot,
  StatisticsResponse,
} from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import { formatTokens } from "../quota-metrics";
import { HUD_METRICS, sourcesForMark } from "../model";
import { usePanelQuery } from "../PanelContext";
import { agentColor, palette, tabular, type Logo, type Period } from "./theme";
import { useFormat } from "@agent-hud/ui/format-context";
import { dateFormat, numberFormat, systemTimeZone } from "@agent-hud/ui/format";
import { hexAlpha } from "../tokens";
import { useGround } from "../ground";
import { orderedModels, visibleModelCounts } from "./model-visibility";

const moving: Transition = {
  duration: HUD_METRICS.duration,
  ease: HUD_METRICS.ease,
};

export function SourceTokenChart({
  snapshot,
  focusVendor,
  light,
  Logo,
  layout = {},
  active = true,
}: {
  snapshot: Snapshot;
  focusVendor?: string;
  light: boolean;
  Logo?: Logo;
  layout?: PanelLayout["tokens"];
  active?: boolean;
}) {
  const zh = useFormat().language === "zh";
  const initial =
    layout.period === "week" ? "7d" : layout.period === "month" ? "30d" : "24h";
  const [period, setPeriod] = useState<Period>(initial);
  useEffect(() => setPeriod(initial), [initial]);
  const [hovered, setHovered] = useState<number | null>(null);
  const histogram = useRef<HTMLDivElement>(null);
  const colors = palette(useGround(light));
  const now = Date.parse(snapshot.generatedAt);
  const periods: { label: Period; duration: number }[] = [
    { label: "24h", duration: 86_400_000 },
    { label: "7d", duration: 7 * 86_400_000 },
    { label: "30d", duration: 30 * 86_400_000 },
  ];
  const duration = periods.find((p) => p.label === period)!.duration;
  const to = new Date(Math.ceil(now / 900000) * 900000).toISOString();
  const from = new Date(Date.parse(to) - duration).toISOString();
  const query = usePanelQuery<StatisticsResponse>(
    "statistics",
    {
      from,
      to,
      bucketMinutes: period === "24h" ? 30 : 1440,
      timeZone: systemTimeZone(),
      tokenKinds: ["input", "output", "cacheRead", "cacheWrite", "reasoning"],
      ...(focusVendor
        ? { providers: sourcesForMark(snapshot, focusVendor) }
        : layout.agents?.length
          ? { providers: layout.agents }
          : {}),
      ...(layout.models?.length ? { models: layout.models } : {}),
    },
    snapshot.statisticsRevision,
    active,
  );
  // Each model in its agent colour, by its place in Core's list of models,
  // as Open colours a consumer by its place in the report's consumers.
  const models = orderedModels(
    query.data?.available.models ?? [],
    layout.modelOrder,
    layout.hiddenModels,
  );
  const points: {
    timestamp: string;
    tokens: number;
    models?: Record<string, number>;
  }[] = query.data
    ? query.data.buckets.map((p) => {
        const visible = visibleModelCounts(
          p.models,
          p.selectedTokens,
          layout.hiddenModels,
        );
        return {
          timestamp: p.timestamp,
          tokens: visible.total,
          models: visible.counts,
        };
      })
    : snapshot.timeline.filter(
        (p) =>
          Date.parse(p.timestamp) >= now - duration &&
          Date.parse(p.timestamp) <= now,
      );
  // The shown answer's own buckets: while another range loads, the last one stays.
  const bucketMs =
    (query.data?.query.bucketMinutes ?? (period === "24h" ? 30 : 1440)) *
    60_000;
  const known =
    Boolean(query.data) ||
    (!focusVendor &&
      !layout.agents?.length &&
      !layout.models?.length &&
      !layout.hiddenModels?.length &&
      snapshot.timeline.length > 0);
  const total = layout.hiddenModels?.length
    ? points.reduce((sum, p) => sum + p.tokens, 0)
    : (query.data?.totals.selectedTokens ??
      points.reduce((sum, p) => sum + p.tokens, 0));
  const peak = Math.max(1, ...points.map((p) => p.tokens));
  const notice = query.error ?? query.data?.coverage.notice;
  // Still, everything is drawn at its value at once, with no frame to wait
  // for: motion applies even an instant change on the next frame, and a
  // picture's hidden window never draws one.
  const still = Boolean(useReducedMotionConfig());
  const bars = known && points.some((point) => point.tokens > 0);
  const heights = points.map(
    (point) => `${Math.max(0, (point.tokens / peak) * 95)}%`,
  );
  // Re-measure the columns' places only when the set of columns changes.
  const arrangement = `${bucketMs}:${points.map((point) => point.timestamp).join()}`;
  // Models with usage somewhere in the range, each keeping its colour slot.
  const stacked = models
    .map((model, slot) => ({ model, slot }))
    .filter(({ model }) =>
      points.some((point) => (point.models?.[model.id] ?? 0) > 0),
    );
  return (
    <section
      className="hud-source-token-chart"
      style={{
        display: "grid",
        gap: 6,
        paddingTop: 8,
        boxShadow: `inset 0 1px 0 ${colors.divider}`,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          minHeight: 16,
          gap: 8,
        }}
      >
        <strong style={{ fontSize: 13, fontWeight: 600, color: colors.text }}>
          {zh ? "Token 消耗" : "Tokens"}
        </strong>
        <span style={{ flex: 1 }} />
        <div style={{ display: "flex", gap: 2 }}>
          {periods.map((p) => {
            const available = query.available || p.label === "24h";
            return (
              <Button
                key={p.label}
                size="xs"
                variant="ghost"
                aria-pressed={period === p.label}
                disabled={!available}
                title={
                  available
                    ? zh
                      ? `最近 ${p.label}`
                      : `Last ${p.label}`
                    : zh
                      ? `这份快照没有 ${p.label} 的历史`
                      : `${p.label} history is not provided by this snapshot`
                }
                onClick={() => setPeriod(p.label)}
                style={{
                  height: 18,
                  minHeight: 18,
                  minWidth: 24,
                  padding: "3px 2px",
                  fontSize: 9,
                  fontWeight: 600,
                  borderRadius: 5,
                  lineHeight: "12px",
                  background:
                    period === p.label ? colors.selected : "transparent",
                  color: period === p.label ? colors.text : colors.tertiary,
                }}
              >
                {p.label}
              </Button>
            );
          })}
        </div>
        <span
          style={{
            ...tabular,
            fontSize: 12,
            color: colors.secondary,
            whiteSpace: "nowrap",
          }}
          title={
            notice ??
            (zh
              ? "本机账本记录的用量"
              : "Recorded usage from the shared ledger")
          }
        >
          {period} · {known ? <TokenCount tokens={total} still={still} /> : "—"}{" "}
          tok
        </span>
      </div>
      <div
        ref={histogram}
        className="hud-source-token-histogram"
        style={{
          position: "relative",
          height: 40,
          margin: "8px 6px 0",
          boxShadow: `inset 0 -1px 0 ${colors.divider}`,
          display: "flex",
          alignItems: "flex-end",
          gap: 2,
        }}
        onPointerLeave={() => setHovered(null)}
      >
        {/* Each column keeps its bucket's key: a refresh grows or shrinks the
            bar in place, a moving window slides the columns along, and a new
            range's columns grow in as the old ones fade. */}
        <AnimatePresence initial={false} mode="popLayout">
          {bars
            ? points.map((point, index) => (
                <motion.div
                  key={`${bucketMs}:${point.timestamp}`}
                  layout="position"
                  layoutDependency={arrangement}
                  exit={still ? undefined : { opacity: 0 }}
                  transition={moving}
                  onPointerEnter={() => setHovered(index)}
                  style={{
                    position: "relative",
                    alignSelf: "stretch",
                    display: "flex",
                    alignItems: "flex-end",
                    flex: "1 1 0",
                    minWidth: 0,
                    background:
                      hovered === index
                        ? hexAlpha(colors.text, 0.08)
                        : "transparent",
                  }}
                >
                  <motion.div
                    initial={still ? false : { height: "0%" }}
                    animate={still ? undefined : { height: heights[index] }}
                    exit={still ? undefined : { height: "0%" }}
                    transition={moving}
                    style={{
                      height: still ? heights[index] : undefined,
                      width: "100%",
                      display: "flex",
                      flexDirection: "column-reverse",
                      borderRadius: "2px 2px 0 0",
                      overflow: "hidden",
                      opacity: hovered == null || hovered === index ? 1 : 0.55,
                      background: point.models
                        ? "transparent"
                        : colors.secondary,
                    }}
                  >
                    {point.models &&
                      stacked.map(({ model, slot }) => (
                        <i
                          key={model.id}
                          style={{
                            display: "block",
                            flex: `${point.models![model.id] ?? 0} 0 0`,
                            background: agentColor(slot, light),
                            transition: still
                              ? undefined
                              : `flex-grow ${HUD_METRICS.duration}s cubic-bezier(${HUD_METRICS.ease.join(",")})`,
                          }}
                        />
                      ))}
                  </motion.div>
                </motion.div>
              ))
            : (known || query.error) && (
                // Said only once the range is known to be empty, never while it loads.
                <motion.span
                  key="empty"
                  initial={still ? false : { opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={still ? undefined : { opacity: 0 }}
                  transition={moving}
                  style={{
                    alignSelf: "center",
                    textAlign: "center",
                    flex: 1,
                    fontSize: 11,
                    color: colors.secondary,
                  }}
                >
                  {query.error
                    ? query.error
                    : layout.hiddenModels?.length &&
                        (query.data?.totals.selectedTokens ?? 0) > 0 &&
                        total === 0
                      ? zh
                        ? "这段时间的模型消耗已隐藏"
                        : "Model usage in this range is hidden"
                      : zh
                        ? "这段时间没有 token 消耗"
                        : "No token usage in this range"}
                </motion.span>
              )}
        </AnimatePresence>
        {hovered != null && points[hovered] && (
          <ColumnDetail
            point={points[hovered]}
            index={hovered}
            count={points.length}
            width={histogram.current?.clientWidth ?? 0}
            bucketMs={bucketMs}
            models={models}
            light={light}
            zh={zh}
            Logo={Logo}
          />
        )}
      </div>
    </section>
  );
}

/** The range's total, counting to each new value as the bars move. */
function TokenCount({ tokens, still }: { tokens: number; still: boolean }) {
  const count = useMotionValue(tokens);
  const text = useTransform(count, (value) => formatTokens(Math.round(value)));
  useEffect(() => {
    if (still) {
      count.jump(tokens);
      return;
    }
    const controls = animate(count, tokens, moving);
    return () => controls.stop();
  }, [count, tokens, still]);
  return still ? formatTokens(tokens) : <motion.span>{text}</motion.span>;
}

/** A column's detail, raised over the chart while the pointer is on it
 * (Open's TokenBarsChart detail): its period, its total, and each model's
 * share in its colour; beside the column, toward the chart's wider side. */
function ColumnDetail({
  point,
  index,
  count,
  width,
  bucketMs,
  models,
  light,
  zh,
  Logo,
}: {
  point: { timestamp: string; tokens: number; models?: Record<string, number> };
  index: number;
  count: number;
  width: number;
  bucketMs: number;
  models: { id: string; name: string; provider: string }[];
  light: boolean;
  zh: boolean;
  Logo?: Logo;
}) {
  const colors = palette(useGround(light));
  const cardWidth = Math.min(280, width);
  const midpoint = ((index + 0.5) / Math.max(1, count)) * width;
  const preferred =
    midpoint < width / 2 ? midpoint + 12 : midpoint - cardWidth - 12;
  const start = Date.parse(point.timestamp);
  const locale = zh ? "zh-CN" : "en-US";
  const time = dateFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  });
  const period =
    bucketMs < 86_400_000
      ? `${time.format(start)}–${time.format(start + bucketMs)}`
      : dateFormat(locale, {
          month: "short",
          day: "numeric",
          weekday: "short",
        }).format(start);
  const shares = models
    .map((model, slot) => ({
      model,
      slot,
      tokens: point.models?.[model.id] ?? 0,
    }))
    .filter((share) => share.tokens > 0);
  return (
    <div
      role="tooltip"
      style={{
        position: "absolute",
        bottom: 6,
        left: Math.max(0, Math.min(preferred, width - cardWidth)),
        width: cardWidth,
        zIndex: 5,
        pointerEvents: "none",
        display: "grid",
        gap: 6,
        padding: 10,
        borderRadius: 8,
        fontSize: 11,
        color: colors.text,
        background: light ? "#ffffff" : "#2c2c2e",
        border: `1px solid ${colors.divider}`,
        boxShadow: "0 6px 18px rgba(0,0,0,0.28)",
      }}
    >
      <span style={{ color: colors.secondary }}>{period}</span>
      <strong style={{ ...tabular, fontSize: 14, fontWeight: 600 }}>
        {numberFormat(locale).format(point.tokens)} tokens
      </strong>
      {shares.map(({ model, slot, tokens }) => (
        <div
          key={model.id}
          style={{ display: "flex", alignItems: "center", gap: 5 }}
        >
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: "50%",
              flex: "0 0 6px",
              background: agentColor(slot, light),
            }}
          />
          {Logo && <Logo source={model.provider} size={12} light={light} />}
          <span
            style={{
              minWidth: 0,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {model.name}
          </span>
          <span style={{ flex: 1, minWidth: 6 }} />
          <span style={tabular}>{numberFormat(locale).format(tokens)}</span>
        </div>
      ))}
      {point.tokens === 0 && (
        <span style={{ color: colors.secondary }}>
          {zh ? "此时段没有 Token 消耗" : "No token usage in this period"}
        </span>
      )}
    </div>
  );
}
