import { Card } from "@agent-hud/ui/components/card";
import type { Session, SessionDetail } from "@agent-hud/protocol";
import { Tween } from "../statistics/motion";
import {
  accountAt,
  integerAt,
  moneyAt,
  percentAt,
  tokensAt,
} from "./SessionMotion";
import {
  card,
  clipped,
  compact,
  integer,
  numeric,
  sessionMoney,
  type Theme,
  unpricedHelp,
} from "./SessionTheme";

/** The session's figures in a row: each counts up when first drawn and moves to new values, written as it is at rest. */
export function DetailFigures({
  session,
  detail,
  theme,
}: {
  session: Session;
  detail: SessionDetail | null;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const context =
    detail?.contextTokens ?? detail?.turns.at(-1)?.context ?? null;
  // Every share reads as a percent, so one from the list carries on into Core's.
  const percent =
    detail?.contextFill != null
      ? detail.contextFill * 100
      : context === null
        ? session.contextPercent
        : null;
  // The session with its sub-agents, as Core counts it on its row too.
  const totals = detail?.usage ?? session.usage ?? null;
  // A Project counts only what its workers ran on this Mac.
  const localOnly =
    (detail?.session.desktopProject?.localWorkerCount ??
      session.desktopProject?.localWorkerCount ??
      0) > 0;
  // A priced account's own estimate comes first, as Open's SessionMoney.
  const account = session.accountCost !== undefined;
  const money = sessionMoney(session);
  const unpricedModels = (detail?.models ?? [])
    .filter((model) => model.priced === false)
    .map((model) => model.name ?? model.id ?? "")
    .filter(Boolean);
  const credits = detail?.credits ?? null;
  const breakdown = detail?.turnsBy != null;
  const calls = detail?.callCount ?? 0;
  const items = [
    {
      title: zh ? "Token" : "Tokens",
      value: (
        <Tween value={totals?.total ?? session.tokens} format={tokensAt} />
      ),
      note: localOnly
        ? zh
          ? "仅本机执行会话"
          : "Local execution sessions only"
        : detail?.subagents
          ? (zh ? "含子 agent " : "Sub-agents ") +
            compact(detail.subagents.total)
          : totals
            ? zh
              ? `不含缓存读取 ${compact(totals.total - totals.cacheRead)}`
              : `Excl. cache reads ${compact(totals.total - totals.cacheRead)}`
            : zh
              ? "不含缓存读取 —"
              : "Excl. cache reads —",
      help: zh
        ? "五类合计，含缓存读取"
        : "All five kinds, cache reads included",
    },
    account
      ? {
          title: zh ? "费用" : "Cost",
          value: session.accountCost ? (
            <Tween
              value={session.accountCost.amount}
              format={accountAt(session.accountCost, zh)}
            />
          ) : (
            "—"
          ),
          note: zh ? "本会话" : "This session",
          help: zh
            ? "按账户价格估算的本会话费用"
            : "This session's cost at the account's prices",
        }
      : {
          title: zh ? "费用" : "Cost",
          value:
            money.value === null ? (
              "—"
            ) : (
              <Tween value={money.value} format={moneyAt(money, zh)} />
            ),
          note:
            money.value === null
              ? "—"
              : money.unpriced > 0
                ? zh
                  ? "部分 · 按 API 价"
                  : "Partial · at API prices"
                : zh
                  ? "按 API 价"
                  : "At API prices",
          help: [
            zh
              ? "同样的调用按厂商 API 公开价计算的费用，不是实际扣费"
              : "What the same calls cost at the vendor's API list price; not a charge",
            unpricedHelp(money.unpriced, zh, unpricedModels),
          ]
            .filter(Boolean)
            .join(zh ? "。" : ". "),
        },
    ...(credits
      ? [
          {
            title: zh ? "积分" : "Credits",
            value: credits.text,
            note: credits.unattributedText
              ? zh
                ? `未归因 ${credits.unattributedText}`
                : `Unattributed ${credits.unattributedText}`
              : zh
                ? "已按模型归因"
                : "Attributed to models",
            help: zh
              ? "WorkBuddy 自身计量记录的会话积分消耗；无法唯一确定模型的积分单独列为未归因"
              : "Credits WorkBuddy's own metering charged the session; the part no one model could be pinned on shows as unattributed",
          },
          ...(credits.used != null && credits.size != null
            ? [
                {
                  title: zh ? "计量上下文" : "Metered context",
                  value: <Tween value={credits.used} format={tokensAt} />,
                  note: (zh ? "共 " : "Of ") + compact(credits.size),
                  help: zh
                    ? "WorkBuddy 自身计量记录的上下文用量与上限"
                    : "The context usage and limit WorkBuddy's own metering recorded",
                },
              ]
            : []),
        ]
      : []),
    breakdown
      ? {
          title: zh ? "轮次" : "Turns",
          value: detail?.turnCount ? (
            <Tween value={detail.turnCount} format={integerAt} />
          ) : (
            "—"
          ),
          note: zh
            ? `${integer(calls)} 次调用`
            : calls === 1
              ? "1 call"
              : `${integer(calls)} calls`,
          help: zh ? "每个 prompt 开始一轮" : "Each prompt starts a turn",
        }
      : {
          title: zh ? "用量记录" : "Usage entries",
          value: detail ? (
            <Tween value={detail.turns.length} format={integerAt} />
          ) : (
            "—"
          ),
          note: detail
            ? zh
              ? `已加载 ${detail.calls.length} 次调用`
              : `${detail.calls.length} calls loaded`
            : "—",
          help: zh
            ? "已记录的用量变动条目。"
            : "Recorded usage changes; these are not assumed to be prompt-defined turns.",
        },
    {
      title: zh ? "上下文" : "Context",
      value:
        percent != null ? (
          <Tween key="percent" value={percent} format={percentAt} />
        ) : context !== null ? (
          <Tween key="tokens" value={context} format={tokensAt} />
        ) : (
          "—"
        ),
      note:
        detail?.contextWindow != null && detail.contextWindow > 0
          ? zh
            ? `共 ${compact(detail.contextWindow)}`
            : `Of ${compact(detail.contextWindow)}`
          : zh
            ? "最近一次调用"
            : "Latest call",
      help: zh
        ? "最近一次模型调用的输入：新输入、缓存写入与缓存读取"
        : "The latest model call's input: fresh, cache writes and cache reads",
      warn: detail?.contextFill != null && detail.contextFill >= 0.8,
    },
    {
      title: zh ? "缓存命中" : "Cache hits",
      value:
        detail?.cacheHitRate != null ? (
          <Tween value={detail.cacheHitRate * 100} format={percentAt} />
        ) : (
          "—"
        ),
      note: detail?.usage
        ? zh
          ? `${compact(detail.usage.cacheRead)} 读取 · ${compact(detail.usage.cacheWrite)} 写入`
          : `${compact(detail.usage.cacheRead)} read · ${compact(detail.usage.cacheWrite)} write`
        : zh
          ? "提示读自缓存"
          : "Of prompts, from cache",
      help: zh
        ? "缓存读取占全部提示的比例：新输入、缓存写入与缓存读取"
        : "Cache reads as a share of every prompt: fresh input, cache writes and cache reads",
    },
  ];
  return (
    <Card
      style={{
        ...card(theme, "10px 8px"),
        display: "grid",
        gridTemplateColumns: `repeat(${items.length},minmax(0,1fr))`,
        gap: 0,
      }}
    >
      {items.map((item, index) => (
        <div
          key={item.title}
          title={item.help}
          style={{
            padding: "0 7px",
            minWidth: 0,
            textAlign: "center",
            borderLeft: index > 0 ? `1px solid ${theme.divider}` : 0,
          }}
        >
          <div style={{ fontSize: 11, color: theme.secondary, ...clipped }}>
            {item.title}
          </div>
          <div
            style={{
              fontSize: 17,
              fontWeight: 600,
              margin: "4px 0",
              color: "warn" in item && item.warn ? theme.warning : undefined,
              ...numeric,
            }}
          >
            {item.value}
          </div>
          <div style={{ fontSize: 11, color: theme.secondary, ...clipped }}>
            {item.note}
          </div>
        </div>
      ))}
    </Card>
  );
}
