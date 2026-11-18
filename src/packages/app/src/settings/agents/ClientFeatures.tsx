/* The features one agent can use, each turned off for it alone: live status
 * and its reminders, answers in the HUD (and the switches only some agents
 * have beneath it), and messages where its plugin carries them. A feature
 * turned off for every agent shows here off, with why. What Agent HUD puts
 * into the agent sits with them: its plugin first, HUD tools last. */
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
import { can, messagesKey } from "./features";
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
  const liveEverywhere = settings.liveStatus !== false;
  const live = preferences[client.id]?.live !== false;
  const reminders = preferences[client.id]?.reminders !== false;
  const answerEverywhere = settings.answerInHud !== false;
  const answerOff = settings.answerOff ?? [];
  const answer = !answerOff.includes(client.id);
  const autoEverywhere = settings.autoApprove === true;
  const autoOff = settings.autoApproveOff ?? [];
  const optIn = settings.optInApprovals ?? [];
  const messages = messagesKey(client);
  const why = (everywhereOn: boolean) =>
    !on ? offHere : !everywhereOn ? everywhere : undefined;
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
            checked={on && liveEverywhere && live}
            disabled={pending || !on || !liveEverywhere}
            reason={why(liveEverywhere)}
            change={(value) =>
              void perform(() => onPreferences(client.id, { live: value }))
            }
          />
          <ToggleRow
            title={t("Completion reminders")}
            checked={on && liveEverywhere && live && reminders}
            disabled={pending || !on || !liveEverywhere || !live}
            reason={
              why(liveEverywhere) ??
              (!live ? t("Needs Live status.") : undefined)
            }
            change={(value) =>
              void perform(() => onPreferences(client.id, { reminders: value }))
            }
          />
        </>
      )}
      {can(client, "answer") && (
        <ToggleRow
          title={t("Answer in the HUD")}
          checked={on && answerEverywhere && answer}
          disabled={pending || !on || !answerEverywhere}
          reason={why(answerEverywhere)}
          change={(value) =>
            set("answerOff", [
              ...answerOff.filter((id) => id !== client.id),
              ...(value ? [] : [client.id]),
            ])
          }
        />
      )}
      {can(client, "answer") && (
        <ToggleRow
          title={t("Approve automatically")}
          checked={
            on &&
            answerEverywhere &&
            answer &&
            autoEverywhere &&
            !autoOff.includes(client.id)
          }
          disabled={
            pending || !on || !answerEverywhere || !answer || !autoEverywhere
          }
          reason={
            !on
              ? offHere
              : !answerEverywhere || !answer
                ? t("Needs Answer in the HUD.")
                : !autoEverywhere
                  ? everywhere
                  : undefined
          }
          change={(value) =>
            set("autoApproveOff", [
              ...autoOff.filter((id) => id !== client.id),
              ...(value ? [] : [client.id]),
            ])
          }
        />
      )}
      {can(client, "answer") && client.agent === "claude" && (
        <ToggleRow
          title={t("Answer questions")}
          checked={
            on && answerEverywhere && answer && settings.claudeAnswers !== false
          }
          disabled={pending || !on || !answerEverywhere || !answer}
          reason={why(answerEverywhere)}
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
          disabled={pending || !on || !answerEverywhere || !answer}
          reason={why(answerEverywhere)}
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
