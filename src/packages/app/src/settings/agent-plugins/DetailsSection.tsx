// Details, folded at the bottom of the page: whether the pieces behind hooks
// and HUD tools are running, for when something doesn't work.
import type { Snapshot } from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { Collapsible, Pill, Row } from "../SettingsPrimitives";
import type { MCPClientsStatus } from "./services";
import type { HudTools } from "./use-status";

export function DetailsSection({
  snapshot,
  clients,
  tools,
  t,
}: {
  snapshot: Snapshot;
  clients: MCPClientsStatus | null;
  tools: HudTools;
  t: Translate;
}) {
  const hooks = snapshot.connection.clientHooks;
  const local = snapshot.connection.localTools;
  const name = (source: string) =>
    snapshot.providers.find((provider) => provider.id === source)?.name ??
    source;
  return (
    <Collapsible title={t("Details")}>
      <Row title={t("Hook receiver")}>
        {hooks?.enabled ? (
          <Pill tone="green" dot>
            {t("Listening")}
          </Pill>
        ) : (
          <Pill tone="gray" dot>
            {t("Off")}
          </Pill>
        )}
      </Row>
      {hooks?.clients?.map((client) => (
        <Row
          key={client.source}
          title={name(client.source)}
          subtitle={client.notice ?? undefined}
        >
          <Pill tone={client.installed ? "green" : "gray"} dot>
            {client.installed ? t("Hooks installed") : t("No hooks")}
          </Pill>
        </Row>
      ))}
      <Row title={t("HUD tools server")} subtitle={local?.notice ?? undefined}>
        {!local?.supported ? (
          <Pill tone="gray" dot>
            {t("Not supported")}
          </Pill>
        ) : !tools.on ? (
          <Pill tone="gray" dot>
            {t("Off")}
          </Pill>
        ) : tools.listening ? (
          <Pill tone="green" dot>
            {t("Listening")}
          </Pill>
        ) : (
          <Pill tone="red" dot>
            {t("Not running")}
          </Pill>
        )}
      </Row>
      <Row title={t("HUD tools setup")} subtitle={clients?.notice ?? undefined}>
        {clients === null ? (
          <Pill tone="gray" dot>
            {t("Checking…")}
          </Pill>
        ) : clients.ready ? (
          <Pill tone="green" dot>
            {t("Ready")}
          </Pill>
        ) : (
          <Pill tone="gray" dot>
            {t("Unavailable")}
          </Pill>
        )}
      </Row>
    </Collapsible>
  );
}
