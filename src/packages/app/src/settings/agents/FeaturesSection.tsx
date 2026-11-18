/* What Agent HUD does with the agents, one switch each for all of them: live
 * status, answering in the HUD, and (in the Agent tools section below it)
 * HUD tools and messages. Live status and answers need Agent HUD's hooks in
 * the agents; turning either on puts them in, turning both off takes them
 * out, so the hooks are never a switch of their own. */
import type { NativeMethod, Snapshot } from "@agent-hud/protocol";
import { Switch } from "@agent-hud/ui/components/switch";
import type { Translate } from "@agent-hud/ui/format";
import { Menu, SmallButton, ToggleRow, Unavailable } from "../SettingsControls";
import { Notice, Pill, Row, Section } from "../SettingsPrimitives";
import { usePerform } from "./use-perform";

export function FeaturesSection({
  t,
  snapshot,
  action,
}: {
  t: Translate;
  snapshot: Snapshot;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
}) {
  const { pending, error, perform } = usePerform();
  const settings = snapshot.settings;
  const live = settings.liveStatus !== false;
  const answer = settings.answerInHud !== false;
  const hooks = snapshot.connection.clientHooks;
  const set = (key: string, value: unknown) =>
    action("set_setting", { key, value });
  // Why the hooks can't go in here, when they can't.
  const blocked =
    hooks?.supported !== true
      ? t("This system doesn't support client hooks.")
      : hooks.installable !== true
        ? t("Needs Agent HUD at its approved installation location.")
        : null;
  const change = (key: "liveStatus" | "answerInHud", value: boolean) =>
    void perform(async () => {
      await set(key, value);
      const wanted =
        (key === "liveStatus" ? value : live) ||
        (key === "answerInHud" ? value : answer);
      if (!blocked && wanted !== settings.clientHooks)
        await set("clientHooks", wanted);
    });
  const hooksMissing = (live || answer) && !settings.clientHooks;

  return (
    <Section
      anchor="agent-features"
      title={t("What Agent HUD does")}
      footer={hooks?.notice ?? undefined}
    >
      <Row
        title={t("Sessions, tokens and costs")}
        subtitle={t("Always on. Read from the agents' own records.")}
      />
      <ToggleRow
        title={t("Live status")}
        subtitle={t(
          "Who's working and who's waiting, and a note when they're done.",
        )}
        checked={live}
        disabled={pending}
        change={(value) => change("liveStatus", value)}
      />
      <ToggleRow
        title={t("Answer in the HUD")}
        subtitle={t(
          "Approve requests and answer questions without switching apps.",
        )}
        checked={answer}
        disabled={pending}
        change={(value) => change("answerInHud", value)}
      />
      {answer && (
        <Row
          title={t("Wait for an answer")}
          subtitle={t("Then the request goes back to the client.")}
        >
          <Menu
            label={t("Wait for an answer")}
            value={String(settings.approvalWaitMinutes ?? 10)}
            options={[1, 3, 5, 10, 30, 60].map((minutes) => [
              String(minutes),
              t("{{count}} minutes", { count: minutes }),
            ])}
            disabled={pending}
            change={(value) =>
              void perform(() => set("approvalWaitMinutes", Number(value)))
            }
          />
        </Row>
      )}
      {answer && (
        // Off by default; each agent can be left out on its own page.
        <Row
          title={
            <>
              {t("Approve automatically")}{" "}
              <Pill tone="orange">{t("Caution")}</Pill>
            </>
          }
          subtitle={t(
            "Permission requests are allowed without asking. Questions and plans still wait for you.",
          )}
        >
          <Switch
            aria-label={t("Approve automatically")}
            checked={settings.autoApprove === true}
            disabled={pending}
            onCheckedChange={(value) =>
              void perform(() => set("autoApprove", value))
            }
          />
        </Row>
      )}
      {hooksMissing && (
        <Notice
          action={
            blocked ? (
              <Unavailable reason={blocked}>
                <SmallButton disabled>{t("Add hooks")}</SmallButton>
              </Unavailable>
            ) : (
              <SmallButton
                variant="default"
                disabled={pending}
                onClick={() => void perform(() => set("clientHooks", true))}
              >
                {t("Add hooks")}
              </SmallButton>
            )
          }
        >
          {t("Live status and answers need Agent HUD's hooks in your agents.")}
        </Notice>
      )}
      {error && <Notice tone="error">{error}</Notice>}
    </Section>
  );
}
