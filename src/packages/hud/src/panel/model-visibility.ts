/** The saved order is only a preference: keep new and returning models visible. */
export function orderedModels<T extends { id: string }>(
  models: T[],
  order: string[] = [],
  hidden: string[] = [],
): T[] {
  const positions = new Map(order.map((id, index) => [id, index]));
  const invisible = new Set(hidden);
  return models
    .filter((model) => !invisible.has(model.id))
    .map((model, index) => ({ model, index }))
    .sort(
      (a, b) =>
        (positions.get(a.model.id) ?? Infinity) -
          (positions.get(b.model.id) ?? Infinity) || a.index - b.index,
    )
    .map(({ model }) => model);
}

/** Derive both bars and totals from the same visible model counts. */
export function visibleModelCounts(
  counts: Record<string, number>,
  total: number,
  hidden: string[] = [],
): { counts: Record<string, number>; total: number } {
  const invisible = new Set(hidden);
  const visible = Object.fromEntries(
    Object.entries(counts).filter(([id]) => !invisible.has(id)),
  );
  const removed = Object.entries(counts).reduce(
    (sum, [id, count]) => sum + (invisible.has(id) ? count : 0),
    0,
  );
  return {
    counts: visible,
    total: Math.max(0, total - removed),
  };
}
