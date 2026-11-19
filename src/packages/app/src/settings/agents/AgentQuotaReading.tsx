/* How far Agent HUD may go to read one agent's quota (`quotaReading`). Core
 * says which levels read differently for the agent and which one it reads
 * at (agents `quota_reading::choices`); an agent offered none shows nothing. */
import type {
  NativeMethod,
  Provider,
  QuotaReadingLevel,
  Snapshot,
} from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { Menu } from "../SettingsControls";
import { Row, Section } from "../SettingsPrimitives";
import type { Perform } from "./use-perform";

export function AgentQuotaReading({
  t,
  snapshot,
  source,
  action,
  perform,
  pending,
}: {
  t: Translate;
  snapshot: Snapshot;
  source: Provider;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  perform: Perform;
  pending: boolean;
}) {
  const reading = source.quotaReading;
  if (!reading?.choices.length) return null;
  const levels = snapshot.settings.quotaReading ?? {};
  const value = reading.level;
  const names: Record<QuotaReadingLevel, string> = {
    off: t("Off"),
    official: t("Own app only"),
    full: t("Everything"),
  };
  // What the chosen level reads, under the row.
  const reads: Record<QuotaReadingLevel, string> = {
    off: t(
      "Shows no quota. Sessions and tokens are still counted from the agent's own logs.",
    ),
    official: t("Uses only the agent's own app, not sign-ins other apps keep."),
    full: t(
      "Uses any sign-in or key found for this agent, including ones other apps keep.",
    ),
  };
  return (
    <Section>
      <Row title={t("Quota reading")} subtitle={reads[value]}>
        <Menu
          label={t("Quota reading")}
          value={value}
          options={reading.choices.map((level) => [level, names[level]])}
          disabled={pending}
          width="auto"
          change={(level) =>
            void perform(() =>
              action("set_setting", {
                key: "quotaReading",
                value: { ...levels, [source.id]: level },
              }),
            )
          }
        />
      </Row>
    </Section>
  );
}
