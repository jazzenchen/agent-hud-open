// One folding card per client: its logo and name, a summary on the right, and
// its rows inside. A known client's card is in KnownClientCard.tsx; here are
// the card itself, a product client's card and the card for other agents.
import { Fragment, type ReactNode } from "react";
import { Check } from "lucide-react";
import { HUDArtwork } from "@agent-hud/hud/HUDArtwork";
import { Collapsible, Row, type Text } from "../SettingsPrimitives";
import { SmallButton } from "../SettingsControls";
import type { AgentPluginContext, AgentPluginItem } from "./types";
import { useBriefly } from "./use-status";

export function ClientCard({
  name,
  logo,
  light,
  missing,
  summary,
  defaultOpen,
  children,
}: {
  name: string;
  logo?: string;
  light: boolean;
  missing: boolean;
  summary: ReactNode;
  defaultOpen: boolean;
  children: ReactNode;
}) {
  const muted = { color: "var(--muted-foreground)" };
  return (
    <Collapsible
      icon={
        logo && (
          <span style={{ display: "inline-flex", opacity: missing ? 0.5 : 1 }}>
            <HUDArtwork source={logo} size={18} light={light} />
          </span>
        )
      }
      title={missing ? <span style={muted}>{name}</span> : name}
      summary={summary}
      defaultOpen={defaultOpen}
    >
      {children}
    </Collapsible>
  );
}

/** A card for a client only a product build knows, holding its rows. */
export function ProductClientCard({
  name,
  items,
  context,
  light,
}: {
  name: string;
  items: AgentPluginItem[];
  context: AgentPluginContext;
  light: boolean;
}) {
  return (
    <ClientCard
      name={name}
      light={light}
      missing={false}
      summary={null}
      defaultOpen={false}
    >
      {items.map((item) => (
        <Fragment key={item.id}>{item.render(context)}</Fragment>
      ))}
    </ClientCard>
  );
}

/** HUD tools for any other agent: the settings to paste by hand. */
export function OtherAgentsCard({
  json,
  icon,
  t,
}: {
  json: unknown;
  icon: ReactNode;
  t: Text;
}) {
  const [copied, setCopied] = useBriefly<true>(2000);
  return (
    <Collapsible icon={icon} title={t("Other agents", "其他智能体")}>
      <Row
        title={t("HUD tools", "HUD 工具")}
        tag="MCP"
        subtitle={t(
          "Paste into the agent's MCP settings.",
          "粘贴到智能体的 MCP 设置里。",
        )}
      >
        <SmallButton
          disabled={json == null}
          onClick={() =>
            void navigator.clipboard
              ?.writeText(JSON.stringify(json, null, 2))
              .then(() => setCopied(true))
              .catch(() => {})
          }
        >
          {copied ? (
            <>
              <Check size={13} aria-hidden />
              {t("Copied", "已复制")}
            </>
          ) : (
            t("Copy settings", "复制设置")
          )}
        </SmallButton>
      </Row>
    </Collapsible>
  );
}
