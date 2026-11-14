// Display formatting for tokens, money and time. Follows agent-hud-native-open's
// TokenFormat, MoneyFormat and Countdown; the shared fixtures in
// src/fixtures/parity check every function here, including the improvements
// recorded in src/fixtures/parity/decisions.json.

export {
  dateFormat,
  localeDateTime,
  numberFormat,
  systemTimeZone,
} from "./format/intl.ts";
export {
  addCatalog,
  hasCatalog,
  interfaceLanguage,
  isChinese,
  isLanguage,
  languageOfTag,
  languages,
  listSeparator,
  locale,
  resolveLanguage,
  snapshotLanguage,
  translate,
  translator,
  unspaced,
  type Language,
  type Translate,
} from "./format/language.ts";
export {
  percent,
  percent1,
  share,
  tokensInOut,
  tokensShort,
} from "./format/numbers.ts";
export {
  listCost,
  listPrice,
  moneyAmount,
  moneyPrice,
  moneyShort,
} from "./format/money.ts";
export {
  age,
  countdown,
  countdownCompact,
  countdownRough,
  forecast,
  waited,
} from "./format/durations.ts";
export {
  monthDayTime,
  relativeWeekdayTime,
  resetLabel,
  resetLabelCompact,
  systemCalendar,
  type FormatSettings,
  type WeekCalendar,
} from "./format/calendar.ts";
export {
  budgetText,
  byReset,
  outlookBasisText,
  outlookText,
  quotaDisplayLabel,
  quotaPercent,
  quotaPercentLabel,
  samplingHint,
  samplingLabel,
  type OutlookKind,
  type QuotaDisplay,
} from "./format/quota-text.ts";
