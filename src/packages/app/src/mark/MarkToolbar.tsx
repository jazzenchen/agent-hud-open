import type { ReactNode } from "react";
import {
  MessageSquarePlus,
  PenLine,
  Square,
  Trash2,
  Undo2,
} from "lucide-react";
import { useT } from "@agent-hud/ui/format-context";
import type { MarkTool } from "./mark-model";
import type { MarkColors } from "./MarkWindow";

function ToolButton({
  chosen,
  label,
  onClick,
  colors,
  children,
}: {
  chosen?: boolean;
  label: string;
  onClick: () => void;
  colors: MarkColors;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={chosen}
      title={label}
      onClick={onClick}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        height: 30,
        padding: "0 10px",
        border: 0,
        borderRadius: 7,
        background: chosen ? colors.chosen : "transparent",
        color: colors.text,
        fontSize: 13,
        fontWeight: chosen ? 600 : 500,
        cursor: "pointer",
      }}
    >
      {children}
      {label}
    </button>
  );
}

/** The markup window's tools: box, pen and comment, undo and clear, and
 * Cancel and Done. */
export function MarkToolbar({
  tool,
  choose,
  undo,
  clear,
  cancel,
  done,
  saving,
  colors,
}: {
  tool: MarkTool;
  choose: (tool: MarkTool) => void;
  undo: () => void;
  clear: () => void;
  cancel: () => void;
  done: () => void;
  saving: boolean;
  colors: MarkColors;
}) {
  const t = useT();
  const icon = { size: 15, strokeWidth: 2 };
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 4,
        padding: "8px 12px",
        borderBottom: `1px solid ${colors.line}`,
      }}
    >
      <ToolButton
        chosen={tool === "box"}
        label={t("Box")}
        onClick={() => choose("box")}
        colors={colors}
      >
        <Square {...icon} />
      </ToolButton>
      <ToolButton
        chosen={tool === "pen"}
        label={t("Pen")}
        onClick={() => choose("pen")}
        colors={colors}
      >
        <PenLine {...icon} />
      </ToolButton>
      <ToolButton
        chosen={tool === "pin"}
        label={t("Comment")}
        onClick={() => choose("pin")}
        colors={colors}
      >
        <MessageSquarePlus {...icon} />
      </ToolButton>
      <span style={{ width: 1, height: 18, background: colors.line }} />
      <ToolButton label={t("Undo")} onClick={undo} colors={colors}>
        <Undo2 {...icon} />
      </ToolButton>
      <ToolButton label={t("Clear")} onClick={clear} colors={colors}>
        <Trash2 {...icon} />
      </ToolButton>
      <span style={{ flex: 1 }} />
      <ToolButton label={t("Cancel")} onClick={cancel} colors={colors}>
        {null}
      </ToolButton>
      <button
        type="button"
        onClick={done}
        style={{
          height: 30,
          padding: "0 16px",
          border: 0,
          borderRadius: 7,
          background: "#0A84FF",
          color: "#FFFFFF",
          fontSize: 13,
          fontWeight: 600,
          cursor: saving ? "default" : "pointer",
          opacity: saving ? 0.6 : 1,
        }}
      >
        {saving ? t("Saving…") : t("Done")}
      </button>
    </div>
  );
}
