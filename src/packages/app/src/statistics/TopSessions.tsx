import type { Session, StatisticsProvider } from "@agent-hud/protocol";
import {
  compact,
  costText,
  integer,
  number,
  plain,
  row,
  truncate,
  type StatsTheme,
} from "./StatsTheme";

/** An agent card's top sessions in the selected range, each with a bar of its share of the agent's tokens. */
export function TopSessions({
  provider,
  accent,
  theme,
  onSelectSession,
}: {
  provider: StatisticsProvider;
  accent: string;
  theme: StatsTheme;
  onSelectSession?: (session: Session) => void;
}) {
  const zh = Boolean(theme.zh);
  return (
    <div style={{ display: "grid", gap: 7, flex: 1, minWidth: 0 }}>
      <span style={{ color: theme.secondary, fontSize: 11 }}>
        {zh ? "最高会话" : "Top sessions"}
      </span>
      {provider.topSessions.slice(0, 3).map((entry) => (
        <button
          key={entry.session.id}
          type="button"
          disabled={!onSelectSession}
          onClick={() => onSelectSession?.(entry.session)}
          title={
            zh
              ? `${entry.session.title}\n所选范围内 ${integer(entry.selectedTokens)} Token\n按 API 价格估算 ${costText(entry.cost, zh)}`
              : `${entry.session.title}\n${integer(entry.selectedTokens)} tokens in the selected range\n${costText(entry.cost, zh)} at API prices`
          }
          style={{
            ...plain,
            cursor: onSelectSession ? "pointer" : "default",
            minWidth: 0,
          }}
        >
          <div
            style={{
              ...row,
              justifyContent: "space-between",
              fontSize: 12,
              marginBottom: 3,
            }}
          >
            <span style={truncate}>
              {entry.session.title || (zh ? "未命名会话" : "Untitled session")}
            </span>
            <span style={number}>{compact(entry.selectedTokens)}</span>
          </div>
          <div
            style={{
              height: 2,
              borderRadius: 2,
              overflow: "hidden",
              background: theme.track,
            }}
          >
            <div
              style={{
                width: `${provider.selectedTokens ? Math.min(100, (entry.selectedTokens / provider.selectedTokens) * 100) : 0}%`,
                height: "100%",
                background: accent,
                transition: "width 240ms cubic-bezier(0.22, 1, 0.36, 1)",
              }}
            />
          </div>
        </button>
      ))}
      {!provider.topSessions.length && (
        <span style={{ fontSize: 11, color: theme.secondary }}>
          {zh ? "该范围内无已归属会话" : "No attributed sessions in this range"}
        </span>
      )}
    </div>
  );
}
