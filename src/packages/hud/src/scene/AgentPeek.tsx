import { labelWidth } from "../geometry";
import { useFormat } from "@agent-hud/ui/format-context";
import { quotaPercentLabel, type QuotaDisplay } from "@agent-hud/ui/format";
import { useMarkLevelText } from "../ground";
import {
  isPointsShareQuota,
  quotaShortName,
  statusColor,
  type HUDEdge,
  type HUDVendor,
} from "../model";

export interface PeekItem {
  id: string;
  label: string;
  value: string;
  color?: string;
}

/** What the Simple dock's one-line peek says: each window's share used, once
 * per short name, then the balance of an API-billed agent. */
export function peekItems(
  vendor: HUDVendor,
  zh: boolean,
  color: (level: HUDVendor["level"]) => string = statusColor,
  display?: QuotaDisplay,
): PeekItem[] {
  const seen = new Set<string>();
  const items: PeekItem[] = vendor.quotas
    .filter((q) => {
      const label = quotaShortName(q);
      if (seen.has(label)) return false;
      seen.add(label);
      return true;
    })
    .map((q) => ({
      id: q.id,
      label: quotaShortName(q),
      value:
        q.usedPercent == null
          ? "--"
          : isPointsShareQuota(q)
            ? quotaPercentLabel(q.usedPercent)
            : quotaPercentLabel(q.usedPercent, display),
      color: q.usedPercent == null ? undefined : color(q.status),
    }));
  if (vendor.balance)
    items.push({
      id: "balance",
      label: zh ? "余额" : "Balance",
      value: vendor.balance,
      color: color(vendor.level),
    });
  if (!items.length)
    items.push({ id: "quota", label: zh ? "额度" : "Quota", value: "--" });
  return items;
}

/** The line is as long as what it says; its text is measured, since window names differ in length. */
export function peekLineWidth(
  vendor: HUDVendor,
  scale: number,
  zh: boolean,
  tuck: number,
  display?: QuotaDisplay,
): number {
  const font = Math.round(9 * scale),
    gap = Math.round(5 * scale),
    spacing = Math.round(12 * scale),
    items = peekItems(vendor, zh, statusColor, display);
  const cells = items.length
    ? items.reduce(
        (sum, item) =>
          sum +
          labelWidth(item.label, font) +
          gap +
          labelWidth(item.value, font) +
          2,
        0,
      ) +
      spacing * (items.length - 1)
    : labelWidth(vendor.name, font);
  return Math.max(
    100,
    Math.min(640 + tuck, Math.ceil(cells + spacing * 2 + 4 + tuck)),
  );
}

export function AgentPeek({
  vendor,
  simple,
  scale,
  light = false,
  zh,
  tuck = 0,
  edge = "top",
}: {
  vendor: HUDVendor;
  simple: boolean;
  scale: number;
  light?: boolean;
  zh: boolean;
  tuck?: number;
  edge?: HUDEdge;
}) {
  const color = useMarkLevelText(light);
  const { quotaDisplay } = useFormat();
  if (simple) {
    const items = peekItems(vendor, zh, color, quotaDisplay);
    return (
      <div
        className="hud-agent-peek-line"
        style={{
          display: "flex",
          alignItems: "center",
          gap: Math.round(12 * scale),
          padding: `0 ${Math.round(12 * scale)}px`,
          ...(edge === "right"
            ? { paddingRight: Math.round(12 * scale) + tuck }
            : edge === "left"
              ? { paddingLeft: Math.round(12 * scale) + tuck }
              : {}),
          height: Math.round(26 * scale),
          fontSize: Math.round(9 * scale),
          whiteSpace: "nowrap",
        }}
      >
        {items.length ? (
          items.map((item) => (
            <span
              key={item.id}
              style={{ display: "inline-flex", gap: Math.round(5 * scale) }}
            >
              <span style={{ opacity: 0.55 }}>{item.label}</span>
              <strong style={{ fontWeight: 600, color: item.color }}>
                {item.value}
              </strong>
            </span>
          ))
        ) : (
          <span style={{ opacity: 0.7 }}>{vendor.name}</span>
        )}
      </div>
    );
  }
  return (
    <div
      className="hud-agent-peek-bubble"
      style={{
        fontSize: 12 * scale,
        padding: `${10 * scale}px ${12 * scale}px`,
        width: 196 * scale,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 7,
          height: 18 * scale,
          fontWeight: 600,
          fontSize: 13 * scale,
        }}
      >
        {vendor.name}
      </div>
      {vendor.quotas.map((q) => (
        <div
          key={q.id}
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 8,
            height: 18 * scale,
            alignItems: "center",
          }}
        >
          <span style={{ opacity: 0.55 }}>{quotaShortName(q)}</span>
          <strong
            style={{
              color: q.usedPercent == null ? undefined : color(q.status),
              fontSize: 12 * scale,
            }}
          >
            {q.usedPercent == null
              ? "--"
              : isPointsShareQuota(q)
                ? quotaPercentLabel(q.usedPercent)
                : quotaPercentLabel(q.usedPercent, quotaDisplay)}
          </strong>
        </div>
      ))}
      {!vendor.quotas.length && !vendor.balance && (
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            height: 18 * scale,
          }}
        >
          <span style={{ opacity: 0.55 }}>{zh ? "额度" : "Quota"}</span>
          <strong>--</strong>
        </div>
      )}
      {vendor.balance && (
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            height: 18 * scale,
          }}
        >
          <span style={{ opacity: 0.55 }}>{zh ? "余额" : "Balance"}</span>
          <strong
            style={{
              color: color(vendor.level),
              fontWeight: 600,
              fontSize: 12 * scale,
            }}
          >
            {vendor.balance}
          </strong>
        </div>
      )}
    </div>
  );
}
