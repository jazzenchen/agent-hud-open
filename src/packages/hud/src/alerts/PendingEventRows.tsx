import { useEffect, useState } from "react";
import type { Approval } from "@agent-hud/protocol";
import { HUDArtwork } from "../HUDArtwork";
import {
  clientDisplayName,
  compactDuration,
  type IslandCard,
} from "../IslandNews";
import { projectName } from "../project-path";
import { alertText } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { permissionToolBadge } from "./permission-tool-badge";
import { CompletionClosedRow } from "./CompletionClosedRow";

export function WaitedLabel({ createdAt }: { createdAt: string }) {
  const [, setTick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => setTick((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, []);
  const start = Date.parse(createdAt);
  const seconds = Number.isFinite(start)
    ? Math.max(0, Math.floor((Date.now() - start) / 1000))
    : 0;
  const label = seconds < 60 ? `${seconds}s` : compactDuration(seconds);
  return <span>{label}</span>;
}

export function PermissionRowHead({
  request,
  open,
  light = false,
}: {
  request: Approval;
  open: boolean;
  light?: boolean;
}) {
  const { badge, Icon } = permissionToolBadge(request);
  const who =
    projectName(request.cwd) ??
    request.vendor ??
    clientDisplayName(request.source);
  const summary = request.summary ?? request.title;
  const detail = request.detail ?? request.input;
  const isQuestion = request.kind === "question";
  const isPlan = request.kind === "plan";
  const showsDetail = Boolean(
    request.removed || request.added || detail || isPlan,
  );
  const hideSummary =
    open && (isQuestion || (showsDetail && detail === summary));
  const signalText = alertText("warning", useGround(light));
  const secondary = light ? "rgba(0,0,0,0.55)" : "rgba(255,255,255,0.55)";
  const tertiary = light ? "rgba(0,0,0,0.38)" : "rgba(255,255,255,0.38)";
  const openSummaryColor = light
    ? "rgba(0,0,0,0.85)"
    : "rgba(255,255,255,0.85)";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        minWidth: 0,
        width: "100%",
      }}
    >
      <span
        style={{
          opacity: open ? 1 : 0.8,
          display: "inline-flex",
          flexShrink: 0,
        }}
      >
        <HUDArtwork source={request.source} size={13} light={light} />
      </span>
      <span
        style={{
          fontSize: 12,
          fontWeight: 500,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          flexShrink: 0,
          maxWidth: 150,
        }}
      >
        {who}
      </span>
      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 3,
          padding: "1px 4px",
          borderRadius: 3,
          background: "rgba(233, 161, 109, 0.14)",
          color: signalText,
          flexShrink: 0,
        }}
      >
        <Icon size={8} strokeWidth={2.5} />
        <span
          style={{
            fontSize: 10,
            fontWeight: 700,
            fontVariantNumeric: "tabular-nums",
          }}
        >
          {badge}
        </span>
      </span>
      {!hideSummary && (
        <span
          style={{
            fontSize: 11,
            color: open ? openSummaryColor : secondary,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            flex: 1,
            minWidth: 0,
          }}
        >
          {summary}
        </span>
      )}
      {hideSummary && <span style={{ flex: 1 }} />}
      <span
        style={{
          fontSize: 11,
          fontVariantNumeric: "tabular-nums",
          color: tertiary,
          flexShrink: 0,
        }}
      >
        <WaitedLabel createdAt={request.createdAt} />
      </span>
    </div>
  );
}

export function PermissionClosedRow({
  request,
  onSelect,
  light = false,
}: {
  request: Approval;
  onSelect: () => void;
  light?: boolean;
}) {
  const [hover, setHover] = useState(false);
  const segmentBg = light ? "rgba(0,0,0,0.05)" : "rgba(255,255,255,0.06)";
  return (
    <button
      type="button"
      className="hud-closed-request"
      onClick={onSelect}
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
      style={{
        width: "100%",
        display: "flex",
        alignItems: "center",
        padding: "9px 10px",
        borderRadius: 8,
        border: 0,
        background: hover ? segmentBg : "transparent",
        color: "inherit",
        textAlign: "left",
        cursor: "pointer",
      }}
    >
      <PermissionRowHead request={request} open={false} light={light} />
    </button>
  );
}

/** "Waiting" over the requests that wait, with how many. */
export function WaitingHeader({
  count,
  padding,
  light,
  zh,
}: {
  count: number;
  padding: string;
  light: boolean;
  zh: boolean;
}) {
  const signalText = alertText("warning", useGround(light));
  const secondary = light ? "rgba(0,0,0,0.55)" : "rgba(255,255,255,0.55)";
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding,
      }}
    >
      <span style={{ fontSize: 11, fontWeight: 600, color: secondary }}>
        {zh ? "待审批" : "Waiting"}
      </span>
      <span
        style={{
          fontSize: 10,
          fontWeight: 600,
          fontVariantNumeric: "tabular-nums",
          color: signalText,
          padding: "2px 6px",
          borderRadius: 999,
          background: "rgba(233, 161, 109, 0.16)",
        }}
      >
        {count}
      </span>
    </div>
  );
}

export function PendingEventRows({
  queue,
  currentID,
  requests,
  select,
  includeRequests = false,
  light = false,
  zh = false,
}: {
  queue: IslandCard[];
  currentID: string;
  requests: Approval[];
  select: (id: string) => void;
  includeRequests?: boolean;
  light?: boolean;
  zh?: boolean;
}) {
  const waitingRequests = includeRequests
    ? requests.filter((req) => req.id !== currentID)
    : [];
  const otherCompletions = queue.filter(
    (card): card is Extract<IslandCard, { type: "news" }> =>
      card.type === "news" &&
      card.id !== currentID &&
      card.event.kind === "completion",
  );
  if (waitingRequests.length === 0 && otherCompletions.length === 0)
    return null;

  return (
    <div style={{ ...cardColumn(6), marginTop: 10 }}>
      {waitingRequests.length > 0 && (
        <div style={cardColumn(2)}>
          <WaitingHeader
            count={waitingRequests.length}
            padding="4px 4px 6px"
            light={light}
            zh={zh}
          />
          {waitingRequests.map((req) => (
            <PermissionClosedRow
              key={req.id}
              request={req}
              onSelect={() => select(req.id)}
              light={light}
            />
          ))}
        </div>
      )}
      {otherCompletions.length > 0 && (
        <div style={cardColumn(2)}>
          {otherCompletions.map((card) => (
            <CompletionClosedRow
              key={card.id}
              event={card.event}
              onSelect={() => select(card.id)}
              light={light}
              zh={zh}
            />
          ))}
        </div>
      )}
    </div>
  );
}
