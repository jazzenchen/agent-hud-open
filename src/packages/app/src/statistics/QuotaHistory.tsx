import { useMemo } from "react";
import { useCoreQuery } from "../lib/use-core-query";
import { useFormat } from "@agent-hud/ui/format-context";
import { quotaDisplayLabel } from "@agent-hud/ui/format";
import type {
  NativeMethod,
  Quota,
  QuotaHistoryResponse,
} from "@agent-hud/protocol";
import { dateLabel, row, type StatsTheme } from "./StatsTheme";
import { QuotaAgentChart } from "./QuotaAgentChart";
import { quotaGroups } from "@agent-hud/hud/panel/quota-groups";
import { SectionFilter } from "./SectionFilter";
import type { ModelOption } from "./ModelFilter";
import { useHiddenFilter } from "./section-filter";
import { storageKeys } from "../lib/storage-keys";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/** The Statistics window's quota history: one chart per agent (per account
 * where it has several) over the selected range, its windows' lines on one
 * fixed 0–100% axis. */
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
  const groups = quotaGroups(windows);
  const names = new Map(choices.agents.map((agent) => [agent.id, agent.name]));
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
      <div style={{ display: "grid", gap: 16, paddingTop: 8 }}>
        {groups.map((group) => (
          <QuotaAgentChart
            key={group.key}
            group={group}
            name={names.get(group.vendor) ?? group.vendor}
            live={live}
            display={display}
            now={now}
            from={shownFrom}
            to={shownTo}
            reveal={reveal}
            timeZone={timeZone}
            theme={theme}
            light={light}
          />
        ))}
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
