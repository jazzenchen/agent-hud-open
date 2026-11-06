import type {
  Approval,
  RequestDecision,
  RequestDelivery,
} from "@agent-hud/protocol";
import { useState } from "react";
import { ArrowUpRight, RotateCcw } from "lucide-react";
import RequestActions from "../RequestActions";
import { cardColumn } from "../card-column";
import { CardButton } from "./CardButton";
import {
  PermissionClosedRow,
  PermissionRowHead,
  WaitingHeader,
} from "../IslandNews";
import { shortenHomePath } from "../project-path";
import { alertText } from "../tokens";
import { useGround } from "../ground";

export function RequestDeliveryNotice({
  delivery,
  language,
  light = false,
}: {
  delivery: RequestDelivery | undefined;
  language: string;
  light?: boolean;
}) {
  // The warning's peach on an opaque ground, as before; its see-through
  // shade where the desktop shows through.
  const ground = useGround(light);
  const color = ground.seeThrough ? alertText("warning", ground) : "#d99a69";
  if (!delivery) return null;
  const zh = language.startsWith("zh");
  return (
    <p
      role={delivery.state === "failed" ? "alert" : "status"}
      style={{
        margin: "12px 0 0",
        paddingTop: 10,
        borderTop: "1px solid currentColor",
        borderColor: "#a7794a30",
        color,
        fontSize: 11,
        lineHeight: 1.5,
        overflowWrap: "anywhere",
      }}
    >
      <strong style={{ fontWeight: 550 }}>{delivery.source} · </strong>
      {delivery.state === "failed"
        ? zh
          ? "回答未能送达客户端。"
          : "The response could not be delivered to the client."
        : zh
          ? "尚未确认客户端收到回答，请在客户端核对。"
          : "Delivery is unconfirmed. Check the client."}
      {delivery.reason && <span> {delivery.reason}</span>}
    </p>
  );
}

/** The open request's way back to its session, as the reply card's. */
function BackToSession({
  open,
  light,
  zh,
}: {
  open: () => Promise<boolean>;
  light: boolean;
  zh: boolean;
}) {
  const [failed, setFailed] = useState(false);
  const ink = (alpha: number) =>
    light ? `rgba(0,0,0,${alpha})` : `rgba(255,255,255,${alpha})`;
  return (
    <CardButton
      onClick={async () => {
        setFailed(false);
        if (!(await open())) setFailed(true);
      }}
      title={
        failed
          ? zh
            ? "无法回到会话，请重试"
            : "Couldn't return to session. Try again."
          : undefined
      }
      edge={ink(0.18)}
      color={ink(0.6)}
      style={{ whiteSpace: "nowrap", flexShrink: 0 }}
    >
      {failed ? (
        <RotateCcw size={9} strokeWidth={2.6} />
      ) : (
        <ArrowUpRight size={9} strokeWidth={2.6} />
      )}
      {failed
        ? zh
          ? "重试会话跳转"
          : "Retry session"
        : zh
          ? "回到会话"
          : "Return to session"}
    </CardButton>
  );
}

export function ApprovalContent({
  requests,
  selected,
  select,
  decide,
  openSettings,
  backToSession,
  light,
  language,
}: {
  requests: Approval[];
  selected: Approval;
  select: (id: string) => void;
  decide?: (id: string, decision: RequestDecision) => void | Promise<void>;
  /** Settings, on a page such as "agent-plugins", with the HUD put away. */
  openSettings?: (page: string) => void;
  /** A request's way back to its session, false when it fails; none where
   * its session cannot be reached. */
  backToSession?: (request: Approval) => (() => Promise<boolean>) | undefined;
  light: boolean;
  language?: string;
}) {
  const zh = language?.startsWith("zh") ?? false;
  const tertiary = light ? "rgba(0,0,0,0.38)" : "rgba(255,255,255,0.38)";
  const cardSurface = light ? "rgba(0,0,0,0.035)" : "rgba(255,255,255,0.045)";

  return (
    <div className="hud-approval-content" style={cardColumn(0)}>
      <WaitingHeader
        count={requests.length}
        padding="0 4px 8px"
        light={light}
        zh={zh}
      />
      <div style={cardColumn(2)}>
        {requests.map((request) => {
          const contextPath = shortenHomePath(request.path ?? request.cwd);
          const back =
            request.id === selected.id ? backToSession?.(request) : undefined;
          return request.id === selected.id ? (
            <div
              key={request.id}
              className="hud-open-request"
              style={{
                ...cardColumn(9),
                borderRadius: 10,
                padding: 10,
                background: cardSurface,
              }}
            >
              <PermissionRowHead request={request} open light={light} />
              {(contextPath || back) && (
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    minWidth: 0,
                  }}
                >
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      fontSize: 11,
                      fontVariantNumeric: "tabular-nums",
                      color: tertiary,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {contextPath}
                  </span>
                  {back && <BackToSession open={back} light={light} zh={zh} />}
                </div>
              )}
              {/* A request whose questions change is a new card. */}
              <RequestActions
                key={`${request.id}:${JSON.stringify(request.questions ?? [])}`}
                request={request}
                light={light}
                language={language}
                onOpenSettings={openSettings}
                onDecision={
                  decide
                    ? (decision) => decide(request.id, decision)
                    : undefined
                }
              />
            </div>
          ) : (
            <PermissionClosedRow
              key={request.id}
              request={request}
              onSelect={() => select(request.id)}
              light={light}
            />
          );
        })}
      </div>
    </div>
  );
}
