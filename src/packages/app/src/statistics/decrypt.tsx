import { useEffect, useRef, useState, type CSSProperties } from "react";

const DIGITS = "0123456789";
/** How often the cycling digits change, ms. */
const TICK_MS = 50;
/** How long an arriving figure takes to lock in, left to right, ms. */
const SETTLE_MS = 560;

const still = () =>
  typeof matchMedia === "function" &&
  matchMedia("(prefers-reduced-motion: reduce)").matches;
const digit = () => DIGITS[Math.floor(Math.random() * DIGITS.length)];

/** A figure that decrypts into place. While `text` is null it shows `width`
 * cycling digits in `waiting`'s colour; when the text arrives its digits
 * lock in from left to right over about half a second, its signs, letters
 * and separators standing from the start. Later changes show at once: the
 * figure's own counting moves them. Text already there on the first draw
 * shows at once, and so does everything with Reduce Motion. */
export function Decrypt({
  text,
  width = 5,
  waiting,
  style,
}: {
  text: string | null;
  width?: number;
  /** The cycling digits' colour. */
  waiting: string;
  style?: CSSProperties;
}) {
  const [, setTick] = useState(0);
  const ready = text !== null;
  const shownAtFirst = useRef(ready);
  const [arrivedAt, setArrivedAt] = useState<number | null>(null);
  useEffect(() => {
    if (ready && arrivedAt === null)
      setArrivedAt(shownAtFirst.current || still() ? -1 : performance.now());
  }, [ready, arrivedAt]);
  const elapsed =
    arrivedAt !== null && arrivedAt >= 0 ? performance.now() - arrivedAt : null;
  const moving =
    !still() && (!ready || (elapsed !== null && elapsed < SETTLE_MS));
  useEffect(() => {
    if (!moving) return;
    const timer = window.setInterval(() => setTick((n) => n + 1), TICK_MS);
    return () => window.clearInterval(timer);
  }, [moving]);
  if (!ready)
    return (
      <span aria-hidden style={{ ...style, color: waiting }}>
        {still() ? "·".repeat(width) : Array.from({ length: width }, digit)}
      </span>
    );
  if (!moving || elapsed === null) return <span style={style}>{text}</span>;
  const chars = [...text];
  return (
    <span style={style} aria-label={text}>
      {chars.map((char, index) =>
        /\d/.test(char) && elapsed < ((index + 1) / chars.length) * SETTLE_MS
          ? digit()
          : char,
      )}
    </span>
  );
}
