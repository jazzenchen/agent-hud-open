import { useState } from "react";
import {
  ArrowUpRight,
  ChartNoAxesColumn,
  MessageSquareText,
  RotateCcw,
  X,
} from "lucide-react";
import type { IslandEvent, Session, Snapshot } from "@agent-hud/protocol";
import { usePanelServices } from "../PanelContext";
import { clientDisplayName, compactDuration, newsTitle } from "../IslandNews";
import { ReminderMark } from "./ReminderMark";
import { dateFormat } from "@agent-hud/ui/format";
import { alertFill, alertInk, alertText, onAlertFill } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { CardButton } from "../permissions/CardButton";
import { useT } from "@agent-hud/ui/format-context";

export function CompletionAlertView({
  event,
  snapshot,
  dismiss,
  onStats,
  handoff,
  light = false,
  botLogos = true,
}: {
  event: IslandEvent;
  snapshot: Snapshot;
  dismiss: () => void;
  onStats?: () => void;
  handoff?: (callback?: () => void) => void;
  light?: boolean;
  botLogos?: boolean;
}) {
  const t = useT();
  const services = usePanelServices(),
    [failed, setFailed] = useState(false),
    [closeHover, setCloseHover] = useState(false);
  const session = snapshot.sessions.find((s) => s.id === event.sessionId);
  const hasSession = Boolean(
    session?.navigationTarget || event.navigationTarget,
  );
  const ground = useGround(light);
  const ink = (alpha: number) => alertInk(alpha, ground);
  const turnAccent = alertFill("turn", ground);
  const turnText = alertText("turn", ground);
  const onTurnAccent = onAlertFill(ground);

  const startedRaw = event.startedAt ?? session?.liveFacts?.turnStartedAt;
  const started = startedRaw ? Date.parse(startedRaw) : NaN;
  const ended = Date.parse(event.at);
  const message = event.message ?? session?.liveFacts?.message;

  const openUsage = () => {
    dismiss();
    const run = () => {
      if (session && services?.selectSession) services.selectSession(session);
      else onStats?.();
    };
    if (handoff) handoff(run);
    else run();
  };
  const openSession = async () => {
    if (!hasSession) return;
    const target: Session | undefined =
      session ??
      (event.sessionId
        ? {
            id: event.sessionId,
            title: event.title ?? "",
            source: event.source,
            model: event.model ?? "",
            cwd: null,
            updatedAt: event.at,
            state: "idle",
            tokens: 0,
            cost: null,
            contextPercent: null,
            logPath: null,
            parentId: null,
            navigationTarget: event.navigationTarget,
          }
        : undefined);
    if (!target) return;
    setFailed(false);
    const opened = await services?.openSession?.(target);
    if (opened === false) setFailed(true);
    else {
      dismiss();
      handoff?.();
    }
  };

  return (
    <article style={{ ...cardColumn(18), fontSize: 12, color: "inherit" }}>
      <header style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <ReminderMark
          event={event}
          size={24}
          light={light}
          botLogos={botLogos}
        />
        <div style={{ flex: 1, ...cardColumn(3), minWidth: 0 }}>
          <strong
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: ink(0.94),
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {clientDisplayName(event.source)}
          </strong>
          {event.model && event.model !== "Unknown" && (
            <span
              style={{
                fontSize: 11,
                color: ink(0.45),
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {event.model}
            </span>
          )}
        </div>
        <MessageSquareText size={12} strokeWidth={2.2} color={turnText} />
        <span style={{ color: turnText, fontSize: 12, fontWeight: 500 }}>
          {t("New reply")}
        </span>
        <button
          type="button"
          aria-label={t("Dismiss reply")}
          title={t("Dismiss reply")}
          onClick={dismiss}
          onPointerEnter={() => setCloseHover(true)}
          onPointerLeave={() => setCloseHover(false)}
          style={{
            width: 24,
            height: 24,
            display: "grid",
            placeItems: "center",
            border: 0,
            borderRadius: 6,
            background: closeHover ? ink(0.08) : "transparent",
            color: ink(closeHover ? 0.92 : 0.65),
            cursor: "pointer",
          }}
        >
          <X size={10} strokeWidth={2.4} />
        </button>
      </header>
      <div style={cardColumn(8)}>
        <strong
          style={{
            fontSize: 15,
            fontWeight: 500,
            color: ink(0.95),
            overflowWrap: "anywhere",
            display: "-webkit-box",
            WebkitLineClamp: 2,
            WebkitBoxOrient: "vertical",
            overflow: "hidden",
          }}
        >
          {event.title ?? newsTitle(event, t)}
        </strong>
        {message && (
          <p
            style={{
              margin: 0,
              fontSize: 12,
              lineHeight: "15px",
              color: ink(0.68),
              display: "-webkit-box",
              WebkitLineClamp: 4,
              WebkitBoxOrient: "vertical",
              overflow: "hidden",
              whiteSpace: "pre-wrap",
              overflowWrap: "anywhere",
            }}
          >
            {message}
          </p>
        )}
      </div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontSize: 11,
          fontVariantNumeric: "tabular-nums",
          color: ink(0.5),
        }}
      >
        <time>
          {dateFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(
            new Date(event.at),
          )}
        </time>
        {Number.isFinite(started) && ended > started && (
          <span>{compactDuration((ended - started) / 1000)}</span>
        )}
      </div>
      <div style={cardColumn(8)}>
        {failed && hasSession ? (
          <span style={{ fontSize: 11, color: ink(0.65) }}>
            {t("Couldn't return to session. Retry or view token usage.")}
          </span>
        ) : (
          !hasSession && (
            <span style={{ fontSize: 11, color: ink(0.5) }}>
              {t("Session return unavailable")}
            </span>
          )
        )}
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <CardButton
            onClick={openUsage}
            edge={ink(0.18)}
            color={ink(0.6)}
            style={{ whiteSpace: "nowrap" }}
          >
            <ChartNoAxesColumn size={9} strokeWidth={2.6} />
            {t("View token usage")}
          </CardButton>
          <span style={{ flex: 1, minWidth: 8 }} />
          {hasSession && (
            <CardButton
              onClick={openSession}
              fill={turnAccent}
              color={onTurnAccent}
              style={{ whiteSpace: "nowrap" }}
            >
              {failed ? (
                <RotateCcw size={9} strokeWidth={2.6} />
              ) : (
                <ArrowUpRight size={9} strokeWidth={2.6} />
              )}
              {failed ? t("Retry session") : t("Return to session")}
            </CardButton>
          )}
        </div>
      </div>
    </article>
  );
}
