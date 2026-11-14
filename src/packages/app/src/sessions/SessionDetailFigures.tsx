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
  numeric,
  sessionMoney,
  type Theme,
  unpricedHelp,
} from "./SessionTheme";
import { useFormat, useT } from "@agent-hud/ui/format-context";

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
  const t = useT();
  const { language } = useFormat();
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
  const unpriced = unpricedHelp(money.unpriced, language, unpricedModels);
  const listPriceHelp = t(
    "What the same calls cost at the vendor's API list price; not a charge",
  );
  const items = [
    {
      title: t("Tokens"),
      value: (
        <Tween value={totals?.total ?? session.tokens} format={tokensAt} />
      ),
      note: localOnly
        ? t("Local execution sessions only")
        : detail?.subagents
          ? t("Sub-agents {{tokens}}", {
              tokens: compact(detail.subagents.total),
            })
          : totals
            ? t("Excl. cache reads {{tokens}}", {
                tokens: compact(totals.total - totals.cacheRead),
              })
            : t("Excl. cache reads —"),
      help: t("All five kinds, cache reads included"),
    },
    account
      ? {
          title: t("Cost"),
          value: session.accountCost ? (
            <Tween
              value={session.accountCost.amount}
              format={accountAt(session.accountCost, language)}
            />
          ) : (
            "—"
          ),
          note: t("This session"),
          help: t("This session's cost at the account's prices"),
        }
      : {
          title: t("Cost"),
          value:
            money.value === null ? (
              "—"
            ) : (
              <Tween value={money.value} format={moneyAt(money, language)} />
            ),
          note:
            money.value === null
              ? "—"
              : money.unpriced > 0
                ? t("Partial · at API prices")
                : t("At API prices"),
          help: unpriced
            ? t("{{listPrice}}. {{unpriced}}", {
                listPrice: listPriceHelp,
                unpriced,
              })
            : listPriceHelp,
        },
    ...(credits
      ? [
          {
            title: t("Credits"),
            value: credits.text,
            note: credits.unattributedText
              ? t("Unattributed {{credits}}", {
                  credits: credits.unattributedText,
                })
              : t("Attributed to models"),
            help: t(
              "Credits WorkBuddy's own metering charged the session; the part no one model could be pinned on shows as unattributed",
            ),
          },
          ...(credits.used != null && credits.size != null
            ? [
                {
                  title: t("Metered context"),
                  value: <Tween value={credits.used} format={tokensAt} />,
                  note: t("Of {{tokens}}", { tokens: compact(credits.size) }),
                  help: t(
                    "The context usage and limit WorkBuddy's own metering recorded",
                  ),
                },
              ]
            : []),
        ]
      : []),
    breakdown
      ? {
          title: t("Turns"),
          value: detail?.turnCount ? (
            <Tween value={detail.turnCount} format={integerAt} />
          ) : (
            "—"
          ),
          note: t("{{count}} calls", { count: calls }),
          help: t("Each prompt starts a turn"),
        }
      : {
          title: t("Usage entries"),
          value: detail ? (
            <Tween value={detail.turns.length} format={integerAt} />
          ) : (
            "—"
          ),
          note: detail
            ? t("{{count}} calls loaded", { count: detail.calls.length })
            : "—",
          help: t(
            "Recorded usage changes; these are not assumed to be prompt-defined turns.",
          ),
        },
    {
      title: t("Context"),
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
          ? t("Of {{tokens}}", { tokens: compact(detail.contextWindow) })
          : t("Latest call"),
      help: t(
        "The latest model call's input: fresh, cache writes and cache reads",
      ),
      warn: detail?.contextFill != null && detail.contextFill >= 0.8,
    },
    {
      title: t("Cache hits"),
      value:
        detail?.cacheHitRate != null ? (
          <Tween value={detail.cacheHitRate * 100} format={percentAt} />
        ) : (
          "—"
        ),
      note: detail?.usage
        ? t("{{read}} read · {{write}} write", {
            read: compact(detail.usage.cacheRead),
            write: compact(detail.usage.cacheWrite),
          })
        : t("Of prompts, from cache"),
      help: t(
        "Cache reads as a share of every prompt: fresh input, cache writes and cache reads",
      ),
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
