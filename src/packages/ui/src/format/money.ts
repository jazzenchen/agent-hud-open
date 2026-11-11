// Money
import { numberFormat } from "./intl.ts";
import type { Language } from "./language.ts";

type Amount = number | string;

function isCredits(code: string) {
  return code.toLowerCase() === "credits";
}

function currency(
  language: Language,
  code: string,
  digits: [number, number],
  rounding: "halfEven" | "halfExpand",
) {
  const locale = language === "zh" ? "zh-CN" : "en-US";
  if (isCredits(code)) {
    return numberFormat(locale, {
      minimumFractionDigits: 0,
      maximumFractionDigits: digits[1],
      roundingMode: rounding,
    });
  }
  return numberFormat(locale, {
    style: "currency",
    currency: code,
    currencyDisplay: "narrowSymbol",
    minimumFractionDigits: digits[0],
    maximumFractionDigits: digits[1],
    roundingMode: rounding,
  });
}

/** Formats a decimal string exactly; a number as its binary value. */
function formatAmount(format: Intl.NumberFormat, amount: Amount) {
  return format.format(amount as never);
}

const isPositive = (amount: Amount) => Number(amount) > 0;

/** A balance or an estimate with the currency's symbol: "$12.99", or up to six decimals for an estimate. */
export function moneyAmount(
  amount: Amount,
  code: string,
  language: Language,
  estimated = false,
) {
  const format = currency(language, code, [2, estimated ? 6 : 2], "halfEven");
  if (estimated && isPositive(amount) && Number(amount) < 0.000001) {
    return "<" + formatAmount(format, "0.000001");
  }
  return formatAmount(format, amount);
}

/** A list price as a pricing page writes it: whole units for a whole amount ("$20"), else two decimals ("$19.99"). */
export function listPrice(amount: number, code: string, language: Language) {
  const digits = Number.isInteger(amount) ? 0 : 2;
  return formatAmount(
    currency(language, code, [digits, digits], "halfEven"),
    amount,
  );
}

/** Whole units from a hundred up ("$412"), two decimals below ("$3.10"), "<$0.01" for a positive amount that rounds to none. */
export function moneyShort(amount: Amount, code: string, language: Language) {
  const cents = numberFormat("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
    roundingMode: "halfEven",
    useGrouping: false,
  }).format(amount as never);
  if (Number(cents) === 0 && isPositive(amount)) {
    return (
      "<" + formatAmount(currency(language, code, [2, 2], "halfEven"), "0.01")
    );
  }
  const digits = Math.abs(Number(cents)) >= 100 ? 0 : 2;
  return formatAmount(
    currency(language, code, [digits, digits], "halfEven"),
    cents,
  );
}

/**
 * What some usage would cost at list prices, each currency as it was priced: "≈$12.30 · ≈¥45.60", US dollars first,
 * then by code (Open's ModelCatalog.ListCost.text). Empty when nothing was priced.
 */
export function listCost(amounts: Record<string, Amount>, language: Language) {
  const order = (a: string, b: string) =>
    a === b ? 0 : a === "USD" ? -1 : b === "USD" ? 1 : a < b ? -1 : 1;
  return Object.keys(amounts)
    .sort(order)
    .map((code) => "≈" + moneyAmount(amounts[code], code, language))
    .join(" · ");
}

/** A price as the website writes it in both languages: "USD 2.99". */
export function moneyPrice(amount: Amount, code: string) {
  const negative = Number(amount) < 0;
  const absolute =
    typeof amount === "string" ? amount.replace(/^-/, "") : Math.abs(amount);
  const number = numberFormat("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
    roundingMode: "halfExpand",
  }).format(absolute as never);
  return `${negative ? "-" : ""}${code} ${number}`;
}
