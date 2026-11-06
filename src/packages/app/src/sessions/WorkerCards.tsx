import { ChevronRight } from "lucide-react";
import { Card } from "@agent-hud/ui/components/card";
import { share } from "@agent-hud/ui/format";
import type { SessionDetail } from "@agent-hud/protocol";
import { picked, ShareRows } from "./ShareRows";
import {
  card,
  CardTitle,
  clipped,
  compact,
  numeric,
  plain,
  row,
  type Theme,
} from "./SessionTheme";

/** How much the sub-agents added: the session's own part beside theirs, then each sub-agent as a labelled bar. */
export function SubagentsCard({
  detail,
  theme,
  onSession,
  selectedId = null,
}: {
  detail: SessionDetail;
  theme: Theme;
  onSession: (id: string) => void;
  /** The sub-agent open in the next column. */
  selectedId?: string | null;
}) {
  const zh = Boolean(theme.zh);
  const total = detail.usage?.total ?? detail.session.tokens;
  const theirs = detail.subagents?.total ?? 0;
  const own = detail.own?.total ?? total - theirs;
  const children = [...(detail.children ?? [])].sort(
    (a, b) => b.tokens - a.tokens,
  );
  const shown = children.slice(0, 6);
  const rest = children.slice(shown.length);
  const parts = [
    {
      key: "own",
      label: zh ? "会话本身" : "This session",
      value: own,
      color: theme.secondary,
      fade: 1,
    },
    {
      key: "sub",
      label: zh ? "子 agent" : "Sub-agents",
      value: theirs,
      color: theme.secondary,
      fade: 0.4,
    },
  ];
  return (
    <Card style={{ ...card(theme), gap: 9 }}>
      <CardTitle
        title={
          zh
            ? `子 agent（${children.length}）`
            : `Sub-agents (${children.length})`
        }
        right={zh ? "占会话 Token" : "Of the session's tokens"}
        theme={theme}
      />
      {theirs > 0 && (
        <div style={{ display: "grid", gap: 5 }}>
          <div
            aria-hidden="true"
            style={{
              display: "flex",
              gap: 1.5,
              height: 8,
              borderRadius: 4,
              overflow: "hidden",
            }}
          >
            {parts.map((part) => (
              <span
                key={part.key}
                style={{
                  flex: `${total > 0 ? part.value / total : 0} 1 0`,
                  minWidth: part.value > 0 ? 2 : 0,
                  background: part.color,
                  opacity: part.fade,
                  transition: "flex-grow 240ms cubic-bezier(0.22, 1, 0.36, 1)",
                }}
              />
            ))}
          </div>
          <div
            style={{ ...row, justifyContent: "space-between", fontSize: 11 }}
          >
            {parts.map((part) => (
              <span key={part.key} style={{ ...row, gap: 5 }}>
                <span style={{ color: theme.secondary }}>{part.label}</span>
                <strong style={{ fontWeight: 600, ...numeric }}>
                  {compact(part.value)}
                </strong>
                <span style={{ color: theme.secondary, ...numeric }}>
                  {share(part.value, total)}
                </span>
              </span>
            ))}
          </div>
        </div>
      )}
      {shown.length > 0 && (
        <div style={{ borderTop: `1px solid ${theme.divider}`, paddingTop: 8 }}>
          <ShareRows
            rows={[
              ...shown.map((child) => ({
                id: child.id,
                label: (
                  <span style={{ ...row, gap: 6 }}>
                    <span style={clipped}>
                      {child.title || child.modelName || child.model}
                    </span>
                    <span
                      style={{
                        color: theme.secondary,
                        fontSize: 11,
                        flexShrink: 0,
                      }}
                    >
                      {child.modelName || child.model}
                    </span>
                    <ChevronRight
                      size={10}
                      color={theme.tertiary}
                      style={{ flexShrink: 0 }}
                    />
                  </span>
                ),
                value: child.tokens,
                note: share(child.tokens, total),
                color: theme.secondary,
                title: child.title,
              })),
              ...(rest.length > 0
                ? [
                    {
                      id: "__rest__",
                      label: (
                        <span style={{ color: theme.secondary }}>
                          {zh
                            ? `其余 ${rest.length} 个`
                            : `${rest.length} more`}
                        </span>
                      ),
                      value: rest.reduce((sum, child) => sum + child.tokens, 0),
                      note: share(
                        rest.reduce((sum, child) => sum + child.tokens, 0),
                        total,
                      ),
                      color: theme.track,
                    },
                  ]
                : []),
            ]}
            whole={total}
            theme={theme}
            selected={selectedId}
            onSelect={(id) => {
              if (id !== "__rest__") onSession(id);
            }}
          />
        </div>
      )}
    </Card>
  );
}

/** The Project's local workers and their usage; the Project's own usage is not on this Mac. */
export function ProjectExecutionsCard({
  detail,
  theme,
  onSession,
  selectedId = null,
}: {
  detail: SessionDetail;
  theme: Theme;
  onSession: (id: string) => void;
  /** The worker open in the next column. */
  selectedId?: string | null;
}) {
  const zh = Boolean(theme.zh);
  const children = [...(detail.children ?? [])].sort(
    (a, b) => b.tokens - a.tokens,
  );
  const count =
    detail.session.desktopProject?.localWorkerCount ?? children.length;
  return (
    <Card style={{ ...card(theme), gap: 9 }}>
      <CardTitle
        title={
          zh
            ? `本机执行会话（${count}）`
            : `Local execution sessions (${count})`
        }
        right={zh ? "不含 Project 本身" : "Project itself not included"}
        theme={theme}
      />
      {children.map((child) => (
        <button
          key={child.id}
          type="button"
          onClick={() => onSession(child.id)}
          aria-current={child.id === selectedId || undefined}
          title={child.title}
          style={{
            ...plain,
            ...row,
            width: "100%",
            gap: 8,
            color: theme.text,
            textAlign: "left",
            minWidth: 0,
            ...(child.id === selectedId ? picked(theme) : {}),
          }}
        >
          <span style={{ ...clipped, flex: 1, minWidth: 0 }}>
            {child.title || (zh ? "本机执行会话" : "Local execution session")}
          </span>
          <span style={{ ...numeric, color: theme.secondary, flexShrink: 0 }}>
            {compact(child.tokens)} {zh ? "Token" : "tok"}
          </span>
          <ChevronRight
            size={10}
            color={theme.tertiary}
            style={{ flexShrink: 0 }}
          />
        </button>
      ))}
    </Card>
  );
}
