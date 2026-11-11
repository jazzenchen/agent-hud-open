import type { BillingAccount, PanelBlockLayout } from "@agent-hud/protocol";
import { dateFormat, moneyAmount, type Language } from "@agent-hud/ui/format";

/** A balance ID includes the account and currency, unlike accountId. */
export function visibleBalances(
  rows: readonly BillingAccount[],
  layout: PanelBlockLayout = {},
): BillingAccount[] {
  const order = layout.rows ?? [];
  const position = (id: string) => {
    const index = order.indexOf(id);
    return index < 0 ? Infinity : index;
  };
  return rows
    .filter(
      (row) =>
        !(layout.hiddenRows ?? []).includes(row.id) &&
        !(layout.hiddenAccounts ?? []).includes(row.accountId ?? row.id) &&
        (!layout.accounts?.length ||
          layout.accounts.includes(row.accountId ?? row.id)),
    )
    .sort((a, b) => position(a.id) - position(b.id));
}

/** What a balance card can show, in picker order. */
export const balanceMeasures = ["quota", "burn", "cost"] as const;
export type BalanceMeasure = (typeof balanceMeasures)[number];

export function balanceColumns(
  account: BillingAccount,
  period: "day" | "week" | "month",
  selected: BalanceMeasure,
  language: Language,
): { name: string; value: string }[] {
  const isCredits = account.currency.toLowerCase() === "credits";
  const metrics = account.metrics?.periods[period];
  const cost = metrics?.cost;
  const burn = metrics?.burnRate;
  const money = (amount: number, currency: string) =>
    moneyAmount(amount, currency, language);
  if (!isCredits && selected === "burn") {
    return [
      {
        name: language === "zh" ? "每日消耗" : "Consumption / day",
        value: burn ? `${money(burn.perDay, burn.currency)}/d` : "—",
      },
      {
        name: language === "zh" ? "预计用完" : "Estimated depletion",
        value: metrics?.runsOutAt
          ? dateFormat(language === "zh" ? "zh-CN" : "en-US").format(
              new Date(metrics.runsOutAt),
            )
          : "—",
      },
    ];
  }
  if (!isCredits && selected === "cost") {
    return [
      {
        name: language === "zh" ? "估算花费" : "Estimated cost",
        value: cost ? money(cost.amount, cost.currency) : "—",
      },
      {
        name: language === "zh" ? "输出速度" : "Output speed",
        value: metrics?.replyRate
          ? `${metrics.replyRate.lowerBound ? "≥" : ""}${metrics.replyRate.tokensPerSecond.toFixed(1)} tok/s`
          : "—",
      },
    ];
  }
  if (account.planCredits != null && account.rewardCredits != null) {
    return [
      {
        name:
          account.label ||
          (language === "zh" ? "可用积分" : "Available credits"),
        value: money(account.amount, account.currency),
      },
      {
        name: language === "zh" ? "套餐积分" : "Plan credits",
        value: money(account.planCredits, account.currency),
      },
      {
        name: language === "zh" ? "奖励积分" : "Reward credits",
        value: money(account.rewardCredits, account.currency),
      },
    ];
  }
  if (isCredits) {
    return [
      {
        name:
          account.label ||
          (language === "zh" ? "可用积分" : "Available credits"),
        value: money(account.amount, account.currency),
      },
    ];
  }
  return [
    {
      name: account.label || (language === "zh" ? "余额" : "Balance"),
      value: money(account.amount, account.currency),
    },
    {
      name: language === "zh" ? "估算花费" : "Estimated cost",
      value: cost ? money(cost.amount, cost.currency) : "—",
    },
  ];
}
