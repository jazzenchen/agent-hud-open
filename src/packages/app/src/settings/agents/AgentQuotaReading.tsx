/* How far Agent HUD may go to read one agent's quota (`quotaReading`). The
 * levels each agent offers follow what its reader can do without the next
 * one: Cursor's quota exists only behind its sign-in, so it is read in full
 * or not at all, as is every agent whose reader does not sort its sources. */
import type {
  NativeMethod,
  QuotaReadingLevel,
  Snapshot,
} from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { Menu } from "../SettingsControls";
import { Row, Section } from "../SettingsPrimitives";
import type { Perform } from "./use-perform";

const GRADED = new Set(["claude", "codex", "antigravity"]);

export function AgentQuotaReading({
  t,
  snapshot,
  agent,
  action,
  perform,
  pending,
}: {
  t: Translate;
  snapshot: Snapshot;
  agent: string;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  perform: Perform;
  pending: boolean;
}) {
  const levels = snapshot.settings.quotaReading ?? {};
  const value: QuotaReadingLevel = levels[agent] ?? "full";
  const names: Record<QuotaReadingLevel, string> = {
    off: t("Off"),
    official: t("The agent's own app"),
    full: t("Everything (default)"),
  };
  const offered: QuotaReadingLevel[] = GRADED.has(agent)
    ? ["off", "official", "full"]
    : ["off", "full"];
  const explained: Record<QuotaReadingLevel, string> = {
    off: t(
      "Quota is not read. Sessions and tokens are still counted from the agent's logs.",
    ),
    official: t(
      "Also asks the agent's own app, which reads the account the way it always does.",
    ),
    full: t(
      "Also uses sign-ins other apps keep and the vendor's web service, as before.",
    ),
  };
  return (
    <Section title={t("Quota reading")} footer={explained[value]}>
      <Row
        title={t("Read quota from")}
        subtitle={t("Lower levels use fewer sign-ins and send less.")}
      >
        <Menu
          label={t("Read quota from")}
          value={value}
          options={offered.map((level) => [level, names[level]])}
          disabled={pending}
          width="auto"
          change={(level) =>
            void perform(() =>
              action("set_setting", {
                key: "quotaReading",
                value: { ...levels, [agent]: level },
              }),
            )
          }
        />
      </Row>
    </Section>
  );
}
