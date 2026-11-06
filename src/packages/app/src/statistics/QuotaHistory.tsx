import { useMemo, useState } from "react";
import { vendorKey } from "@agent-hud/hud/model";
import { useCoreQuery } from "../lib/use-core-query";
import { useFormat } from "@agent-hud/ui/format-context";
import type {
  NativeMethod,
  Quota,
  QuotaHistoryResponse,
} from "@agent-hud/protocol";
import { row, type StatsTheme } from "./StatsTheme";
import { QuotaAgentChart } from "./QuotaAgentChart";
import { Segments } from "./Segments";
import { cycleSpan, quotaGroups } from "@agent-hud/hud/panel/quota-groups";
import { SectionFilter } from "./SectionFilter";
import type { ModelOption } from "./ModelFilter";
import { useHiddenFilter } from "./section-filter";
import { storageKeys } from "../lib/storage-keys";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/** The Statistics window's quota history: for each agent (each account
 * where it has several), a lane per window with a bar per cycle over the last
 * week or month, its height the share that cycle used. It keeps its own
 * range: a cycle can be a week long, longer than the page's shorter ones. */
export function QuotaHistory({
  quotas,
  to,
  action,
  active,
  theme,
  light,
  timeZone,
  agents = null,
}: {
  quotas: Quota[];
  /** The latest reading: the history runs up to it. */
  to: number;
  action: Action;
  active: boolean;
  theme: StatsTheme;
  light: boolean;
  timeZone: string;
  /** The page's agent filter: only the picked agents' windows show. */
  agents?: string[] | null;
}) {
  const [days, setDays] = useState(30);
  const from = to - days * 86_400_000;
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
  const shownTo = data ? Date.parse(data.to) : to;
  const shownDays = data
    ? Math.round((shownTo - Date.parse(data.from)) / 86_400_000)
    : days;
  const { from: shownFrom, until } = cycleSpan(shownTo, shownDays);
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
      (!agents || agents.some((id) => vendorKey(id) === quota.vendor)) &&
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
        <span style={{ ...row, gap: 10, color: theme.secondary, fontSize: 11 }}>
          {zh
            ? "每个周期用掉的份额 · 0–100%"
            : "Share each cycle used · 0–100%"}
          <Segments
            label={zh ? "额度历史范围" : "Quota history range"}
            value={String(days)}
            onChange={(value) => setDays(Number(value))}
            choices={[
              { value: "7", label: zh ? "7 天" : "7 days" },
              { value: "30", label: zh ? "30 天" : "30 days" },
            ]}
            theme={theme}
          />
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
            until={until}
            days={shownDays}
            timeZone={timeZone}
            theme={theme}
            light={light}
          />
        ))}
      </div>
    </div>
  );
}
