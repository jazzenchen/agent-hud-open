import type { ReactNode } from "react";
import { offset } from "./subagent-lanes";
import { plain, row, type Theme } from "./SessionTheme";

/** The timeline's key: a call's tick, a long step, the session waiting, a sub-agent sent and handed back; then how to zoom, or to return from a zoom. */
export function SubagentTimelineLegend({
  zoomed,
  onShowAll,
  theme,
}: {
  /** How long the zoomed view is, ms; null while the whole turn shows. */
  zoomed: number | null;
  onShowAll: () => void;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  return (
    <div
      style={{
        ...row,
        flexWrap: "wrap",
        columnGap: 14,
        rowGap: 4,
        fontSize: 10.5,
        color: theme.secondary,
      }}
    >
      <Key>
        <span
          style={{
            width: 2,
            height: 10,
            borderRadius: 1,
            background: theme.secondary,
          }}
        />
        {zh ? "模型调用" : "Model call"}
      </Key>
      <Key>
        <span
          style={{
            width: 14,
            height: 8,
            background: `color-mix(in srgb, ${theme.secondary} 55%, transparent)`,
          }}
        />
        {zh ? "长步骤（一步 ≥15 秒）" : "Long step (≥15 s)"}
      </Key>
      <Key>
        <span
          style={{ width: 14, borderTop: `1.5px dotted ${theme.tertiary}` }}
        />
        {zh ? "主会话等待" : "Session waiting"}
      </Key>
      <Key>
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: 3,
            background: theme.secondary,
          }}
        />
        {zh ? "派出 / 交回" : "Sent / handed back"}
      </Key>
      {zoomed !== null ? (
        <button
          type="button"
          onClick={onShowAll}
          style={{ ...plain, color: theme.action, marginLeft: "auto" }}
        >
          {zh
            ? `已放大 ${offset(zoomed)} · 还原整轮`
            : `Zoomed to ${offset(zoomed)} · Show the whole turn`}
        </button>
      ) : (
        <span style={{ marginLeft: "auto" }}>
          {zh ? "拖选一段放大" : "Drag across to zoom"}
        </span>
      )}
    </div>
  );
}

function Key({ children }: { children: ReactNode }) {
  return <span style={{ ...row, gap: 5 }}>{children}</span>;
}
