// Details, folded at the bottom of the page: whether the pieces behind hooks
// and HUD tools are running, for when something doesn't work.
import type { Snapshot } from "@agent-hud/protocol";
import { Collapsible, Pill, Row, type Text } from "../SettingsPrimitives";
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
  t: Text;
}) {
  const hooks = snapshot.connection.clientHooks;
  const local = snapshot.connection.localTools;
  const name = (source: string) =>
    snapshot.providers.find((provider) => provider.id === source)?.name ??
    source;
  return (
    <Collapsible title={t("Details", "详细信息")}>
      <Row title={t("Hook receiver", "回调接收器")}>
        {hooks?.enabled ? (
          <Pill tone="green" dot>
            {t("Listening", "正在监听")}
          </Pill>
        ) : (
          <Pill tone="gray" dot>
            {t("Off", "未运行")}
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
            {client.installed
              ? t("Hooks installed", "回调已安装")
              : t("No hooks", "未安装回调")}
          </Pill>
        </Row>
      ))}
      <Row
        title={t("HUD tools server", "HUD 工具服务")}
        subtitle={local?.notice ?? undefined}
      >
        {!local?.supported ? (
          <Pill tone="gray" dot>
            {t("Not supported", "不支持")}
          </Pill>
        ) : !tools.on ? (
          <Pill tone="gray" dot>
            {t("Off", "已关闭")}
          </Pill>
        ) : tools.listening ? (
          <Pill tone="green" dot>
            {t("Listening", "正在监听")}
          </Pill>
        ) : (
          <Pill tone="red" dot>
            {t("Not running", "未运行")}
          </Pill>
        )}
      </Row>
      <Row
        title={t("HUD tools setup", "HUD 工具安装")}
        subtitle={clients?.notice ?? undefined}
      >
        {clients === null ? (
          <Pill tone="gray" dot>
            {t("Checking…", "正在检查…")}
          </Pill>
        ) : clients.ready ? (
          <Pill tone="green" dot>
            {t("Ready", "可以使用")}
          </Pill>
        ) : (
          <Pill tone="gray" dot>
            {t("Unavailable", "不可用")}
          </Pill>
        )}
      </Row>
    </Collapsible>
  );
}
