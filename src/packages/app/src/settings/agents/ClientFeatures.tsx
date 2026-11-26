/* The features one agent can use, each turned on or off for it alone: live
 * status and its reminders, answers in the HUD (and the switches only some
 * agents have beneath it), and messages where its plugin carries them. Live
 * status, answers and approving automatically follow the switch for every
 * agent until they are set here, and what is set here wins over it. Messages
 * need the switch for every agent, since they go between agents. What Agent
 * HUD puts into the agent sits with them: its plugin first, HUD tools last. */
import type {
  ClientRow,
  NativeMethod,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import type { useClientInstalls } from "./ClientInstalls";
import { ToggleRow } from "../SettingsControls";
import { Pill, Row, Section } from "../SettingsPrimitives";
import { can, liveOn, messagesKey, switchFor, switchedOn } from "./features";
import type { Perform } from "./use-perform";

export function ClientFeatures({
  t,
  snapshot,
  client,
  action,
  preferences,
  onPreferences,
  perform,
  pending,
  installs,
}: {
  t: Translate;
  snapshot: Snapshot;
  client: ClientRow;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
  perform: Perform;
  pending: boolean;
  /** The agent's plugin, a product's rows and HUD tools. */
  installs: ReturnType<typeof useClientInstalls>;
}) {
  const settings = snapshot.settings;
  const set = (key: string, value: unknown) =>
    void perform(() => action("set_setting", { key, value }));
  const everywhere = t("Turned off for every agent.");
  const offHere = t("Turn on Enabled first.");
  const on = client.enabled;
  const live = liveOn(client, snapshot, preferences);
  const reminders = preferences[client.id]?.reminders !== false;
  const answer = switchedOn(settings, "answer", client.id);
  const auto = switchedOn(settings, "autoApprove", client.id);
  const optIn = settings.optInApprovals ?? [];
  const messages = messagesKey(client);
  const why = (everywhereOn: boolean) =>
    !on ? offHere : !everywhereOn ? everywhere : undefined;
  // Live status and answers need Agent HUD's hooks in the agents; turning
  // either on here puts them in, as the switch for every agent does.
  const hooks = snapshot.connection.clientHooks;
  const withHooks = async (value: boolean) => {
    if (
      value &&
      !settings.clientHooks &&
      hooks?.supported === true &&
      hooks.installable === true
    )
      await action("set_setting", { key: "clientHooks", value: true });
  };
  const switchHere = (feature: "answer" | "autoApprove", value: boolean) =>
    void perform(async () => {
      for (const [key, list] of switchFor(settings, feature, client.id, value))
        await action("set_setting", { key, value: list });
      if (feature === "answer") await withHooks(value);
    });
  if (
    !installs.plugin &&
    !["live", "answer", "talk", "tools"].some((f) => can(client, f as never))
  )
    return null;

  return (
    <Section title={t("What it uses")} footer={installs.note}>
      {installs.plugin}
      {can(client, "live") && (
        <>
          <ToggleRow
            title={t("Live status")}
            checked={on && live}
            disabled={pending || !on}
            reason={why(true)}
            change={(value) =>
              void perform(async () => {
                await onPreferences(client.id, { live: value });
                await withHooks(value);
              })
            }
          />
          <ToggleRow
            title={t("Completion reminders")}
            checked={on && live && reminders}
            disabled={pending || !on || !live}
            reason={why(true) ?? (!live ? t("Needs Live status.") : undefined)}
            change={(value) =>
              void perform(() => onPreferences(client.id, { reminders: value }))
            }
          />
        </>
      )}
      {can(client, "answer") && (
        <ToggleRow
          title={t("Answer in the HUD")}
          checked={on && answer}
          disabled={pending || !on}
          reason={why(true)}
          change={(value) => switchHere("answer", value)}
        />
      )}
      {can(client, "answer") && (
        <ToggleRow
          title={t("Approve automatically")}
          checked={on && answer && auto}
          disabled={pending || !on || !answer}
          reason={
            !on ? offHere : !answer ? t("Needs Answer in the HUD.") : undefined
          }
          change={(value) => switchHere("autoApprove", value)}
        />
      )}
      {can(client, "answer") && client.agent === "claude" && (
        <ToggleRow
          title={t("Answer questions")}
          checked={on && answer && settings.claudeAnswers !== false}
          disabled={pending || !on || !answer}
          reason={
            !on ? offHere : !answer ? t("Needs Answer in the HUD.") : undefined
          }
          change={(value) => set("claudeAnswers", value)}
        />
      )}
      {client.agent === "copilot" && (
        <ToggleRow
          title={t("Answer permission requests in the HUD")}
          subtitle={t(
            "Copilot then asks Agent HUD before its own rules and this session's approvals, so every tool call it would run or ask about waits for your answer in the HUD. Runs started with -p, --allow-all-tools, --allow-all or --yolo stay with Copilot.",
          )}
          checked={optIn.includes(client.agent)}
          disabled={pending || !on || !answer}
          reason={
            !on ? offHere : !answer ? t("Needs Answer in the HUD.") : undefined
          }
          change={(value) =>
            set("optInApprovals", [
              ...optIn.filter((id) => id !== client.agent),
              ...(value ? [client.agent] : []),
            ])
          }
        />
      )}
      {can(client, "talk") &&
        (messages ? (
          <ToggleRow
            title={t("Send messages")}
            checked={
              on &&
              settings.agentMessages === true &&
              settings[messages] !== false
            }
            disabled={pending || !on || settings.agentMessages !== true}
            reason={why(settings.agentMessages === true)}
            change={(value) => set(messages, value)}
          />
        ) : (
          <Row title={t("Messages")}>
            <Pill>{on && settings.agentMessages ? t("On") : t("Off")}</Pill>
          </Row>
        ))}
      {installs.own}
      {can(client, "tools") &&
        (installs.tools ?? (
          <Row title={t("HUD tools")}>
            <Pill>{on && settings.hudTools ? t("On") : t("Off")}</Pill>
          </Row>
        ))}
    </Section>
  );
}
