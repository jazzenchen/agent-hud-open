// What a quota row's measure says when it has no value, where Core gave no
// reason of its own; kept apart from the readings, as quota-alert-copy is.
import type { QuotaMetrics, QuotaMeasure } from "@agent-hud/protocol";
import {
  monthDayTime,
  relativeWeekdayTime,
  type FormatSettings,
  type Language,
} from "@agent-hud/ui/format";

export const say = (language: Language, zh: string, en: string) =>
  language === "zh" ? zh : en;

/** Why a measure has no value, where Core gave no reason of its own. */
export function missing(measure: QuotaMeasure, language: Language) {
  switch (measure) {
    case "quota":
      return say(
        language,
        "这个窗口暂无当前额度读数。",
        "No current quota reading is available for this window.",
      );
    case "burn":
      return say(
        language,
        "消耗速度需要本周期内跨越最短采样时长的读数。",
        "Burn rate needs readings spanning the minimum sampling interval in this quota cycle.",
      );
    case "tokens":
      return say(
        language,
        "输出速度需要这个窗口模型至少 3 次可计时的回复，合计至少 5 秒：按日志记下的用时，或同一日志中相隔不超过 120 秒的上一次调用算起。",
        "Reply rate needs at least 3 timed replies from this window's models, 5 seconds in all: each timed by its log, or from the same log's call before it within 120 seconds.",
      );
    case "consumed":
      return say(
        language,
        "已消耗需要来源自己的计数，或已知周期内对应模型的用量。",
        "Consumed needs the source's tally, or a known quota cycle with matching model usage.",
      );
    case "cost":
      return say(
        language,
        "这项估算需要已知的额度周期、对应模型的用量和这些模型的标价。",
        "This estimate needs a known quota cycle, matching model usage and list prices for its models.",
      );
  }
}

export function exhaustionHint(
  outlook: QuotaMetrics["outlook"] | undefined,
  sampling: QuotaMetrics["sampling"] | undefined,
  now: number,
  { language, calendar }: FormatSettings,
) {
  const at = outlook?.exhaustsAt ? Date.parse(outlook.exhaustsAt) : NaN;
  let time = say(language, "暂无预测", "Unavailable");
  if (outlook?.kind === "exhausted")
    time = say(language, "已耗尽", "Exhausted");
  else if (outlook?.kind === "no-usage")
    time = say(language, "暂无消耗", "No usage");
  else if (outlook?.kind === "insufficient-data" && sampling)
    time = say(language, "采样中", "Sampling");
  else if (outlook?.kind === "exhausts") {
    if (outlook.beforeReset === false)
      time = say(language, "重置前不会耗尽", "Not before reset");
    else if (Number.isFinite(at))
      time =
        at - now < 7 * 86_400_000
          ? relativeWeekdayTime(at, now, calendar, language)
          : monthDayTime(at, calendar, language);
  }
  return say(language, `预计耗尽：${time}`, `Estimated exhaustion: ${time}`);
}
