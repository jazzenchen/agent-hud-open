import type { StatisticsCost } from "@agent-hud/protocol";
import {
  listCost,
  moneyShort,
  translate,
  type Language,
} from "@agent-hud/ui/format";
import { unpricedHelp } from "../lib/sessions";

/** What the selection would cost at list prices, as Open writes it: "≈$12.30 · ≈¥45.60"; "+" when unpriced tokens are left out, which the help names. */
export function costText(cost: StatisticsCost | null, language: Language) {
  const text = cost && listCost(cost.amounts, language);
  if (!text) return "—";
  return cost.unpricedTokens > 0 ? `${text}+` : text;
}

/** A cost in whole units from a hundred up, "$6,465 · ¥0.70"; "+" when
 * unpriced tokens are left out. */
export function costShort(cost: StatisticsCost | null, language: Language) {
  const amounts = Object.entries(cost?.amounts ?? {}).filter(
    ([, amount]) => amount > 0,
  );
  if (!cost || !amounts.length) return "—";
  amounts.sort(([a], [b]) =>
    a === "USD" ? -1 : b === "USD" ? 1 : a < b ? -1 : 1,
  );
  const text = amounts
    .map(([code, amount]) => moneyShort(amount, code, language))
    .join(" · ");
  return cost.unpricedTokens > 0 ? `${text}+` : text;
}

export function costHelp(cost: StatisticsCost | null, language: Language) {
  if (!cost)
    return translate(
      language,
      "A list-price estimate is unavailable for this selection.",
    );
  const unpriced = unpricedHelp(
    cost.unpricedTokens,
    language,
    cost.unpricedNames ?? cost.unpricedModels,
  );
  const help = translate(
    language,
    "Selected token kinds at the vendors' API list prices; currencies are not converted.",
  );
  return unpriced
    ? translate(language, "{{help}} {{unpriced}}.", { help, unpriced })
    : help;
}
