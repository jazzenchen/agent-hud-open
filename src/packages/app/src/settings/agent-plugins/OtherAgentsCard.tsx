// The card at the bottom of Settings → Agents for agents Agent HUD doesn't
// know: the HUD tools settings to paste into one by hand.
import type { ReactNode } from "react";
import { Check } from "lucide-react";
import type { Translate } from "@agent-hud/ui/format";
import { Collapsible, Row } from "../SettingsPrimitives";
import { SmallButton } from "../SettingsControls";
import { useBriefly } from "./use-status";

/** HUD tools for any other agent: the settings to paste by hand. */
export function OtherAgentsCard({
  json,
  icon,
  t,
}: {
  json: unknown;
  icon: ReactNode;
  t: Translate;
}) {
  const [copied, setCopied] = useBriefly<true>(2000);
  return (
    <Collapsible icon={icon} title={t("Other agents")}>
      <Row
        title={t("HUD tools")}
        tag="MCP"
        subtitle={t("Paste into the agent's MCP settings.")}
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
              {t("Copied")}
            </>
          ) : (
            t("Copy settings")
          )}
        </SmallButton>
      </Row>
    </Collapsible>
  );
}
