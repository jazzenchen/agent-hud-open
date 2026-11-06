import type { ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { Card } from "@agent-hud/ui/components/card";
import { Button } from "@agent-hud/ui/components/button";
import { dateFormat, localeDateTime } from "@agent-hud/ui/format";
import type { SessionDetail } from "@agent-hud/protocol";
import { MessagesPlaceholder } from "./SessionPlaceholders";
import { card, row, type Theme } from "./SessionTheme";

/** A folded section at the foot of the page. */
function Folded({
  title,
  theme,
  children,
}: {
  title: string;
  theme: Theme;
  children: ReactNode;
}) {
  return (
    <Card style={{ ...card(theme, "0") }}>
      <details className="session-folded">
        <summary
          style={{
            ...row,
            cursor: "pointer",
            padding: "10px 14px",
            fontSize: 13,
            fontWeight: 600,
            listStyle: "none",
          }}
        >
          <ChevronRight
            size={11}
            className="session-folded-chevron"
            style={{
              transition: "transform 180ms cubic-bezier(0.22, 1, 0.36, 1)",
            }}
          />
          {title}
        </summary>
        <div style={{ padding: "0 14px 12px", display: "grid", gap: 8 }}>
          {children}
        </div>
      </details>
    </Card>
  );
}

/** The foot of the session page, folded: the conversation, a page at a time, and the recorded calls as Core keeps them. */
export function SessionTranscript({
  detail,
  theme,
  loadMessages,
  loadingMessages,
}: {
  detail: SessionDetail;
  theme: Theme;
  loadMessages: () => void;
  loadingMessages: boolean;
}) {
  const zh = Boolean(theme.zh);
  return (
    <>
      {detail.messages && detail.messages.length > 0 && (
        <Folded
          title={
            zh
              ? `对话记录（${detail.messages.length}）`
              : `Conversation (${detail.messages.length})`
          }
          theme={theme}
        >
          {detail.messageNotice && (
            <p style={{ fontSize: 11, color: theme.secondary, margin: 0 }}>
              {detail.messageNotice}
            </p>
          )}
          {detail.messages.map((message) => (
            <section
              key={message.id}
              style={{
                borderTop: `1px solid ${theme.divider}`,
                paddingTop: 8,
              }}
            >
              <header
                style={{
                  fontSize: 11,
                  color: theme.secondary,
                  display: "flex",
                  gap: 8,
                }}
              >
                <strong>{message.role}</strong>
                {message.timestamp && (
                  <time>
                    {dateFormat(undefined, localeDateTime).format(
                      new Date(message.timestamp),
                    )}
                  </time>
                )}
              </header>
              <div
                style={{
                  fontSize: 12,
                  lineHeight: 1.5,
                  whiteSpace: "pre-wrap",
                  overflowWrap: "anywhere",
                  userSelect: "text",
                  paddingTop: 5,
                }}
              >
                {message.text}
              </div>
              {message.toolCall != null && (
                <details style={{ fontSize: 11, paddingTop: 5 }}>
                  <summary>
                    {zh
                      ? `工具调用 ${message.toolCallId ?? ""}`
                      : `Tool call ${message.toolCallId ?? ""}`}
                  </summary>
                  <pre
                    style={{
                      whiteSpace: "pre-wrap",
                      overflowWrap: "anywhere",
                    }}
                  >
                    {JSON.stringify(message.toolCall, null, 2)}
                  </pre>
                </details>
              )}
            </section>
          ))}
          {loadingMessages && <MessagesPlaceholder theme={theme} />}
          {detail.nextMessageCursor != null && (
            <Button
              variant="ghost"
              size="sm"
              disabled={loadingMessages}
              onClick={loadMessages}
            >
              {zh ? "加载更多消息" : "Load more messages"}
            </Button>
          )}
          {detail.messagesPending && (
            <p style={{ fontSize: 11, color: theme.secondary, margin: 0 }}>
              {zh
                ? "正在索引更多对话内容。"
                : "More transcript content is being indexed."}
            </p>
          )}
        </Folded>
      )}
      {detail.calls.length > 0 && (
        <Folded
          title={
            zh
              ? `已记录的调用（${detail.calls.length}）`
              : `Recorded calls (${detail.calls.length})`
          }
          theme={theme}
        >
          {detail.calls.map((call, index) => (
            <pre
              key={index}
              style={{
                fontFamily: "ui-monospace, monospace",
                fontSize: 10,
                color: theme.secondary,
                whiteSpace: "pre-wrap",
                overflowWrap: "anywhere",
                margin: 0,
                paddingTop: index > 0 ? 8 : 0,
                borderTop: index > 0 ? `1px solid ${theme.divider}` : 0,
              }}
            >
              {typeof call === "string" ? call : JSON.stringify(call, null, 2)}
            </pre>
          ))}
        </Folded>
      )}
    </>
  );
}
