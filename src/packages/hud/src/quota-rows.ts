import type { PanelBlockLayout, Quota } from "@agent-hud/protocol";

/** The key a layout writes for a quota row: the Swift app's window id where the
 * row has one, so layouts carried over from it keep naming the same row. */
export function quotaRowKey(row: Quota): string {
  return row.legacyWindowId ?? row.id;
}

/** Whether a layout key names this row: its id, its Swift window id, or that
 * window id without the account (one key for every account's row). */
export function quotaRowMatches(row: Quota, key: string): boolean {
  return (
    row.id === key ||
    row.legacyWindowId === key ||
    !!(
      row.accountId &&
      row.legacyWindowId?.startsWith(`${row.accountId}/`) &&
      row.legacyWindowId.slice(row.accountId.length + 1) === key
    )
  );
}

/** Whether a layout's hidden rows name this row by any of its keys. */
export function quotaRowHidden(
  row: Quota,
  hiddenRows: readonly string[],
): boolean {
  return hiddenRows.some((key) => quotaRowMatches(row, key));
}

/** Hidden rows after one row is shown again: every key that names it goes, and
 * the other rows such a key hid (a window named without its account) stay
 * hidden under their own keys. */
export function withRowShown(
  hiddenRows: readonly string[],
  row: Quota,
  rows: readonly Quota[],
): string[] {
  const dropped = hiddenRows.filter((key) => quotaRowMatches(row, key));
  const others = rows
    .filter(
      (other) =>
        other.id !== row.id &&
        dropped.some((key) => quotaRowMatches(other, key)),
    )
    .map(quotaRowKey);
  return [
    ...new Set([
      ...hiddenRows.filter((key) => !dropped.includes(key)),
      ...others,
    ]),
  ];
}

/** Rows in a layout's order: a row stands where the first key that names it
 * does; rows no key names follow in the order they came. */
export function inRowOrder<T extends Quota>(
  rows: readonly T[],
  order: readonly string[],
): T[] {
  const position = (row: T) => {
    const index = order.findIndex((key) => quotaRowMatches(row, key));
    return index < 0 ? Infinity : index;
  };
  return [...rows].sort((a, b) => position(a) - position(b));
}

/** Account labels are display values; provider identity owns saved visibility. */
export function quotaAccountKey(row: Quota): string {
  return row.accountId ?? row.account ?? "unassigned";
}

/** The rows a block shows, in its order: none its layout hides, none of a
 * hidden account, only the accounts it names when it names some. */
export function blockRows<T extends Quota>(
  rows: readonly T[],
  block: PanelBlockLayout | undefined,
): T[] {
  return inRowOrder(
    rows.filter(
      (row) =>
        !quotaRowHidden(row, block?.hiddenRows ?? []) &&
        !(block?.hiddenAccounts ?? []).includes(quotaAccountKey(row)) &&
        (!block?.accounts?.length ||
          block.accounts.includes(quotaAccountKey(row))),
    ),
    block?.rows ?? [],
  );
}
