import type { BillingAccount, PanelBlockLayout } from "@agent-hud/protocol";

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

/**
 * The measures a balance card offers: the ones its layout names, in that
 * order, or every one when it names none. The card shows a picker when it
 * offers more than one.
 */
export function balanceMeasuresOffered(
  measures: readonly string[] | undefined,
): readonly BalanceMeasure[] {
  const named = (measures ?? []).filter((value): value is BalanceMeasure =>
    (balanceMeasures as readonly string[]).includes(value),
  );
  return named.length ? named : balanceMeasures;
}
