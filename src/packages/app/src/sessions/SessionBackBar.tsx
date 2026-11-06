import { ChevronLeft } from "lucide-react";
import { clipped, plain, row, type Theme } from "./SessionTheme";

/** The way back above a session's detail: to its parent session, or to
 * the list. It sticks under the toolbar, or at the top of the detail's own
 * column beside the list. */
export function SessionBackBar({
  label,
  top,
  onBack,
  theme,
}: {
  label: string;
  top: number;
  onBack: () => void;
  theme: Theme;
}) {
  return (
    <div
      style={{
        ...row,
        position: "sticky",
        top,
        zIndex: 15,
        background: `var(--statistics-ground, ${theme.ground})`,
        minHeight: 28,
        paddingTop: 6,
        paddingBottom: 12,
        marginBottom: 14,
        borderBottom: `1px solid ${theme.divider}`,
      }}
    >
      <button
        type="button"
        onClick={onBack}
        style={{ ...plain, ...row, fontSize: 12, fontWeight: 600 }}
      >
        <ChevronLeft size={13} />
        <span style={clipped}>{label}</span>
      </button>
    </div>
  );
}

/** The detail column before a session is picked. */
export function NoSessionPicked({ theme }: { theme: Theme }) {
  const zh = Boolean(theme.zh);
  return (
    <div
      style={{
        height: "100%",
        minHeight: 240,
        display: "grid",
        placeItems: "center",
        color: theme.secondary,
        fontSize: 12,
        textAlign: "center",
      }}
    >
      {zh
        ? "选一个会话，查看它的回合、Token 和子 Agent"
        : "Pick a session to see its turns, tokens and agents"}
    </div>
  );
}
