import type { ReactNode } from "react";
import { offset } from "./subagent-lanes";
import { plain, row, type Theme } from "./SessionTheme";
import { useT } from "@agent-hud/ui/format-context";

/** The timeline's key: a call's tick, calls in a row, a long step, the session waiting, a sub-agent sent and handed back; then how to zoom, or how to pan and return from a zoom. */
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
  const t = useT();
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
        {t("Model call")}
      </Key>
      <Key>
        <span
          style={{
            width: 14,
            height: 8,
            borderRadius: 2,
            background: theme.secondary,
          }}
        />
        {t("Calls in a row")}
      </Key>
      <Key>
        <span
          style={{
            width: 14,
            height: 8,
            background: `color-mix(in srgb, ${theme.secondary} 55%, transparent)`,
          }}
        />
        {t("Long step (≥15 s)")}
      </Key>
      <Key>
        <span
          style={{ width: 14, borderTop: `1.5px dotted ${theme.tertiary}` }}
        />
        {t("Session waiting")}
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
        {t("Sent / handed back")}
      </Key>
      {zoomed !== null ? (
        <span style={{ ...row, gap: 14, marginLeft: "auto" }}>
          {t("Swipe sideways to pan")}
          <button
            type="button"
            onClick={onShowAll}
            style={{ ...plain, color: theme.action }}
          >
            {t("Zoomed to {{span}} · Show the whole turn", {
              span: offset(zoomed),
            })}
          </button>
        </span>
      ) : (
        <span style={{ marginLeft: "auto" }}>{t("Drag across to zoom")}</span>
      )}
    </div>
  );
}

function Key({ children }: { children: ReactNode }) {
  return <span style={{ ...row, gap: 5 }}>{children}</span>;
}
