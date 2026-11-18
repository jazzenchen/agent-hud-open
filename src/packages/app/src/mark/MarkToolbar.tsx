import { useEffect, useState, type ReactNode } from "react";
import {
  Circle,
  MessageSquarePlus,
  PanelRight,
  PenLine,
  Square,
  Trash2,
  Undo2,
} from "lucide-react";
import { useT } from "@agent-hud/ui/format-context";
import type { MarkTool } from "./mark-model";
import type { MarkColors } from "./mark-theme";

/** Each tool's key, pressed while nothing is being typed. */
export const TOOL_KEYS: Record<string, MarkTool> = {
  b: "box",
  o: "circle",
  p: "pen",
  c: "pin",
};

/** Below this width the tools show their icons alone. */
const NAMED_FROM = 860;

function useNamed() {
  const [named, setNamed] = useState(() => innerWidth >= NAMED_FROM);
  useEffect(() => {
    const measure = () => setNamed(innerWidth >= NAMED_FROM);
    addEventListener("resize", measure);
    return () => removeEventListener("resize", measure);
  }, []);
  return named;
}

function Tool({
  chosen,
  label,
  named,
  hint,
  onClick,
  colors,
  children,
}: {
  chosen: boolean;
  label: string;
  named: boolean;
  hint: string;
  onClick: () => void;
  colors: MarkColors;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={chosen}
      title={`${label} (${hint})`}
      onClick={onClick}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        height: 26,
        padding: "0 11px",
        border: 0,
        borderRadius: 6,
        background: chosen ? colors.chosen : "transparent",
        boxShadow: chosen ? colors.chosenShadow : "none",
        color: chosen ? colors.text : colors.quiet,
        fontSize: 12.5,
        fontWeight: 500,
        cursor: "default",
      }}
    >
      {children}
      {named && label}
    </button>
  );
}

function Action({
  label,
  onClick,
  disabled,
  colors,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled: boolean;
  colors: MarkColors;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      style={{
        display: "inline-grid",
        placeItems: "center",
        width: 30,
        height: 28,
        border: 0,
        borderRadius: 6,
        background: "transparent",
        color: colors.text,
        opacity: disabled ? 0.3 : 0.8,
        cursor: "default",
      }}
    >
      {children}
    </button>
  );
}

/** The markup window's toolbar, in its title bar: the tools in the middle
 * (box, circle, pen, comment, then undo and clear); the comment list's
 * switch, Cancel and Done at the right. The empty left keeps clear of the
 * window's own buttons. */
export function MarkToolbar({
  tool,
  choose,
  undo,
  clear,
  cancel,
  done,
  empty,
  saving,
  list,
  colors,
}: {
  tool: MarkTool;
  choose: (tool: MarkTool) => void;
  undo: () => void;
  clear: () => void;
  cancel: () => void;
  done: () => void;
  /** Nothing drawn yet: Undo and Clear rest. */
  empty: boolean;
  saving: boolean;
  /** The comment list: shown or folded away, and how many it holds. */
  list: { open: boolean; count: number; toggle: () => void };
  colors: MarkColors;
}) {
  const t = useT();
  const named = useNamed();
  const icon = { size: 14, strokeWidth: 2 };
  const tools: [MarkTool, string, string, ReactNode][] = [
    ["box", t("Box"), "B", <Square key="box" {...icon} />],
    ["circle", t("Circle"), "O", <Circle key="circle" {...icon} />],
    ["pen", t("Pen"), "P", <PenLine key="pen" {...icon} />],
    ["pin", t("Comment"), "C", <MessageSquarePlus key="pin" {...icon} />],
  ];
  return (
    <header
      data-tauri-drag-region
      style={{
        display: "grid",
        gridTemplateColumns: "1fr auto 1fr",
        alignItems: "center",
        height: 52,
        flexShrink: 0,
        padding: "0 14px",
        background: colors.ground,
        borderBottom: `1px solid ${colors.line}`,
      }}
    >
      <span data-tauri-drag-region />
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <div
          role="group"
          style={{
            display: "flex",
            gap: 2,
            padding: 2,
            borderRadius: 8,
            background: colors.group,
          }}
        >
          {tools.map(([id, label, hint, glyph]) => (
            <Tool
              key={id}
              chosen={tool === id}
              label={label}
              named={named}
              hint={hint}
              onClick={() => choose(id)}
              colors={colors}
            >
              {glyph}
            </Tool>
          ))}
        </div>
        <div style={{ display: "flex" }}>
          <Action
            label={`${t("Undo")} (⌘Z)`}
            onClick={undo}
            disabled={empty}
            colors={colors}
          >
            <Undo2 {...icon} />
          </Action>
          <Action
            label={t("Clear")}
            onClick={clear}
            disabled={empty}
            colors={colors}
          >
            <Trash2 {...icon} />
          </Action>
        </div>
      </div>
      <div
        data-tauri-drag-region
        style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}
      >
        <button
          type="button"
          aria-pressed={list.open}
          title={list.open ? t("Hide comments") : t("Show comments")}
          onClick={list.toggle}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 5,
            height: 28,
            padding: "0 9px",
            border: 0,
            borderRadius: 7,
            background: list.open ? colors.group : "transparent",
            color: colors.text,
            fontSize: 12.5,
            fontWeight: 600,
            cursor: "default",
          }}
        >
          <PanelRight {...icon} />
          {list.count > 0 && list.count}
        </button>
        <button
          type="button"
          onClick={cancel}
          style={{
            height: 28,
            padding: "0 12px",
            border: `1px solid ${colors.line}`,
            borderRadius: 7,
            background: "transparent",
            color: colors.text,
            fontSize: 13,
            fontWeight: 500,
            cursor: "default",
          }}
        >
          {t("Cancel")}
        </button>
        <button
          type="button"
          title={`${t("Done")} (⌘↩)`}
          onClick={done}
          disabled={saving}
          style={{
            height: 28,
            padding: "0 16px",
            border: 0,
            borderRadius: 7,
            background: "#0A84FF",
            color: "#FFFFFF",
            fontSize: 13,
            fontWeight: 600,
            cursor: "default",
            opacity: saving ? 0.6 : 1,
          }}
        >
          {saving ? t("Saving…") : t("Done")}
        </button>
      </div>
    </header>
  );
}
