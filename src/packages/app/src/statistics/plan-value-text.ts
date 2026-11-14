import { listPrice, translate, type Language } from "@agent-hud/ui/format";
import type { PlanValueAccount } from "@agent-hud/protocol";

/** What Core's advice says, in words: the evidence, then the tier and what it changes a month. */
export function adviceText(
  advice: NonNullable<PlanValueAccount["advice"]>,
  language: Language,
) {
  const values = {
    window: advice.window.shortLabel || advice.window.name,
    plan: advice.plan.name,
    price: listPrice(advice.plan.monthlyUSD, "USD", language),
    delta: listPrice(Math.abs(advice.monthlyDeltaUSD), "USD", language),
    cycles: advice.cycles,
    ranOut: advice.ranOut,
    busiest: Math.round(advice.busiestPercent),
  };
  if (advice.kind === "upgrade")
    return translate(
      language,
      "{{window}} reached 95% in {{ranOut}} of the last {{cycles}} cycles. {{plan}} ({{price}}/mo, {{delta}} more) would have kept the busiest under 80%.",
      values,
    );
  return translate(
    language,
    "{{window}}: over the last {{cycles}} cycles no window went past {{busiest}}%. {{plan}} ({{price}}/mo, {{delta}} less) would still have stayed under 80%.",
    values,
  );
}
