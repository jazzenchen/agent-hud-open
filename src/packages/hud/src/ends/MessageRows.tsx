import { useState } from "react";
import { MessageSquareText, TriangleAlert, X } from "lucide-react";
import type { IslandEvent } from "@agent-hud/protocol";
import { dateFormat } from "@agent-hud/ui/format";
import { HUDArtwork } from "../HUDArtwork";
import { clientDisplayName, newsTitle } from "../IslandNews";
import { cardColumn } from "../card-column";
import { useGround } from "../ground";
import { alertInk, alertText } from "../tokens";
import type { TailItem } from "./end-queue";
import { useT } from "@agent-hud/ui/format-context";

/** "Messages" over the tail's panel, with how many and a way to clear them. */
export function MessagesHeader({
  count,
  clear,
  light,
}: {
  count: number;
  clear: () => void;
  light: boolean;
}) {
  const t = useT();
  const ground = useGround(light);
  const [hover, setHover] = useState(false);
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "0 4px 8px",
      }}
    >
      <span
        style={{ fontSize: 11, fontWeight: 600, color: alertInk(0.55, ground) }}
      >
        {t("Messages")}
      </span>
      <span
        style={{
          fontSize: 10,
          fontWeight: 600,
          fontVariantNumeric: "tabular-nums",
          color: alertText("turn", ground),
          padding: "2px 6px",
          borderRadius: 999,
          background: "rgba(130, 170, 255, 0.16)",
        }}
      >
        {count}
      </span>
      <button
        type="button"
        onClick={clear}
        onPointerEnter={() => setHover(true)}
        onPointerLeave={() => setHover(false)}
        style={{
          marginLeft: "auto",
          fontSize: 11,
          border: 0,
          borderRadius: 6,
          padding: "2px 6px",
          background: hover
            ? light
              ? "rgba(0,0,0,0.05)"
              : "rgba(255,255,255,0.08)"
            : "transparent",
          color: alertInk(0.55, ground),
          cursor: "pointer",
        }}
      >
        {t("Clear all")}
      </button>
    </div>
  );
}

/** One message waiting in the tail: its agent, what it is, its title and
 * time. Pressing it shows it; its × lets it go. */
function MessageRow({
  event,
  onSelect,
  onDrop,
  light,
}: {
  event: IslandEvent;
  onSelect: () => void;
  onDrop: () => void;
  light: boolean;
}) {
  const t = useT();
  const [hover, setHover] = useState(false);
  const ground = useGround(light);
  const reply = event.kind === "completion";
  const Icon = reply ? MessageSquareText : TriangleAlert;
  return (
    <div
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
      style={{
        display: "flex",
        alignItems: "center",
        borderRadius: 8,
        background: hover
          ? light
            ? "rgba(0,0,0,0.05)"
            : "rgba(255,255,255,0.06)"
          : "transparent",
      }}
    >
      <button
        type="button"
        onClick={onSelect}
        style={{
          flex: 1,
          minWidth: 0,
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "9px 4px 9px 10px",
          border: 0,
          background: "transparent",
          color: "inherit",
          textAlign: "left",
          cursor: "pointer",
        }}
      >
        <span style={{ opacity: 0.8, display: "inline-flex", flexShrink: 0 }}>
          <HUDArtwork source={event.source} size={13} light={light} />
        </span>
        <span style={{ fontSize: 12, fontWeight: 500, flexShrink: 0 }}>
          {clientDisplayName(event.source)}
        </span>
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 3,
            padding: "1px 4px",
            borderRadius: 3,
            background: reply
              ? "rgba(130, 170, 255, 0.14)"
              : "rgba(233, 161, 109, 0.14)",
            color: alertText(reply ? "turn" : "warning", ground),
            flexShrink: 0,
          }}
        >
          <Icon size={8} strokeWidth={2.5} />
          <span style={{ fontSize: 10, fontWeight: 700 }}>
            {newsTitle(event, t)}
          </span>
        </span>
        <span
          style={{
            fontSize: 11,
            color: alertInk(0.55, ground),
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            flex: 1,
            minWidth: 0,
          }}
        >
          {event.title ?? event.message ?? ""}
        </span>
        <time
          style={{
            fontSize: 11,
            fontVariantNumeric: "tabular-nums",
            color: alertInk(0.38, ground),
            flexShrink: 0,
          }}
        >
          {dateFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(
            new Date(event.at),
          )}
        </time>
      </button>
      <button
        type="button"
        aria-label={t("Remove this message")}
        onClick={onDrop}
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          width: 26,
          height: 26,
          marginRight: 4,
          flexShrink: 0,
          border: 0,
          borderRadius: 6,
          background: "transparent",
          color: alertInk(hover ? 0.6 : 0.32, ground),
          cursor: "pointer",
        }}
      >
        <X size={12} strokeWidth={2} />
      </button>
    </div>
  );
}

/** The tail's other messages, newest first, under the one on show. */
export function MessageRows({
  items,
  currentID,
  select,
  drop,
  light,
}: {
  items: TailItem[];
  currentID: string;
  select: (id: string) => void;
  drop: (id: string) => void;
  light: boolean;
}) {
  const others = items.filter((item) => item.id !== currentID);
  if (!others.length) return null;
  return (
    <div style={{ ...cardColumn(2), marginTop: 10 }}>
      {others.map((item) => (
        <MessageRow
          key={item.id}
          event={item.event}
          onSelect={() => select(item.id)}
          onDrop={() => drop(item.id)}
          light={light}
        />
      ))}
    </div>
  );
}
