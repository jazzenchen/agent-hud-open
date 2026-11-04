import type {
  Approval,
  RequestDecision,
  RequestDelivery,
} from "@agent-hud/protocol";
import RequestActions from "../RequestActions";
import { cardColumn } from "../card-column";
import {
  PermissionClosedRow,
  PermissionRowHead,
  shortenHomePath,
  WaitingHeader,
} from "../IslandNews";
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

export function ApprovalContent({
  requests,
  selected,
  select,
  decide,
  openSettings,
  light,
  language,
}: {
  requests: Approval[];
  selected: Approval;
  select: (id: string) => void;
  decide?: (id: string, decision: RequestDecision) => void | Promise<void>;
  /** Settings, on a page such as "agent-plugins", with the HUD put away. */
  openSettings?: (page: string) => void;
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
              {contextPath && (
                <div
                  style={{
                    fontSize: 11,
                    fontVariantNumeric: "tabular-nums",
                    color: tertiary,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {contextPath}
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
