import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { moneyAmount, type Language } from "@agent-hud/ui/format";
import { useTween } from "../statistics/motion";
import { compact, integer, moneyText, type ListMoney } from "./SessionTheme";

/** The class that fades content in as it takes its grey shape's place. */
export const reveal = "stats-reveal";

/** A chart's label fades in once its bar has mostly risen. */
export const labelReveal = {
  className: reveal,
  style: { animationDelay: "160ms" },
};

/**
 * False while a part is first drawn, true from then on: what appears in it
 * later (a new session, a page loaded after) arrived later and fades in on
 * its own, where what came with the part fades in with it.
 */
export function useSettled() {
  const [settled, setSettled] = useState(false);
  useEffect(() => setSettled(true), []);
  return settled;
}

/** `children` in a box that fades in if it arrived `late`, as it was when the box was first drawn. */
export function Arrival({
  late,
  style,
  children,
}: {
  late: boolean;
  style?: CSSProperties;
  children: ReactNode;
}) {
  const [fresh] = useState(late);
  return (
    <div className={fresh ? reveal : undefined} style={style}>
      {children}
    </div>
  );
}

/** A bar's reach as it is first drawn: a clip that opens from the left. */
export function useBarReach() {
  const reach = useTween(1);
  return reach < 1 ? `inset(0 ${(1 - reach) * 100}% 0 0)` : undefined;
}

/* Figures as they move, written as they are at rest. */
export const tokensAt = (value: number) => compact(Math.round(value));
export const integerAt = (value: number) => integer(Math.round(value));
export const percentAt = (value: number) => `${Math.round(value)}%`;

/** A list-price cost as it moves: on the way up from nothing it reads $0.00, never "Free". */
export const moneyAt =
  (money: ListMoney, language: Language) => (value: number) =>
    moneyText(
      money.value
        ? { ...money, value: Math.max(value, Number.MIN_VALUE) }
        : money,
      language,
    );

/** An account's own estimate as it moves, with no more decimals than it has at rest. */
export const accountAt =
  (cost: { amount: number; currency: string }, language: Language) =>
  (value: number) => {
    const places = (String(cost.amount).split(".")[1] ?? "").length;
    const shown = Number(value.toFixed(Math.min(6, Math.max(2, places))));
    return `≈${moneyAmount(shown, cost.currency, language, true)}`;
  };
