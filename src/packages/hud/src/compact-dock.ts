/** Dock-only reduction. The expanded panel still receives the full snapshot. */
type DockAgent = {
  used: number | null;
  heldUsed: number | null;
  balance: string | null;
  usage: string | null;
  working: number;
  waiting: number;
  level: string;
};

export function compactVendors<T extends DockAgent>(vendors: T[]): T[] {
  const visible = vendors.filter(
    (vendor) =>
      vendor.working > 0 ||
      vendor.waiting > 0 ||
      vendor.used !== null ||
      vendor.heldUsed !== null ||
      Boolean(vendor.balance || vendor.usage) ||
      vendor.level === "warning" ||
      vendor.level === "critical",
  );
  // Keep an entry point when everything is idle or awaiting its first reading.
  return visible.length ? visible : vendors.slice(0, 1);
}

export function compactLabel(vendor: DockAgent, label: string): string {
  if (label !== "--") return label;
  return vendor.waiting > 0 ? "!" : vendor.working > 0 ? "…" : "·";
}
