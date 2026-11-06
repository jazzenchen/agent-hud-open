import { moneyShort } from "@agent-hud/ui/format";
import type { StatisticsResponse } from "@agent-hud/protocol";
import { compact, number, raisedCard, type StatsTheme } from "../StatsTheme";
import type { Day } from "./days";
import { UsageBreakdown } from "./UsageBreakdown";

/** A day's hover card on the heatmap: its tokens, worth and sessions, then
 * by agent and model; or why it has none. */
export function DayTip({
  day,
  left,
  top,
  pending,
  bucket,
  available,
  agents,
  title,
  theme,
}: {
  day: Day;
  left: number;
  top: number;
  /** The history has not been read back to this day yet. */
  pending: boolean;
  bucket: {
    cost: number;
    sessions: number | null;
    models: Record<string, number>;
  } | null;
  available: StatisticsResponse["available"];
  agents: string[] | null;
  title: string;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  return (
    <div
      style={{
        ...raisedCard(theme),
        position: "absolute",
        left,
        top,
        width: 260,
        fontSize: 11,
      }}
    >
      <strong style={{ fontWeight: 600, fontSize: 12 }}>{title}</strong>
      {pending && (
        <span style={{ color: theme.secondary }}>
          {t("Still being read", "还在读取")}
        </span>
      )}
      {day.outside ? (
        <span style={{ color: theme.secondary }}>
          {t("Before the record starts", "记录开始之前")}
        </span>
      ) : (
        <>
          <span style={{ ...number, fontSize: 14, fontWeight: 600 }}>
            {compact(day.value)} {t("tokens", "Token")}
            {bucket && (
              <span
                style={{
                  fontSize: 11,
                  fontWeight: 400,
                  color: theme.secondary,
                }}
              >
                {" · "}
                {moneyShort(bucket.cost, "USD", zh ? "zh" : "en")}
                {bucket.sessions !== null
                  ? zh
                    ? ` · ${bucket.sessions} 个会话`
                    : ` · ${bucket.sessions} ${bucket.sessions === 1 ? "session" : "sessions"}`
                  : ""}
              </span>
            )}
          </span>
          {bucket && (
            <UsageBreakdown
              models={bucket.models}
              available={available}
              agents={agents}
              theme={theme}
            />
          )}
        </>
      )}
    </div>
  );
}
