import { vendorKey } from "@agent-hud/hud/model";
import type {
  Account,
  BillingAccount,
  PlanValueResponse,
  Quota,
} from "@agent-hud/protocol";

/** A signed-in plan the priced catalog does not cover, shown beside the
 * plans and the API balances. */
export interface SubscriptionTile {
  vendor: string;
  name: string;
  plan: string | null;
  windows: {
    name: string;
    shortLabel?: string;
    usedPercent: number | null;
    used: number | null;
    capacity: number | null;
  }[];
}

/** Quotas and accounts that the plan catalog and the balance list do not
 * already show. Empty until the plan answer has arrived, so they do not
 * flash beside the placeholders. */
export function subscriptionTiles(
  accounts: Account[],
  quotas: Quota[],
  names: Map<string, string>,
  plans: PlanValueResponse | null,
  balances: BillingAccount[],
  selected: string[] | null,
): SubscriptionTile[] {
  if (!plans) return [];
  const shown = new Set([
    ...plans.accounts.map((account) => vendorKey(account.provider)),
    ...balances.map((balance) => vendorKey(balance.vendor)),
  ]);
  const wanted = (vendor: string) =>
    (!selected || selected.some((id) => vendorKey(id) === vendorKey(vendor))) &&
    !shown.has(vendorKey(vendor));
  const byVendor = new Map<string, Quota[]>();
  for (const quota of quotas) {
    const key = vendorKey(quota.vendor);
    if (!wanted(key)) continue;
    byVendor.set(key, [...(byVendor.get(key) ?? []), quota]);
  }
  for (const account of accounts) {
    const key = vendorKey(account.provider);
    if (byVendor.has(key) || !wanted(key)) continue;
    if (!account.plan && !account.planLabel) continue;
    byVendor.set(key, []);
  }
  return [...byVendor].flatMap(([key, windows]) => {
    const account = accounts.find((item) => vendorKey(item.provider) === key);
    const plan = account?.plan ?? account?.planLabel ?? null;
    if (!windows.length && !plan) return [];
    return [
      {
        vendor: key,
        name:
          names.get(key) ??
          names.get(windows[0]?.vendor ?? "") ??
          account?.name ??
          key,
        plan,
        windows: windows.map((quota) => ({
          name: quota.name,
          shortLabel: quota.shortLabel,
          usedPercent: quota.usedPercent,
          used: quota.metrics?.consumed?.used ?? null,
          capacity: quota.metrics?.consumed?.capacity ?? null,
        })),
      },
    ];
  });
}

export function shareAmount(
  used: number,
  capacity: number,
  display: "consumed" | "remaining",
  language: string,
): string {
  const shown = display === "remaining" ? Math.max(0, capacity - used) : used;
  const fmt = (value: number) =>
    Math.round(value).toLocaleString(language === "zh" ? "zh-CN" : "en-US");
  return `${fmt(shown)}/${fmt(capacity)}`;
}
