import { dateFormat, numberFormat } from "@agent-hud/ui/format";
import { useGround } from "../ground";
import { agentColor, palette, tabular, type Logo } from "./theme";

/** A column's detail, raised over the chart while the pointer is on it
 * (Open's TokenBarsChart detail): its period, its total, and each model's
 * share in its colour; beside the column, toward the chart's wider side. */
export function ColumnDetail({
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
