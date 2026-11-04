// Tokens and percentages.
import { numberFormat } from "./intl.ts";

/**
 * The exact decimal value of a double. `Intl` rounds a number from its
 * shortest decimal form ("1.05"), printf from its binary value
 * (1.0500000000000000444…), so 1,050 tokens read 1.0k instead of 1.1k.
 */
function exactDecimal(value: number) {
  if (!Number.isFinite(value)) return String(value);
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  const bits = view.getBigUint64(0);
  const sign = bits >> 63n ? "-" : "";
  const biased = Number((bits >> 52n) & 0x7ffn);
  let mantissa = bits & ((1n << 52n) - 1n);
  if (biased !== 0) mantissa |= 1n << 52n;
  const exponent = (biased === 0 ? 1 : biased) - 1075;
  if (exponent >= 0) return sign + (mantissa << BigInt(exponent)).toString();
  const scale = -exponent;
  const digits = (mantissa * 5n ** BigInt(scale))
    .toString()
    .padStart(scale + 1, "0");
  return `${sign}${digits.slice(0, -scale)}.${digits.slice(-scale)}`;
}

/** printf's "%.Nf": the exact binary value, ties to even. */
function fixed(value: number, digits: number) {
  return numberFormat("en-US", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
    roundingMode: "halfEven",
    useGrouping: false,
  }).format(exactDecimal(value) as never);
}

/** One decimal below ten, none from ten up. */
function trim(value: number) {
  const one = fixed(value, 1);
  return value >= 10 || Number(one) >= 10 ? fixed(value, 0) : one;
}

const tokenUnits = [
  [1_000_000_000, "B"],
  [1_000_000, "M"],
  [1_000, "k"],
] as const;

/**
 * 48_000 → "48k", 1_260 → "1.3k", 950 → "950", 2_400_000 → "2.4M".
 * Better than Open: a value that rounds up to the next unit reads in it
 * ("1.0M", not "1000k"), and 9_999 reads "10k", not "10.0k".
 */
export function tokensShort(tokens: number) {
  for (let index = 0; index < tokenUnits.length; index++) {
    const [size, unit] = tokenUnits[index];
    if (tokens < size) continue;
    const value = trim(tokens / size);
    if (index > 0 && Number(value) >= 1000) {
      const [larger, largerUnit] = tokenUnits[index - 1];
      return trim(tokens / larger) + largerUnit;
    }
    return value + unit;
  }
  return String(Math.trunc(tokens));
}

/** "48k ↓ 12k ↑" */
export function tokensInOut(input: number, output: number) {
  return `${tokensShort(input)} ↓ ${tokensShort(output)} ↑`;
}

/** Half away from zero, as Swift's `rounded()`. */
export function roundHalfAway(value: number) {
  return Math.sign(value) * Math.round(Math.abs(value));
}

/** Percent with no decimals: 72.4 → "72%". */
export function percent(value: number) {
  return `${roundHalfAway(value)}%`;
}

/** Percent with one decimal: 6.2 → "6.2%". */
export function percent1(value: number) {
  return `${fixed(value, 1)}%`;
}

/** A part of a whole; a part that rounds to nothing but is not nothing reads "<1%". */
export function share(part: number, whole: number) {
  const value = (part / Math.max(1, whole)) * 100;
  return part > 0 && value < 0.5 ? "<1%" : percent(value);
}
