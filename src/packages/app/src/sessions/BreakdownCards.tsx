import { Card } from "@agent-hud/ui/components/card";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { share } from "@agent-hud/ui/format";
import type { Session, SessionDetail } from "@agent-hud/protocol";
import { presentKinds } from "../lib/sessions";
import { Tween, useTweens } from "../statistics/motion";
import { percentAt, tokensAt, useBarReach } from "./SessionMotion";
import {
  card,
  CardTitle,
  clipped,
  ColorKey,
  integer,
  kindColor,
  kindLabel,
  numeric,
  row,
  type Theme,
} from "./SessionTheme";

/** What the tokens were: a bar and each kind present with its share, the sub-agents' part noted. The bar fills and the figures count up when first drawn. */
export function KindsCard({
  detail,
  theme,
}: {
  detail: SessionDetail;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const usage = detail.usage;
  const kinds = presentKinds(usage);
  const total = usage?.total ?? 0;
  const reach = useBarReach();
  return (
    <Card style={{ ...card(theme), gap: 8 }}>
      <CardTitle
        title={zh ? "Token 类型" : "Token kinds"}
        right={zh ? "占比" : "Share"}
        theme={theme}
      />
      {total > 0 && (
        <div
          aria-hidden="true"
          style={{
            display: "flex",
            gap: 1.5,
            height: 6,
            overflow: "hidden",
            borderRadius: 3,
            clipPath: reach,
          }}
        >
          {kinds.map(({ kind, value }) => (
            <span
              key={kind}
              style={{
                flex: `${value / total} 1 0`,
                minWidth: 1,
                background: kindColor(theme, kind),
                transition: "flex-grow 240ms cubic-bezier(0.22, 1, 0.36, 1)",
              }}
            />
          ))}
        </div>
      )}
      <div>
        {kinds.map(({ kind, value }, index) => (
          <div
            key={kind}
            title={`${kindLabel(kind, zh)} ${integer(value)}`}
            style={{
              ...row,
              padding: "6px 0",
              borderTop: index > 0 ? `1px solid ${theme.divider}` : 0,
              fontSize: 12,
            }}
          >
            <ColorKey color={kindColor(theme, kind)}>
              {kindLabel(kind, zh)}
            </ColorKey>
            <strong style={{ marginLeft: "auto", fontWeight: 600, ...numeric }}>
              <Tween value={value} format={tokensAt} />
            </strong>
            <span
              style={{
                width: 36,
                textAlign: "right",
                color: theme.secondary,
                fontSize: 11,
                ...numeric,
              }}
            >
              <Tween value={value} format={(part) => share(part, total)} />
            </span>
          </div>
        ))}
        {kinds.length === 0 && (
          <span style={{ color: theme.secondary, fontSize: 11 }}>—</span>
        )}
      </div>
    </Card>
  );
}

/** Which models did the work: each with its share, cache hits and tokens, its bar filling when first drawn. */
export function ModelsCard({
  detail,
  session,
  theme,
}: {
  detail: SessionDetail;
  session: Session;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const models = detail.models ?? [];
  const tokensOf = (model: (typeof models)[number]) =>
    model.tokens.total ??
    model.tokens.input +
      model.tokens.output +
      model.tokens.cacheRead +
      model.tokens.cacheWrite +
      model.tokens.reasoning;
  const whole = models.reduce((sum, model) => sum + tokensOf(model), 0);
  const parts = useTweens(
    models.map((model) => (whole > 0 ? tokensOf(model) / whole : 0)),
  );
  return (
    <Card style={{ ...card(theme), gap: 8 }}>
      <CardTitle
        title={zh ? "模型" : "Models"}
        right={
          <>
            <span
              style={{ width: 64, textAlign: "right", whiteSpace: "nowrap" }}
            >
              {zh ? "缓存命中" : "Cache hits"}
            </span>
            <span
              style={{ width: 48, textAlign: "right", whiteSpace: "nowrap" }}
            >
              {zh ? "Token" : "Tokens"}
            </span>
          </>
        }
        theme={theme}
      />
      {models.map((model, index) => {
        const tokens = tokensOf(model);
        const name = model.name ?? model.id ?? session.model ?? "—";
        return (
          <div
            key={`${model.id ?? name}:${index}`}
            style={{ display: "grid", gap: 4 }}
          >
            <div style={{ ...row, fontSize: 12 }}>
              <HUDLogo
                source={model.provider ?? session.source}
                size={13}
                light={theme.light}
              />
              <span style={{ ...clipped, flex: 1 }}>{name}</span>
              <span
                style={{
                  width: 64,
                  textAlign: "right",
                  color: theme.secondary,
                  fontSize: 11,
                  ...numeric,
                }}
              >
                {model.cacheHitRate != null ? (
                  <Tween value={model.cacheHitRate * 100} format={percentAt} />
                ) : (
                  "—"
                )}
              </span>
              <strong
                style={{
                  width: 48,
                  textAlign: "right",
                  fontWeight: 600,
                  ...numeric,
                }}
              >
                <Tween value={tokens} format={tokensAt} />
              </strong>
            </div>
            <span
              aria-hidden="true"
              style={{
                display: "block",
                height: 4,
                borderRadius: 2,
                background: theme.track,
                overflow: "hidden",
              }}
            >
              <span
                style={{
                  display: "block",
                  height: "100%",
                  width: `${(parts[index] ?? 0) * 100}%`,
                  background: theme.input,
                  borderRadius: 2,
                }}
              />
            </span>
          </div>
        );
      })}
      {models.length === 0 && (
        <span style={{ color: theme.secondary, fontSize: 11 }}>—</span>
      )}
    </Card>
  );
}
