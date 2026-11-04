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
import { HUD_METRICS, sourcesForMark } from "../model";
import { usePanelQuery } from "../PanelContext";
import { palette, tabular, type Logo, type Period } from "./theme";
import { useFormat } from "@agent-hud/ui/format-context";
import { systemTimeZone, tokensShort } from "@agent-hud/ui/format";
import { hexAlpha } from "../tokens";
import { useGround } from "../ground";
import { orderedModels, visibleModelCounts } from "./model-visibility";
import { ColumnDetail } from "./ColumnDetail";
import { TokenBar } from "./TokenBar";

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
  bare = false,
}: {
  snapshot: Snapshot;
  focusVendor?: string;
  light: boolean;
  Logo?: Logo;
  layout?: PanelLayout["tokens"];
  active?: boolean;
  /** No line above it: it heads a column of its own. */
  bare?: boolean;
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
  // A column's key names its bucket; the snapshot's hourly points have keys of
  // their own, so none is taken for the day that starts at the same hour.
  const bucketKey = query.data ? String(bucketMs) : "snapshot";
  // Re-measure the columns' places only when the set of columns changes.
  const arrangement = `${bucketKey}:${points.map((point) => point.timestamp).join()}`;
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
        // As wide as its place, never wider: a long total shortens instead.
        gridTemplateColumns: "minmax(0, 1fr)",
        gap: 6,
        paddingTop: bare ? 0 : 8,
        boxShadow: bare ? undefined : `inset 0 1px 0 ${colors.divider}`,
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
            minWidth: 0,
            overflow: "hidden",
            textOverflow: "ellipsis",
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
                  key={`${bucketKey}:${point.timestamp}`}
                  // Still, a column stands where it is laid out: a move
                  // waits for frames a picture never draws.
                  layout={still ? false : "position"}
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
                  <TokenBar
                    height={heights[index]}
                    models={point.models}
                    stacked={stacked}
                    still={still}
                    faded={hovered != null && hovered !== index}
                    fill={colors.secondary}
                    light={light}
                    transition={moving}
                  />
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
  const text = useTransform(count, (value) => tokensShort(Math.round(value)));
  useEffect(() => {
    if (still) {
      count.jump(tokens);
      return;
    }
    const controls = animate(count, tokens, moving);
    return () => controls.stop();
  }, [count, tokens, still]);
  return still ? tokensShort(tokens) : <motion.span>{text}</motion.span>;
}
