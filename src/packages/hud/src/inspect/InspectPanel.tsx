import { useState } from "react";
import { useT } from "@agent-hud/ui/format-context";
import { usePanelServices } from "../PanelContext";
import { cardColumn } from "../card-column";
import { useGround } from "../ground";
import { alertInk, alertText } from "../tokens";
import { InspectRow, type ClipRow } from "./InspectRow";
import { TargetPicker } from "./TargetPicker";
import type { ClipEnd } from "./useClipEnd";

/** The clipboard bubble's window: where the words go, what was added to the
 * draft and the latest copy, each with a comment, a note for the whole
 * message, and Send. With nothing added, the latest copy is what goes.
 * Once the words are on their way the HUD steps aside (`handoff`). */
export function InspectPanel({
  clip,
  light,
  handoff,
}: {
  clip: ClipEnd;
  light: boolean;
  handoff: () => void;
}) {
  const t = useT();
  const ground = useGround(light);
  const { view, panel } = clip;
  const services = usePanelServices()?.inspect;
  const [note, setNote] = useState(view.note);
  const latest = view.clip && !view.clip.added ? view.clip : null;
  const rows: ClipRow[] = [
    ...view.items.map((item) => ({ ...item, pending: false })),
    ...(latest
      ? [{ ...latest, comment: "", pending: true } satisfies ClipRow]
      : []),
  ];
  const going = view.items.length || (latest?.content ? 1 : 0);
  const ready = Boolean(panel.target) && (going > 0 || note.trim() !== "");
  const send = () => {
    if (ready) void panel.send(handoff);
  };
  const actions = panel.actions;
  const quiet = {
    border: 0,
    borderRadius: 6,
    padding: "4px 8px",
    background: "transparent",
    cursor: "pointer",
    fontSize: 11,
    color: alertInk(0.5, ground),
  } as const;
  return (
    <div style={cardColumn(8)}>
      <TargetPicker panel={panel} light={light} />
      {rows.map((row, index) => (
        <InspectRow
          key={row.id}
          row={row}
          number={rows.length > 1 ? index + 1 : undefined}
          // The markup window comes forward; this window waits beside it.
          mark={() => void services?.markPicture(row.id)}
          add={() => actions?.add()}
          edit={(text) => actions?.edit(row.id, text)}
          remove={() => actions?.remove(row.id)}
          submit={send}
          light={light}
        />
      ))}
      <input
        type="text"
        value={note}
        placeholder={t("A note for the whole message")}
        aria-label={t("A note for the whole message")}
        onChange={(event) => {
          setNote(event.target.value);
          actions?.edit(null, event.target.value);
        }}
        onKeyDown={(event) => {
          if (
            event.key === "Enter" &&
            !event.nativeEvent.isComposing &&
            event.keyCode !== 229
          ) {
            event.preventDefault();
            send();
          }
        }}
        style={{
          border: 0,
          outline: 0,
          borderRadius: 8,
          padding: "8px 10px",
          background: light ? "rgba(0,0,0,0.035)" : "rgba(255,255,255,0.05)",
          color: alertInk(0.92, ground),
          fontSize: 12,
          fontFamily: "inherit",
        }}
      />
      {panel.outcome && !panel.outcome.sent && (
        // Why it did not go, whole: it often says what to do.
        <div
          style={{
            fontSize: 11,
            lineHeight: 1.4,
            color: alertText("warning", ground),
            overflowWrap: "anywhere",
            userSelect: "text",
          }}
        >
          {panel.outcome.reason ?? t("Not sent")}
        </div>
      )}
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <button type="button" onClick={() => actions?.clear()} style={quiet}>
          {t("Clear all")}
        </button>
        <span style={{ flex: 1 }} />
        <button
          type="button"
          onClick={send}
          style={{
            border: 0,
            borderRadius: 7,
            padding: "5px 12px",
            fontSize: 12,
            fontWeight: 600,
            cursor: ready && !panel.sending ? "pointer" : "default",
            background: "rgba(130, 170, 255, 0.18)",
            color: alertText("turn", ground),
            opacity: ready && !panel.sending ? 1 : 0.45,
          }}
        >
          {panel.sending
            ? t("Sending…")
            : going > 1
              ? t("Send {{number}}", { number: going })
              : t("Send")}
        </button>
      </div>
    </div>
  );
}
