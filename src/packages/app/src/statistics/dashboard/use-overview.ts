import { useMemo, useState } from "react";
import type {
  NativeMethod,
  PlanValueResponse,
  StatisticsQuery,
  StatisticsResponse,
  StatisticsTokenKind,
} from "@agent-hud/protocol";
import { useCoreQuery } from "../../lib/use-core-query";
import { storageKeys } from "../../lib/storage-keys";
import { heatmapSpan, rangeIds, type Range, type Span } from "./model";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/** What the Usage page reads from Core: the range for the agents and models
 * picked; the heatmap's year for every agent (the chips list the agents in
 * it, and the heatmap draws the picked ones' series); and the plans. Money
 * always prices every token kind, as the plan value does. Only the latest
 * query and revision publish; no answer updates the Snapshot. */
export function useOverview({
  action,
  revision,
  active,
  span,
  end,
  kinds,
  providers,
  models,
  timeZone,
  accounts,
}: {
  action: Action;
  revision: number;
  active: boolean;
  span: Span;
  end: number;
  kinds: StatisticsTokenKind[];
  providers: string[] | null;
  models: string[] | null;
  timeZone: string;
  /** Changes when a signed-in account or its plan does. */
  accounts: string;
}) {
  const checked = async (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => {
    const response = (await action(method, params)) as StatisticsResponse;
    if (response.dataRevision < revision)
      throw new Error(
        "Statistics are catching up with the current ledger. Try refreshing again.",
      );
    return response;
  };
  const query: StatisticsQuery = useMemo(
    () => ({
      from: new Date(span.from).toISOString(),
      to: new Date(span.to).toISOString(),
      bucketMinutes: span.bucketMinutes,
      tokenKinds: kinds,
      costKinds: "all",
      ...(providers === null ? {} : { providers }),
      ...(models === null ? {} : { models }),
      timeZone,
    }),
    [span, kinds, providers, models, timeZone],
  );
  const range = useCoreQuery<StatisticsResponse>(
    checked,
    "statistics",
    query,
    active,
    revision,
  );
  const yearQuery: StatisticsQuery = useMemo(() => {
    const { from, to } = heatmapSpan(end);
    return {
      from: new Date(from).toISOString(),
      to: new Date(to).toISOString(),
      bucketMinutes: 1440,
      tokenKinds: kinds,
      costKinds: "all",
      timeZone,
    };
  }, [end, kinds, timeZone]);
  const year = useCoreQuery<StatisticsResponse>(
    checked,
    "statistics",
    yearQuery,
    active,
    revision,
  );
  const plans = useCoreQuery<PlanValueResponse>(
    action,
    "plan_value",
    { hiddenModels: [] },
    active,
    `${revision}|${accounts}`,
  );
  return {
    data: range.data,
    error: range.error,
    loading: !range.data && !range.error && range.fetching,
    year: year.data,
    plans: plans.data,
  };
}

/** The page's range, kept between visits; a day picked on the heatmap is
 * not kept. */
export function useSavedRange(): [Range, (range: Range) => void] {
  const [range, setRange] = useState<Range>(() => {
    try {
      const id = localStorage.getItem(storageKeys.statisticsRange);
      if (id && (rangeIds as string[]).includes(id)) return { id } as Range;
    } catch {
      // A page without storage starts at 30 days.
    }
    return { id: "30d" };
  });
  const keep = (next: Range) => {
    setRange(next);
    if (next.id !== "day")
      try {
        localStorage.setItem(storageKeys.statisticsRange, next.id);
      } catch {
        // The range is a convenience; nothing to keep without storage.
      }
  };
  return [range, keep];
}
