// "Let agents message each other" in Agent tools: agents message, start and
// read other Claude Code, Codex, Antigravity and DeepSeek Harness sessions on
// this computer through Agent HUD's MCP server. Off unless turned on. While
// on: "Send as me", "Ask for replies" (never, when the sending agent says a
// message needs one, or always), "Let agents decide when to steer", and
// how many passes in a row before the user takes a look. A message waits until
// its session is idle, unless it is steered into a Codex or DeepSeek Harness
// session's turn in progress: when the user asks an agent to, or, with
// "Let agents decide when to steer" on, when the agent corrects that turn's
// work on its own.
import { useState } from "react";
import { toast } from "sonner";
import type { Snapshot } from "@agent-hud/protocol";
import { Switch } from "@agent-hud/ui/components/switch";
import { isChinese, type Translate } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { LinkButton, Menu } from "../SettingsControls";
import { Pill, Row, type SettingsPaneProps } from "../SettingsPrimitives";
import { ToolSwitchRow } from "./rows";

const LIMITS = [3, 5, 10, 20, 50];

export function AgentMessagesRow({
  snapshot,
  action,
  t,
}: {
  snapshot: Snapshot;
  action: SettingsPaneProps["action"];
  t: Translate;
}) {
  const { language } = useFormat();
  const [pending, setPending] = useState(false);
  const settings = snapshot.settings;
  const enabled = settings.agentMessages === true;
  const asUser = settings.agentMessagesAsUser === true;
  const limit =
    settings.agentMessagesLimit === undefined
      ? 10
      : settings.agentMessagesLimit;
  const set = (key: string, value: unknown) => {
    setPending(true);
    // The rows have no place for a failure, so it shows as a toast.
    void action("set_setting", { key, value })
      .catch((failure) => toast.error(String(failure)))
      .finally(() => setPending(false));
  };
  const limits =
    limit !== null && !LIMITS.includes(limit) ? [...LIMITS, limit] : LIMITS;
  const asMe = t("Send as me");
  const askForReplies = t("Ask for replies");
  // Before the three choices, "Ask for replies only when needed" turned off
  // meant every labelled message asked.
  const replies =
    settings.agentMessagesReplies ??
    (settings.agentMessagesRequireReply === false ? "always" : "whenNeeded");
  const repliesSubtitle = {
    never: t("No message asks; answers stay in the other session."),
    whenNeeded: t("The sending agent decides for each message."),
    always: t("Every message but a reply asks."),
  }[replies];
  const steerOnOwn = t("Let agents decide when to steer");
  const steerCodex = t("Steer Codex through its hooks");
  // The hooks are Agent HUD's client hooks, so they go in only with those on.
  const clientHooks = settings.clientHooks === true;
  return (
    <>
      <ToolSwitchRow
        anchor="agent-messages"
        title={t("Let agents message each other")}
        description={t("Agents message, start and read other sessions here.")}
        checked={enabled}
        pending={pending}
        onChange={(value) => set("agentMessages", value)}
      >
        {/* "Send as me" carries a caution tag, which PluginControl has no
         * place for, so it is a Settings row of its own. */}
        <Row
          title={
            <>
              {asMe} <Pill tone="orange">{t("Caution")}</Pill>
            </>
          }
          subtitle={t("No agent label: others act on it as yours.")}
        >
          <Switch
            aria-label={asMe}
            checked={asUser}
            disabled={pending}
            onCheckedChange={(value) => set("agentMessagesAsUser", value)}
          />
        </Row>
        <Row title={askForReplies} subtitle={repliesSubtitle}>
          <Menu
            label={askForReplies}
            value={replies}
            disabled={pending}
            change={(value) => set("agentMessagesReplies", value)}
            options={[
              ["never", t("Don't ask")],
              ["whenNeeded", t("Only when needed")],
              ["always", t("Always ask")],
            ]}
          />
        </Row>
        {/* Off unless turned on: an agent then interrupts another session's
         * turn with no one asked, so it carries the caution tag too. */}
        <Row
          title={
            <>
              {steerOnOwn} <Pill tone="orange">{t("Caution")}</Pill>
            </>
          }
          subtitle={t(
            "Agents then interrupt each other's turns without asking you, to correct work in progress.",
          )}
        >
          <Switch
            aria-label={steerOnOwn}
            checked={settings.agentMessagesSteerOnOwn === true}
            disabled={pending}
            onCheckedChange={(value) => set("agentMessagesSteerOnOwn", value)}
          />
        </Row>
        {/* Only where the hooks were checked (macOS). */}
        {snapshot.connection.clientHooks?.steers?.includes("codex") && (
          <Row
            title={
              <>
                {steerCodex} <Pill tone="orange">{t("Caution")}</Pill>
              </>
            }
            subtitle={
              clientHooks
                ? t(
                    "Where the Codex app takes no steer. Allow its hooks once in Codex (Settings → Hooks), then quit and reopen it. While a turn runs, a small process waits beside it.",
                  )
                : t(
                    'Needs Agent HUD\'s hooks: turn on live status or answers under "{{section}}".',
                    { section: t("What Agent HUD does") },
                  )
            }
          >
            <Switch
              aria-label={steerCodex}
              checked={settings.codexHookSteer === true}
              disabled={pending || !clientHooks}
              onCheckedChange={(value) => set("codexHookSteer", value)}
            />
          </Row>
        )}
        <Row
          title={t("Passes in a row")}
          subtitle={t("Then they wait for you. Your next message resets it.")}
        >
          <Menu
            label={t("Passes in a row")}
            value={limit === null ? "none" : String(limit)}
            disabled={pending}
            width={88}
            change={(value) =>
              set("agentMessagesLimit", value === "none" ? null : Number(value))
            }
            options={[
              ...[...limits]
                .sort((a, b) => a - b)
                .map((each): [string, string] => [String(each), String(each)]),
              ["none", t("No limit")],
            ]}
          />
        </Row>
      </ToolSwitchRow>
      {/* Shown with the switch off too: the guide says how to turn it on. */}
      <Row title={t("How agent messages work")}>
        <LinkButton
          href={
            isChinese(language)
              ? "https://agenthud.app/docs/zh/mcp/messaging/"
              : "https://agenthud.app/docs/en/mcp/messaging/"
          }
        >
          {t("Open the guide")}
        </LinkButton>
      </Row>
    </>
  );
}
