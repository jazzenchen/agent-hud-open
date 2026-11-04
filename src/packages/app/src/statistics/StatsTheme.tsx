import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { StatisticsCost, StatisticsTokenKind } from "@agent-hud/protocol";
import { tokenKindOrder, unpricedHelp } from "../lib/sessions";
import {
  dateFormat,
  listCost,
  numberFormat,
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

export const kinds: StatisticsTokenKind[] = [...tokenKindOrder];
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
