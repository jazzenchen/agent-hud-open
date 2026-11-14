import type { ReactNode } from "react";
import { Button } from "@agent-hud/ui/components/button";
import { palette, tabular, type Period } from "./theme";
import { useT } from "@agent-hud/ui/format-context";

type Colors = ReturnType<typeof palette>;

/** The quota history's or the plan value's top line: its name, then what
 * stands at the right (periods, a note). */
export function AddedSectionHeading({
  title,
  colors,
  children,
}: {
  title: string;
  colors: Colors;
  children?: ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "baseline",
        minHeight: 16,
        gap: 8,
      }}
    >
      <strong style={{ fontSize: 13, fontWeight: 600, color: colors.text }}>
        {title}
      </strong>
      <span style={{ flex: 1 }} />
      {children}
    </div>
  );
}

/** The 24h / 7d / 30d buttons, as the token chart has them. */
export function PeriodButtons({
  period,
  setPeriod,
  colors,
}: {
  period: Period;
  setPeriod: (period: Period) => void;
  colors: Colors;
}) {
  const t = useT();
  return (
    <div style={{ display: "flex", gap: 2 }}>
      {(["24h", "7d", "30d"] as const).map((label) => (
        <Button
          key={label}
          size="xs"
          variant="ghost"
          aria-pressed={period === label}
          title={t("Last {{period}}", { period: label })}
          onClick={() => setPeriod(label)}
          style={{
            height: 18,
            minHeight: 18,
            minWidth: 24,
            padding: "3px 2px",
            fontSize: 9,
            fontWeight: 600,
            borderRadius: 5,
            lineHeight: "12px",
            background: period === label ? colors.selected : "transparent",
            color: period === label ? colors.text : colors.tertiary,
          }}
        >
          {label}
        </Button>
      ))}
    </div>
  );
}

/** A line of secondary words under a section's heading: loading, a failure,
 * or nothing to show. */
export function SectionNote({
  colors,
  children,
}: {
  colors: Colors;
  children: ReactNode;
}) {
  return (
    <div style={{ ...tabular, fontSize: 11, color: colors.secondary }}>
      {children}
    </div>
  );
}
