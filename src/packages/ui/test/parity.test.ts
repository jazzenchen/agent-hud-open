// Runs the TypeScript-owned shared parity fixtures (src/fixtures/parity).
// agent-hud-native-open computed every expected value; decisions.json lists known
// gaps and intentional improvements. Run: pnpm run test:ts.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import * as format from "../src/format.ts";

type Args = Record<string, any>;
type Language = format.Language;

const root = join(import.meta.dirname, "../../../fixtures/parity");
const decisions = JSON.parse(
  readFileSync(join(root, "decisions.json"), "utf8"),
);
const calendar = (args: Args): format.WeekCalendar => ({
  timeZone: args.timeZone ?? "UTC",
  firstWeekday: args.firstWeekday ?? 1,
});
const outlookKinds: Record<string, format.OutlookKind> = {
  untimed: "untimed",
  exhausted: "exhausted",
  noEstimate: "no-estimate",
  noUsage: "no-usage",
  insufficientData: "insufficient-data",
  exhausts: "exhausts",
};
const instant = (value: string | null | undefined) =>
  value == null ? null : Date.parse(value);

const functions: Record<string, (args: Args, language: Language) => unknown> = {
  "TokenFormat.short": (a) => format.tokensShort(a.tokens),
  "TokenFormat.inOut": (a) => format.tokensInOut(a.input, a.output),
  "TokenFormat.percent": (a) => format.percent(a.value),
  "TokenFormat.percent1": (a) => format.percent1(a.value),
  "TokenFormat.share": (a) => format.share(a.part, a.whole),
  "Countdown.format": (a) => format.countdown(a.seconds),
  "Countdown.compact": (a) => format.countdownCompact(a.seconds),
  "Countdown.formatRough": (a) => format.countdownRough(a.seconds),
  "Countdown.age": (a, l) => format.age(a.seconds, l),
  "Countdown.waited": (a) => format.waited(a.seconds),
  "Countdown.forecast": (a, l) => format.forecast(a.seconds, l),
  "Countdown.resetLabel": (a, l) =>
    format.resetLabel(instant(a.date), instant(a.now)!, calendar(a), l),
  "Countdown.resetLabelCompact": (a, l) =>
    format.resetLabelCompact(instant(a.date), instant(a.now)!, calendar(a), l),
  "MoneyFormat.amount": (a, l) =>
    format.moneyAmount(a.amount, a.currency, l, a.estimated),
  "MoneyFormat.short": (a, l) => format.moneyShort(a.amount, a.currency, l),
  "MoneyFormat.price": (a) => format.moneyPrice(a.amount, a.currency),
  "QuotaForecast.text": (a, l) =>
    format.outlookText(
      {
        kind: outlookKinds[a.outlook],
        seconds: a.seconds,
        beforeReset: a.beforeReset,
      },
      a.projectedUsedAtReset ?? null,
      l,
    ),
  "QuotaForecast.byReset": (a, l) => format.byReset(a.projectedUsedPercent, l),
  "QuotaSampling.label": (a, l) => format.samplingLabel(a.afterReset, l),
  "QuotaSampling.hint": (a, l) =>
    format.samplingHint(a.elapsed, a.required, a.afterReset, l),
};

/** The value this app must produce, and whether the case is a recorded gap. */
function decision(key: string, language: Language, open: unknown) {
  const lookup = (table: Record<string, any>) =>
    table[`${key}/${language}`] ?? table[key];
  const improvement = lookup(decisions.improvements);
  if (improvement) {
    const next =
      typeof improvement.next === "object" &&
      improvement.next !== null &&
      !Array.isArray(improvement.next)
        ? improvement.next[language]
        : improvement.next;
    return { expected: next, gap: undefined, improved: true };
  }
  return {
    expected: open,
    gap: lookup(decisions.gaps) as string | undefined,
    improved: false,
  };
}

for (const file of readdirSync(join(root, "contracts"))
  .filter((name) => name.endsWith(".json"))
  .sort()) {
  const contract = JSON.parse(
    readFileSync(join(root, "contracts", file), "utf8"),
  );
  if (contract.owner !== "ts") continue;
  test(contract.contract, () => {
    const problems: string[] = [];
    for (const item of contract.cases) {
      const run = functions[item.fn];
      if (!run) {
        problems.push(`${item.name}: no TypeScript function for ${item.fn}`);
        continue;
      }
      for (const language of ["en", "zh"] as const) {
        const key = `${contract.contract}/${item.name}`;
        const open = item.expected[language];
        const { expected, gap, improved } = decision(key, language, open);
        const actual = run(item.args, language);
        const same = JSON.stringify(actual) === JSON.stringify(expected);
        if (improved && JSON.stringify(expected) === JSON.stringify(open)) {
          problems.push(
            `${key} [${language}]: the improvement equals Open's value; remove it from decisions.json`,
          );
        } else if (gap && same) {
          problems.push(
            `${key} [${language}]: now matches Open; remove the gap from decisions.json`,
          );
        } else if (!gap && !same) {
          problems.push(
            `${key} [${language}]: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
          );
        }
      }
    }
    assert.deepEqual(problems, []);
  });
}
