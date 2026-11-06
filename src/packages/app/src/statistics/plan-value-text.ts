import { listPrice } from "@agent-hud/ui/format";
import type { PlanValueAccount } from "@agent-hud/protocol";

const usd = (amount: number, zh: boolean) =>
  listPrice(amount, "USD", zh ? "zh" : "en");

/** What Core's advice says, in words: the evidence, then the tier and what it changes a month. */
export function adviceText(
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
