import { ChevronLeft, X } from "lucide-react";
import { clipped, plain, row, type Theme } from "./SessionTheme";
import { NoSessionArt } from "./NoSessionArt";
import { useT } from "@agent-hud/ui/format-context";

/** The way back above a session's detail: to its parent session, or to
 * the list. It sticks under the toolbar, or at the top of the detail's own
 * column beside the list. Over a pick's column it names the pick instead,
 * with a × that closes it. */
export function SessionBackBar({
  label,
  top,
  onBack,
  onClose,
  theme,
}: {
  label: string;
  top: number;
  onBack?: () => void;
  onClose?: () => void;
  theme: Theme;
}) {
  const t = useT();
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
      {onBack ? (
        <button
          type="button"
          onClick={onBack}
          style={{ ...plain, ...row, fontSize: 12, fontWeight: 600 }}
        >
          <ChevronLeft size={13} />
          <span style={clipped}>{label}</span>
        </button>
      ) : (
        <span style={{ ...clipped, fontSize: 12, fontWeight: 600, flex: 1 }}>
          {label}
        </span>
      )}
      {onClose && (
        <button
          type="button"
          aria-label={t("Close")}
          title={t("Close (⌘[)")}
          onClick={onClose}
          style={{
            ...plain,
            ...row,
            marginLeft: "auto",
            color: theme.secondary,
            flexShrink: 0,
          }}
        >
          <X size={13} />
        </button>
      )}
    </div>
  );
}

/** The detail column before a session is picked: a session sketched
 * faintly, and what picking one shows. */
export function NoSessionPicked({ theme }: { theme: Theme }) {
  const t = useT();
  return (
    <div
      style={{
        height: "100%",
        minHeight: 240,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 18,
        padding: "24px 16px",
        color: theme.secondary,
        fontSize: 12,
        textAlign: "center",
      }}
    >
      <NoSessionArt theme={theme} />
      <span>{t("Pick a session to see its turns, tokens and agents")}</span>
    </div>
  );
}
