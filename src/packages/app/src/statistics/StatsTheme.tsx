import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import type { IndexingText } from "@agent-hud/ui/text/indexing";
import type {
  StatisticsCost,
  StatisticsCoverage,
  StatisticsModel,
  StatisticsTokenKind,
  StatisticsTotal,
} from "@agent-hud/protocol";
import { unpricedHelp } from "../lib/sessions";
import {
  dateFormat,
  listCost,
  numberFormat,
  share,
  tokensShort,
} from "@agent-hud/ui/format";

export interface StatsTheme {
  light: boolean;
  zh?: boolean;
  text: string;
  secondary: string;
  tertiary: string;
  ground: string;
  card: string;
  border: string;
  divider: string;
  track: string;
  segment: string;
  selected: string;
}

export const kinds: StatisticsTokenKind[] = [
  "cacheWrite",
  "input",
  "reasoning",
  "output",
  "cacheRead",
];
export const freshKinds = kinds.filter((kind) => kind !== "cacheRead");
export const kindLabels: Record<StatisticsTokenKind, string> = {
  input: "Input",
  output: "Output",
  reasoning: "Reasoning",
  cacheWrite: "Cache write",
  cacheRead: "Cache read",
};
export const zhKindLabels: Record<StatisticsTokenKind, string> = {
  input: "输入",
  output: "输出",
  reasoning: "思考",
  cacheWrite: "缓存写入",
  cacheRead: "缓存读取",
};
export function kindLabel(kind: StatisticsTokenKind, zh = false): string {
  return zh ? zhKindLabels[kind] : kindLabels[kind];
}

const palette = [
  "#c084fc",
  "#60a5fa",
  "#2dd4bf",
  "#fb923c",
  "#f472b6",
  "#a3e635",
];

/** TokenFormat.short: "48k", "1.3k", "2.4M". */
export const compact = tokensShort;

export const integer = (value: number) => numberFormat("en").format(value);

export const row: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 6,
  minWidth: 0,
};

export const truncate: CSSProperties = {
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
  minWidth: 0,
};

export const number: CSSProperties = {
  fontVariantNumeric: "tabular-nums",
  whiteSpace: "nowrap",
};

export const plain: CSSProperties = {
  background: "transparent",
  color: "inherit",
  border: 0,
  padding: 0,
  font: "inherit",
  textAlign: "left",
  cursor: "pointer",
};

export const card = (theme: StatsTheme): CSSProperties => ({
  borderRadius: 10,
  border: `1px solid ${theme.border}`,
  background: `var(--statistics-card, ${theme.card})`,
  boxShadow: "none",
  color: "inherit",
  padding: "12px 14px",
  gap: 10,
  minWidth: 0,
});

/**
 * A box raised off the page, such as a hover card (Open's `raisedBox`): the
 * window's ground, opaque, with the card border, a shadow and the page's ink.
 * It names its own colours because it is drawn outside the page, where it
 * would otherwise inherit the document's.
 */
export const raisedCard = (theme: StatsTheme): CSSProperties => ({
  display: "grid",
  gap: 5,
  padding: 10,
  borderRadius: 8,
  border: `1px solid ${theme.border}`,
  background: theme.ground,
  color: theme.text,
  boxShadow: "0 2px 12px rgba(0,0,0,.2)",
  zIndex: 50,
  pointerEvents: "none",
});

export function themeFor(light: boolean, zh = false): StatsTheme {
  return light
    ? {
        light,
        zh,
        ground: "#f5f5f7",
        text: "#1d1d1f",
        secondary: "#6e6e73",
        tertiary: "#8e8e93",
        card: "#fff",
        border: "rgba(0,0,0,.06)",
        divider: "rgba(0,0,0,.08)",
        track: "rgba(0,0,0,.10)",
        segment: "rgba(0,0,0,.08)",
        selected: "#fff",
      }
    : {
        light,
        zh,
        ground: "#282828",
        text: "#f5f5f7",
        secondary: "#98989d",
        tertiary: "#6e6e73",
        card: "rgba(255,255,255,.05)",
        border: "rgba(255,255,255,.12)",
        divider: "rgba(255,255,255,.10)",
        track: "rgba(255,255,255,.12)",
        segment: "rgba(255,255,255,.08)",
        selected: "rgba(255,255,255,.24)",
      };
}

export function dimensionsLabel(selected: StatisticsTokenKind[], zh = false) {
  if (selected.length === kinds.length) return zh ? "全部 Token" : "All tokens";
  if (selected.length === freshKinds.length && !selected.includes("cacheRead"))
    return zh ? "不含缓存读取" : "Excl. cache reads";
  return kinds
    .filter((kind) => selected.includes(kind))
    .map((kind) => kindLabel(kind, zh))
    .join(" + ");
}

export function modelColor(id: string) {
  let hash = 0;
  for (const char of id) hash = ((hash << 5) - hash + char.charCodeAt(0)) | 0;
  return palette[(hash >>> 0) % palette.length];
}

export function dateLabel(at: string | number, timeZone: string, zh = false) {
  return dateFormat(zh ? "zh-CN" : "en-GB", {
    timeZone,
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(at));
}

/** What the selection would cost at list prices, as Open writes it: "≈$12.30 · ≈¥45.60"; "+" when unpriced tokens are left out, which the help names. */
export function costText(cost: StatisticsCost | null, zh = false) {
  const text = cost && listCost(cost.amounts, zh ? "zh" : "en");
  if (!text) return "—";
  return cost.unpricedTokens > 0 ? `${text}+` : text;
}

export function costHelp(cost: StatisticsCost | null, zh = false) {
  if (!cost)
    return zh
      ? "当前筛选暂无目录价估算。"
      : "A list-price estimate is unavailable for this selection.";
  const unpriced = unpricedHelp(
    cost.unpricedTokens,
    zh,
    cost.unpricedNames ?? cost.unpricedModels,
  );
  return zh
    ? `按各厂商 API 目录价估算所选 Token 类型，不同币种不做换算。${unpriced ? `${unpriced}。` : ""}`
    : `Selected token kinds at the vendors' API list prices; currencies are not converted.${unpriced ? ` ${unpriced}.` : ""}`;
}

export function Coverage({
  coverage,
  theme,
}: {
  coverage: StatisticsCoverage;
  theme: StatsTheme;
}) {
  if (coverage.state === "complete" && !coverage.notice) return null;
  const zh = Boolean(theme.zh);
  const detail =
    coverage.notice ??
    (coverage.state === "unavailable"
      ? zh
        ? "当前筛选暂无账本数据。"
        : "No ledger data is available for this selection."
      : zh
        ? `已记录用量尚不完整${coverage.pendingSources ? ` · ${integer(coverage.pendingSources)} 个文件待解析` : ""}。`
        : `Recorded usage is partial${coverage.pendingSources ? ` · ${integer(coverage.pendingSources)} files pending` : ""}.`);
  return (
    <div
      role="status"
      style={{ color: theme.secondary, fontSize: 11, lineHeight: 1.45 }}
    >
      {detail}
      {coverage.unavailableProviders.length
        ? zh
          ? ` 暂不可用：${coverage.unavailableProviders.join("、")}。`
          : ` Unavailable: ${coverage.unavailableProviders.join(", ")}.`
        : ""}
    </div>
  );
}

/** While the usage ledger is read through, in place of the coverage note:
 * what runs, a bar of how far, and that totals are incomplete meanwhile. */
export function IndexingProgress({
  text,
  theme,
}: {
  text: IndexingText;
  theme: StatsTheme;
}) {
  return (
    <div
      role="status"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 6,
        padding: "0 16px",
      }}
    >
      <span style={{ fontSize: 13, fontWeight: 600 }}>{text.title}</span>
      <span
        role="progressbar"
        aria-label={text.title}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.floor(text.fraction * 100)}
        style={{
          position: "relative",
          height: 5,
          background: theme.track,
          borderRadius: 3,
          overflow: "hidden",
        }}
      >
        <i
          style={{
            position: "absolute",
            inset: "0 auto 0 0",
            display: "block",
            width: `${text.fraction * 100}%`,
            borderRadius: 3,
            background: theme.light ? "#007aff" : "#0a84ff",
            transition: "width 600ms ease-out",
          }}
        />
      </span>
      <span style={{ color: theme.secondary, fontSize: 11 }}>
        {text.detail}
      </span>
    </div>
  );
}

export function Heading({
  children,
  trailing,
  theme,
}: {
  children: ReactNode;
  trailing?: ReactNode;
  theme: StatsTheme;
}) {
  return (
    <div style={{ ...row, justifyContent: "space-between" }}>
      <strong style={{ fontSize: 13, fontWeight: 600 }}>{children}</strong>
      {trailing && (
        <span style={{ color: theme.secondary, fontSize: 11 }}>{trailing}</span>
      )}
    </div>
  );
}

export function Swatch({ color }: { color: string }) {
  return (
    <span
      aria-hidden
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: 2,
        background: color,
        flexShrink: 0,
      }}
    />
  );
}

export function ModelLine({
  model,
  total,
  theme,
}: {
  model: StatisticsModel;
  total: number;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  return (
    <div
      style={{ ...row, fontSize: 11 }}
      title={
        zh
          ? `${model.provider} · ${model.name}\n${integer(model.selectedTokens)} 已选 Token\n按 API 价格估算 ${costText(model.cost, zh)}`
          : `${model.provider} · ${model.name}\n${integer(model.selectedTokens)} selected tokens\n${costText(model.cost, zh)} at API prices`
      }
    >
      <Swatch color={modelColor(model.id)} />
      <span style={truncate}>{model.name}</span>
      <span style={{ marginLeft: "auto", ...number }}>
        {compact(model.selectedTokens)}
      </span>
      <span
        style={{
          width: 34,
          textAlign: "right",
          color: theme.secondary,
          ...number,
        }}
      >
        {total > 0 ? share(model.selectedTokens, total) : "—"}
      </span>
    </div>
  );
}

export function ModelBar({
  models,
  total,
  theme,
}: {
  models: StatisticsModel[];
  total: number;
  theme: StatsTheme;
}) {
  return (
    <div
      aria-hidden
      style={{
        display: "flex",
        height: 5,
        overflow: "hidden",
        background: theme.track,
        borderRadius: 3,
      }}
    >
      {models.map((model) => (
        <span
          key={model.id}
          style={{
            flex: `${total > 0 ? model.selectedTokens / total : 0} 1 0`,
            minWidth: 0,
            background: modelColor(model.id),
            transition:
              "flex-grow 240ms cubic-bezier(0.22, 1, 0.36, 1), background-color 200ms ease",
          }}
        />
      ))}
    </div>
  );
}

export function useAnimatedNumber(target: number, duration = 220): number {
  const [current, setCurrent] = useState(target);
  const currentRef = useRef(target);

  useEffect(() => {
    const from = currentRef.current;
    if (
      from === target ||
      matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      currentRef.current = target;
      setCurrent(target);
      return;
    }
    const start = performance.now();
    let frame = 0;
    const step = (now: number) => {
      const elapsed = now - start;
      const t = Math.min(1, Math.max(0, elapsed / duration));
      const eased = 1 - Math.pow(1 - t, 3);
      const next = from + (target - from) * eased;
      currentRef.current = next;
      setCurrent(t >= 1 ? target : Math.round(next));
      if (t < 1) {
        frame = requestAnimationFrame(step);
      }
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [target, duration]);

  return current;
}

export function ModelList({
  models,
  total,
  theme,
}: {
  models: StatisticsModel[];
  total: number;
  theme: StatsTheme;
}) {
  const [expanded, setExpanded] = useState(false);
  const zh = Boolean(theme.zh);
  return (
    <div style={{ display: "grid", gap: 5, minWidth: 0, flex: 1 }}>
      <ModelBar models={models} total={total} theme={theme} />
      {(expanded ? models : models.slice(0, 3)).map((model) => (
        <ModelLine key={model.id} model={model} total={total} theme={theme} />
      ))}
      {models.length > 3 && (
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          style={{ ...plain, color: theme.secondary, fontSize: 11 }}
        >
          {expanded
            ? zh
              ? "收起模型"
              : "Show fewer models"
            : zh
              ? `其余 ${models.length - 3} 个模型`
              : `${models.length - 3} more models`}
        </button>
      )}
    </div>
  );
}

export function useCanvasWidth() {
  const ref = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth((old) => {
        const next = Math.round(entry.contentRect.width);
        if (next <= 0 && old > 0) return old;
        return old === next ? old : next;
      }),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

export function TotalSummary({
  value,
  coverage,
  theme,
}: {
  value: StatisticsTotal;
  coverage: StatisticsCoverage;
  theme: StatsTheme;
}) {
  const animatedTokens = useAnimatedNumber(value.selectedTokens);
  const zh = Boolean(theme.zh);
  return (
    <div style={{ textAlign: "right" }}>
      <div
        title={
          zh
            ? "所选时间范围、智能体、模型和类型的已记录 Token 数"
            : "Recorded tokens for exactly the selected range, agents, models and kinds"
        }
        style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, ...number }}
      >
        {coverage.state === "unavailable" ? "—" : compact(animatedTokens)}
      </div>
      <div
        title={costHelp(value.cost, zh)}
        style={{ color: theme.secondary, fontSize: 11, marginTop: 2 }}
      >
        {zh
          ? `按 API 价格估算 ${costText(value.cost, zh)}`
          : `At API prices ${costText(value.cost, zh)}`}
      </div>
    </div>
  );
}
