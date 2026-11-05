import { useMemo } from "react";
import { useCoreQuery } from "../lib/use-core-query";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { useFormat } from "@agent-hud/ui/format-context";
import {
  budgetText,
  outlookBasisText,
  outlookText,
  quotaDisplayLabel,
  quotaPercentLabel,
  type QuotaDisplay,
} from "@agent-hud/ui/format";
import type {
  NativeMethod,
  Quota,
  QuotaHistoryResponse,
} from "@agent-hud/protocol";
import {
  dateLabel,
  number,
  row,
  truncate,
  type StatsTheme,
} from "./StatsTheme";
import { QuotaLine } from "./QuotaLine";
import { SectionFilter } from "./SectionFilter";
import type { ModelOption } from "./ModelFilter";
import { useHiddenFilter } from "./section-filter";
import { storageKeys } from "../lib/storage-keys";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/** What the window reads now, from the snapshot Core worked out: its share, its budget, and its forecast with the pace it comes from. */
function nowLine(
  quota: Quota | undefined,
  display: QuotaDisplay,
  now: number,
  zh: boolean,
) {
  if (!quota || quota.usedPercent == null || quota.status === "stale")
    return null;
  const language = zh ? "zh" : "en";
  const parts = [quotaPercentLabel(quota.usedPercent, display)];
  const metrics = quota.metrics;
  if (metrics?.budget && Number.isFinite(metrics.budget.paceDelta))
    parts.push(budgetText(metrics.budget, language));
  const outlook = metrics?.outlook;
  if (outlook) {
    const exhaustsAt = outlook.exhaustsAt
      ? Date.parse(outlook.exhaustsAt)
      : NaN;
    const words = outlookText(
      {
        kind: outlook.kind,
        seconds: Number.isFinite(exhaustsAt)
          ? Math.max(0, (exhaustsAt - now) / 1000)
          : undefined,
        beforeReset: outlook.beforeReset,
      },
      outlook.projectedUsedPercent ?? null,
      language,
    );
    if (words && ["exhausted", "exhausts", "no-usage"].includes(outlook.kind)) {
      parts.push(words);
      if (outlook.basis) parts.push(outlookBasisText(outlook.basis, language));
    }
  }
  return parts.join(" · ");
}

/** The Statistics window's quota history: one row per shown window over the selected range, each on its own fixed 0–100% axis. */
export function QuotaHistory({
  quotas,
  from,
  to,
  action,
  active,
  theme,
  light,
  timeZone,
}: {
  quotas: Quota[];
  from: number;
  to: number;
  action: Action;
  active: boolean;
  theme: StatsTheme;
  light: boolean;
  timeZone: string;
}) {
  // Only the latest stretch publishes; a failure keeps the last one drawn.
  const { data, error } = useCoreQuery<QuotaHistoryResponse>(
    action,
    "quota_history",
    { from: new Date(from).toISOString(), to: new Date(to).toISOString() },
    active,
  );
  const display = useFormat().quotaDisplay ?? "consumed";
  const zh = Boolean(theme.zh);
  const now = Date.now();
  const live = useMemo(
    () => new Map(quotas.map((quota) => [quota.id, quota])),
    [quotas],
  );
  const shownFrom = data ? Date.parse(data.from) : from;
  const shownTo = data ? Date.parse(data.to) : to;
  // A stretch of another length, once it has come, draws the lines in again.
  const reveal = String(shownTo - shownFrom);
  const filter = useHiddenFilter(storageKeys.quotaHistoryFilter);
  // Each vendor lists the models whose calls spend one of its windows.
  const choices = useMemo(() => {
    const names = new Map(
      (data?.models ?? []).map((model) => [model.id, model.name]),
    );
    const agents =
      data?.agents ??
      [...new Set((data?.windows ?? []).map((quota) => quota.vendor))].map(
        (id) => ({ id, name: id }),
      );
    const models: ModelOption[] = (data?.windows ?? []).flatMap((quota) =>
      (quota.models ?? []).map((id) => ({
        id,
        name: names.get(id) ?? id,
        agent: quota.vendor,
      })),
    );
    return { agents, models };
  }, [data]);
  // A window shows while its vendor does and, when calls spent it in the
  // stretch, while one of their models does.
  const windows = (data?.windows ?? []).filter(
    (quota) =>
      filter.agentShown(quota.vendor) &&
      (!quota.models?.length || quota.models.some(filter.modelShown)),
  );
  const accounts = useMemo(() => {
    const ids = new Map<string, Set<string>>();
    for (const quota of data?.windows ?? []) {
      const vendor = ids.get(quota.vendor) ?? new Set<string>();
      vendor.add(quota.accountId ?? "");
      ids.set(quota.vendor, vendor);
    }
    return ids;
  }, [data]);
  return (
    <div
      className="statistics-quota-history"
      style={{ padding: "6px 16px 8px", minWidth: 0 }}
    >
      <div
        style={{
          ...row,
          alignItems: "baseline",
          justifyContent: "space-between",
          marginBottom: 6,
        }}
      >
        <strong style={{ fontSize: 13, fontWeight: 600 }}>
          {zh ? "额度历史" : "Quota history"}
        </strong>
        <span style={{ color: theme.secondary, fontSize: 11 }}>
          {quotaDisplayLabel(display, zh ? "zh" : "en")} · 0–100%
        </span>
      </div>
      {data && data.windows.length > 0 && (
        <SectionFilter
          agents={choices.agents}
          models={choices.models}
          state={filter}
          theme={theme}
          light={light}
        />
      )}
      {error && (
        <div role="alert" style={{ color: theme.secondary, fontSize: 11 }}>
          {zh ? "额度历史暂不可用：" : "Quota history is unavailable: "}
          {error}
        </div>
      )}
      {!data && !error && (
        <div style={{ color: theme.secondary, fontSize: 11, padding: "8px 0" }}>
          {zh ? "正在加载额度历史…" : "Loading quota history…"}
        </div>
      )}
      {data && !data.windows.length && (
        <div style={{ color: theme.secondary, fontSize: 11, padding: "8px 0" }}>
          {zh ? "暂无可显示的额度窗口" : "No quota windows to show"}
        </div>
      )}
      {data && data.windows.length > 0 && !windows.length && (
        <div style={{ color: theme.secondary, fontSize: 11, padding: "8px 0" }}>
          {zh ? "筛选隐藏了全部额度窗口" : "The filter hides every window"}
        </div>
      )}
      <div style={{ display: "grid", gap: 14, paddingTop: 8 }}>
        {windows.map((quota) => {
          const reading = nowLine(live.get(quota.id), display, now, zh);
          const several = (accounts.get(quota.vendor)?.size ?? 0) > 1;
          return (
            <div key={quota.id} style={{ display: "grid", gap: 5 }}>
              <div style={{ ...row, fontSize: 11 }}>
                <HUDLogo source={quota.vendor} size={12} light={light} />
                <span
                  style={{ ...truncate, fontWeight: 600 }}
                  title={quota.name}
                >
                  {quota.shortLabel ?? quota.name}
                </span>
                {several && quota.account && (
                  <span style={{ ...truncate, color: theme.secondary }}>
                    {quota.account}
                  </span>
                )}
                {reading && (
                  <span
                    title={reading}
                    style={{
                      ...truncate,
                      ...number,
                      marginLeft: "auto",
                      color: theme.secondary,
                    }}
                  >
                    {reading}
                  </span>
                )}
              </div>
              <QuotaLine
                window={quota}
                from={shownFrom}
                to={shownTo}
                display={display}
                theme={theme}
                reveal={reveal}
                timeZone={timeZone}
              />
            </div>
          );
        })}
      </div>
      {windows.length > 0 && (
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            paddingTop: 7,
            color: theme.secondary,
            fontSize: 10,
          }}
        >
          {[0, 1 / 3, 2 / 3, 1].map((part) => (
            <span key={part} style={{ maxWidth: "26%" }}>
              {dateLabel(
                shownFrom + (shownTo - shownFrom) * part,
                timeZone,
                zh,
              )}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
