// Client hooks: Agent HUD's hooks in each client's settings, which let the HUD
// answer permission requests and show finished turns, and how long the HUD
// holds a request before the client asks by itself.
import type { Snapshot } from "@agent-hud/protocol";
import { Switch } from "@agent-hud/ui/components/switch";
import type { Translate } from "@agent-hud/ui/format";
import { Menu, Unavailable } from "../SettingsControls";
import {
  Notice,
  Pill,
  Row,
  Section,
  type SettingsPaneProps,
} from "../SettingsPrimitives";
import { usePending } from "../use-pending";

export function HooksSection({
  snapshot,
  action,
  t,
}: {
  snapshot: Snapshot;
  action: SettingsPaneProps["action"];
  t: Translate;
}) {
  const { pending, error, run } = usePending();
  const status = snapshot.connection.clientHooks;
  const on = snapshot.settings.clientHooks;
  const configure = async (
    key: "clientHooks" | "approvalWaitMinutes",
    value: unknown,
  ) => {
    if (pending) return;
    await run(() => action("set_setting", { key, value }));
  };
  const reason =
    status?.supported !== true
      ? t("This system doesn't support client hooks.")
      : status.installable !== true
        ? t("Needs Agent HUD at its approved installation location.")
        : null;
  const toggle = (
    <Switch
      aria-label={t("Client hooks")}
      checked={on}
      disabled={pending || reason !== null}
      onCheckedChange={(value) => void configure("clientHooks", value)}
    />
  );
  return (
    <Section
      title={t("Hooks")}
      // A failed switch already says why in the notice below; the status
      // carries the same words, so it is not repeated as the footer.
      footer={error ? undefined : (status?.notice ?? undefined)}
    >
      <Row
        anchor="client-hooks"
        title={t("Client hooks")}
        subtitle={t("For permission requests and finished turns in the HUD.")}
      >
        {status?.supported &&
          (status.installed ? (
            <Pill tone="green" dot>
              {t("Installed")}
            </Pill>
          ) : (
            <Pill tone={on ? "orange" : "gray"} dot>
              {t("Not installed")}
            </Pill>
          ))}
        {reason && !pending ? (
          <Unavailable reason={reason}>{toggle}</Unavailable>
        ) : (
          toggle
        )}
      </Row>
      <Row
        title={t("Wait for an answer")}
        subtitle={t("Then the request goes back to the client.")}
      >
        <Menu
          label={t("Wait for an answer")}
          value={String(snapshot.settings.approvalWaitMinutes ?? 10)}
          options={[1, 3, 5, 10, 30, 60].map((minutes) => [
            String(minutes),
            t("{{count}} minutes", { count: minutes }),
          ])}
          disabled={pending || !on}
          reason={on ? undefined : t("Turn on client hooks first.")}
          change={(value) =>
            void configure("approvalWaitMinutes", Number(value))
          }
        />
      </Row>
      {error && <Notice tone="error">{error}</Notice>}
    </Section>
  );
}
