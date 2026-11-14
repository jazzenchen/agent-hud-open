import { useState } from "react";
import { MessageSquareText } from "lucide-react";
import type { IslandEvent } from "@agent-hud/protocol";
import { HUDArtwork } from "../HUDArtwork";
import { clientDisplayName, newsTitle } from "../IslandNews";
import { alertText } from "../tokens";
import { useGround } from "../ground";
import { dateFormat } from "@agent-hud/ui/format";
import { useT } from "@agent-hud/ui/format-context";

/** A finished turn waiting behind the card on show: the agent, "New reply",
 * the turn's title and the time it finished. */
export function CompletionClosedRow({
  event,
  onSelect,
  light = false,
}: {
  event: IslandEvent;
  onSelect: () => void;
  light?: boolean;
}) {
  const t = useT();
  const [hover, setHover] = useState(false);
  const segmentBg = light ? "rgba(0,0,0,0.05)" : "rgba(255,255,255,0.06)";
  const turnText = alertText("turn", useGround(light));
  const secondary = light ? "rgba(0,0,0,0.55)" : "rgba(255,255,255,0.55)";
  const tertiary = light ? "rgba(0,0,0,0.38)" : "rgba(255,255,255,0.38)";
  return (
    <button
      type="button"
      onClick={onSelect}
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
      style={{
        width: "100%",
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "9px 10px",
        borderRadius: 8,
        border: 0,
        background: hover ? segmentBg : "transparent",
        color: "inherit",
        textAlign: "left",
        cursor: "pointer",
      }}
    >
      <span style={{ opacity: 0.8, display: "inline-flex", flexShrink: 0 }}>
        <HUDArtwork source={event.source} size={13} light={light} />
      </span>
      <span
        style={{
          fontSize: 12,
          fontWeight: 500,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          flexShrink: 0,
        }}
      >
        {clientDisplayName(event.source)}
      </span>
      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 3,
          padding: "1px 4px",
          borderRadius: 3,
          background: "rgba(130, 170, 255, 0.14)",
          color: turnText,
          flexShrink: 0,
        }}
      >
        <MessageSquareText size={8} strokeWidth={2.5} />
        <span
          style={{
            fontSize: 10,
            fontWeight: 700,
            fontVariantNumeric: "tabular-nums",
          }}
        >
          {t("New reply")}
        </span>
      </span>
      <span
        style={{
          fontSize: 11,
          color: secondary,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          flex: 1,
          minWidth: 0,
        }}
      >
        {event.title ?? newsTitle(event, t)}
      </span>
      <time
        style={{
          fontSize: 11,
          fontVariantNumeric: "tabular-nums",
          color: tertiary,
          flexShrink: 0,
        }}
      >
        {dateFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(
          new Date(event.at),
        )}
      </time>
    </button>
  );
}
