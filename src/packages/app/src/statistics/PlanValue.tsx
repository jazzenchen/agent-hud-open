import { useEffect } from "react";
import { TrendingDown, TrendingUp } from "lucide-react";
import { toast } from "sonner";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { listCost, listPrice, monthDayTime } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import type {
  NativeMethod,
  PlanValueAccount,
  PlanValueResponse,
} from "@agent-hud/protocol";
import { useCoreQuery } from "../lib/use-core-query";
import {
  card,
  number,
  row,
  truncate,
  useAnimatedNumber,
  type StatsTheme,
} from "./StatsTheme";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/** A tier's usage as a multiple of the base tier: one figure, or the
 * weekly and five-hour ones when the vendor states two (Claude Ultra). */
export function multipleText(
  multiple: number,
  short: number | null,
  zh: boolean,
): string {
  if (short != null && short !== multiple)
    return zh
      ? `每周 ${multiple} 倍 · 5 小时 ${short} 倍用量`
      : `${multiple}× weekly · ${short}× 5-hour usage`;
  return zh ? `基础档 ${multiple} 倍用量` : `${multiple}× base usage`;
}

const usd = (amount: number, zh: boolean) =>
  listPrice(amount, "USD", zh ? "zh" : "en");

/** The value as a multiple of the plan's price: "4.1×" / "4.1 倍", "≥" when Core says the value is a lower bound. */
function Ratio({
  account,
  theme,
}: {
  account: PlanValueAccount;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  // Counted in hundredths, so the number moves as the others in the window do.
  const hundredths = useAnimatedNumber(Math.round((account.ratio ?? 0) * 100));
  if (account.ratio == null)
    return <span style={{ color: theme.secondary }}>—</span>;
  const value = hundredths / 100;
  const figure = value < 10 ? value.toFixed(1) : String(Math.round(value));
  return (
    <span
      title={
        zh
          ? "近 30 天 API 价值除以每月标价"
          : "The 30-day API value divided by the monthly list price"
      }
      style={{ ...number, fontSize: 17, fontWeight: 600 }}
    >
      {account.partial ? "≥" : ""}
      {zh ? `${figure} 倍` : `${figure}×`}
    </span>
  );
}

/** What Core's advice says, in words: the evidence, then the tier and what it changes a month. */
function adviceText(
  advice: NonNullable<PlanValueAccount["advice"]>,
  zh: boolean,
) {
  const window = advice.window.shortLabel || advice.window.name;
  const price = usd(advice.plan.monthlyUSD, zh);
  const delta = usd(Math.abs(advice.monthlyDeltaUSD), zh);
  const busiest = Math.round(advice.busiestPercent);
  if (advice.kind === "upgrade")
    return zh
      ? `${window}：近 ${advice.cycles} 个周期有 ${advice.ranOut} 个用到 95% 以上。换 ${advice.plan.name}（每月 ${price}，多 ${delta}），最忙的周期也不超过 80%。`
      : `${window} reached 95% in ${advice.ranOut} of the last ${advice.cycles} cycles. ${advice.plan.name} (${price}/mo, ${delta} more) would have kept the busiest under 80%.`;
  return zh
    ? `${window}：近 ${advice.cycles} 个周期，各窗口最多用到 ${busiest}%。换 ${advice.plan.name}（每月 ${price}，省 ${delta}），也不超过 80%。`
    : `${window}: over the last ${advice.cycles} cycles no window went past ${busiest}%. ${advice.plan.name} (${price}/mo, ${delta} less) would still have stayed under 80%.`;
}

/** The Statistics window's plan value: each signed-in account's plan at its monthly list price beside what its calls on this Mac would have cost at API list prices. */
export function PlanValue({
  refresh,
  action,
  active,
  theme,
  light,
}: {
  /** Changes when the ledger or a reading does: the section asks again. */
  refresh: string;
  action: Action;
  active: boolean;
  theme: StatsTheme;
  light: boolean;
}) {
  // Only the latest request publishes; a failure keeps the last answer shown.
  const { data, error } = useCoreQuery<PlanValueResponse>(
    action,
    "plan_value",
    {},
    active,
    refresh,
  );
  // The section has no place for a failure, so it shows as a toast.
  useEffect(() => {
    if (error) toast.error(error);
  }, [error]);
  const { calendar } = useFormat();
  const zh = Boolean(theme.zh);
  if (!data?.accounts.length) return null;
  const from = Date.parse(data.from);
  return (
    <div
      className="statistics-plan-value"
      style={{ display: "grid", gap: 8, minWidth: 0 }}
    >
      <div
        style={{
          ...row,
          alignItems: "baseline",
          justifyContent: "space-between",
          flexWrap: "wrap",
        }}
      >
        <strong style={{ fontSize: 13, fontWeight: 600 }}>
          {zh ? "订阅价值" : "Plan value"}
        </strong>
        <span style={{ color: theme.secondary, fontSize: 11 }}>
          {zh
            ? `按官方月付标价，核对于 ${data.checked}`
            : `Monthly list prices, checked ${data.checked}`}
        </span>
      </div>
      {data.accounts.map((account) => {
        const value = account.value;
        const since = value ? Date.parse(value.from) : NaN;
        const later = Number.isFinite(since) && since > from + 86_400_000;
        const worth = value
          ? `${listCost(value.amounts, zh ? "zh" : "en") || "—"}${value.unpricedTokens > 0 ? "+" : ""}`
          : null;
        return (
          <div
            key={account.id}
            style={{ ...card(theme), display: "grid", gap: 6 }}
          >
            <div style={row}>
              <HUDLogo source={account.provider} size={14} light={light} />
              <strong style={{ ...truncate, fontSize: 13 }}>
                {account.plan?.name ?? account.planLabel ?? "—"}
              </strong>
              {account.plan?.multiple != null && account.plan.multiple > 1 && (
                <span style={{ color: theme.secondary, fontSize: 11 }}>
                  {multipleText(
                    account.plan.multiple,
                    account.plan.shortMultiple ?? null,
                    zh,
                  )}
                </span>
              )}
              <span
                style={{
                  ...truncate,
                  color: theme.secondary,
                  fontSize: 11,
                  marginLeft: 6,
                }}
              >
                {account.name}
              </span>
              <span style={{ marginLeft: "auto" }}>
                <Ratio account={account} theme={theme} />
              </span>
            </div>
            <div
              style={{
                ...row,
                flexWrap: "wrap",
                color: theme.secondary,
                fontSize: 11,
              }}
            >
              <span style={number}>
                {account.plan
                  ? zh
                    ? `每月 ${usd(account.plan.monthlyUSD, zh)}`
                    : `${usd(account.plan.monthlyUSD, zh)}/mo`
                  : zh
                    ? "没有官方月付标价"
                    : "No monthly list price"}
              </span>
              <span>·</span>
              <span style={number}>
                {worth
                  ? zh
                    ? `近 30 天 API 价值 ${worth}`
                    : `30-day API value ${worth}`
                  : account.unavailable}
              </span>
              {later && (
                <span>
                  {zh
                    ? `（自 ${monthDayTime(since, calendar, "zh")} 起有记录）`
                    : `(recorded since ${monthDayTime(since, calendar, "en")})`}
                </span>
              )}
              {value?.indexing && (
                <span>{zh ? "（仍在索引）" : "(still indexing)"}</span>
              )}
            </div>
            {account.advice && (
              <div
                style={{
                  ...row,
                  alignItems: "flex-start",
                  color: theme.text,
                  fontSize: 11,
                }}
              >
                {account.advice.kind === "upgrade" ? (
                  <TrendingUp size={13} style={{ flexShrink: 0 }} />
                ) : (
                  <TrendingDown size={13} style={{ flexShrink: 0 }} />
                )}
                <span>{adviceText(account.advice, zh)}</span>
              </div>
            )}
          </div>
        );
      })}
      <p
        style={{
          color: theme.secondary,
          fontSize: 11,
          margin: 0,
          lineHeight: 1.45,
        }}
      >
        {zh
          ? "价值按这台 Mac 记录的调用和 API 目录价估算，不含经中转站或编程套餐的调用，也不含其他设备和网页上的用量。"
          : "The value prices the calls recorded on this Mac at API list prices, leaving out calls through a relay or a coding plan and use on other devices or the web."}
      </p>
    </div>
  );
}
