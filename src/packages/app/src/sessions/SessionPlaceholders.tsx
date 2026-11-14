import type { CSSProperties } from "react";
import { useT } from "@agent-hud/ui/format-context";
import { Card } from "@agent-hud/ui/components/card";
import { Bar, ChartPlaceholder } from "../statistics/dashboard/Placeholder";
import { useRowGrid } from "./SessionListSection";
import { card, type Theme } from "./SessionTheme";

const lines: CSSProperties = { display: "grid", gap: 8, minWidth: 0 };

/** Session rows still loading, in the rows' own columns: a title and two
 * lines, a sparkline, three figures. */
export function RowsPlaceholder({
  count = 5,
  theme,
}: {
  count?: number;
  theme: Theme;
}) {
  const grid = useRowGrid();
  return (
    <Card
      aria-hidden
      style={{ ...card(theme, "0"), overflow: "hidden", gap: 0 }}
    >
      {Array.from({ length: count }, (_, index) => (
        <div
          key={index}
          style={{
            padding: "12px 14px 11px",
            display: "grid",
            gridTemplateColumns: grid.columns,
            columnGap: grid.gap,
            alignItems: "center",
            borderTop: index > 0 ? `1px solid ${theme.divider}` : 0,
          }}
        >
          <span style={lines}>
            <Bar
              width={`${72 - (index % 3) * 14}%`}
              height={13}
              theme={theme}
            />
            <Bar width="58%" height={10} theme={theme} />
            <Bar width="34%" height={10} theme={theme} />
          </span>
          <ChartPlaceholder height={34} columns={18} theme={theme} />
          <span style={{ ...lines, justifyItems: "end" }}>
            <Bar width={52} height={13} theme={theme} />
            <Bar width={40} height={10} theme={theme} />
            <Bar width={84} height={10} theme={theme} />
          </span>
        </div>
      ))}
    </Card>
  );
}

/** The list before its first page: today's card (by day), a day's header
 * and its rows. */
export function ListPlaceholder({
  today,
  theme,
}: {
  today: boolean;
  theme: Theme;
}) {
  const t = useT();
  return (
    <div role="status" aria-label={t("Loading sessions")}>
      {today && <TodayPlaceholder theme={theme} />}
      <div style={{ padding: "4px 2px 10px" }}>
        <Bar width={96} height={12} theme={theme} />
      </div>
      <RowsPlaceholder count={6} theme={theme} />
    </div>
  );
}

/** Today's card still loading: its count, total, kinds and where the
 * tokens went. */
function TodayPlaceholder({ theme }: { theme: Theme }) {
  return (
    <Card
      aria-hidden
      style={{
        ...card(theme, "12px 14px"),
        marginBottom: 18,
        display: "grid",
        gridTemplateColumns: "repeat(auto-fit, minmax(290px, 1fr))",
        columnGap: 0,
        rowGap: 14,
      }}
    >
      <div style={{ ...lines, gap: 10, paddingRight: 20 }}>
        <Bar width={150} height={13} theme={theme} />
        <Bar width={110} height={26} theme={theme} />
        <Bar height={6} radius={3} theme={theme} />
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
            columnGap: 20,
            rowGap: 8,
          }}
        >
          {[0, 1, 2, 3, 4, 5].map((index) => (
            <Bar key={index} height={10} theme={theme} />
          ))}
        </div>
      </div>
      <div
        style={{
          ...lines,
          gap: 11,
          paddingLeft: 20,
          borderLeft: `1px solid ${theme.divider}`,
        }}
      >
        <Bar width={170} height={13} theme={theme} />
        {[0, 1, 2, 3, 4].map((index) => (
          <span key={index} style={{ ...lines, gap: 5 }}>
            <Bar width={`${80 - index * 9}%`} height={10} theme={theme} />
            <Bar width={`${64 - index * 11}%`} height={4} theme={theme} />
          </span>
        ))}
      </div>
    </Card>
  );
}

/** The figures row still loading: five cells of a caption, a value and a
 * note. */
function FiguresPlaceholder({ theme }: { theme: Theme }) {
  return (
    <Card
      aria-hidden
      style={{
        ...card(theme, "10px 8px"),
        display: "grid",
        gridTemplateColumns: "repeat(5, minmax(0, 1fr))",
        gap: 0,
      }}
    >
      {[0, 1, 2, 3, 4].map((index) => (
        <div
          key={index}
          style={{
            ...lines,
            gap: 7,
            justifyItems: "center",
            padding: "1px 7px",
            borderLeft: index > 0 ? `1px solid ${theme.divider}` : 0,
          }}
        >
          <Bar width="50%" height={10} theme={theme} />
          <Bar width="64%" height={18} theme={theme} />
          <Bar width="72%" height={10} theme={theme} />
        </div>
      ))}
    </Card>
  );
}

/** What a session's usage draws, still loading: the turn chart, then the
 * breakdown cards. */
export function UsagePlaceholder({ theme }: { theme: Theme }) {
  const t = useT();
  return (
    <div
      role="status"
      aria-label={t("Reading session usage")}
      style={{ display: "grid", gap: 12 }}
    >
      <Card aria-hidden style={{ ...card(theme, "12px 14px"), gap: 12 }}>
        <Bar width={130} height={13} theme={theme} />
        <ChartPlaceholder height={150} columns={36} theme={theme} />
      </Card>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))",
          gap: 12,
        }}
      >
        {[0, 1].map((index) => (
          <Card key={index} aria-hidden style={{ ...card(theme), gap: 10 }}>
            <Bar width={110} height={13} theme={theme} />
            <Bar height={8} theme={theme} />
            {[0, 1, 2].map((line) => (
              <Bar
                key={line}
                width={`${86 - line * 18}%`}
                height={10}
                theme={theme}
              />
            ))}
          </Card>
        ))}
      </div>
    </div>
  );
}

/** A session's page before anything of it is known: its header, figures,
 * chart and cards. */
export function DetailPlaceholder({ theme }: { theme: Theme }) {
  return (
    <div style={{ display: "grid", gap: 12 }}>
      <div aria-hidden style={{ ...lines, padding: "4px 2px 2px" }}>
        <Bar width="58%" height={20} theme={theme} />
        <Bar width="82%" height={12} theme={theme} />
      </div>
      <FiguresPlaceholder theme={theme} />
      <UsagePlaceholder theme={theme} />
    </div>
  );
}

/** A turn's calls still loading, as the table's rows. */
export function CallsPlaceholder({ theme }: { theme: Theme }) {
  const t = useT();
  return (
    <div
      role="status"
      aria-label={t("Reading this turn's calls")}
      style={{
        display: "grid",
        borderTop: `1px solid ${theme.divider}`,
        paddingTop: 4,
      }}
    >
      {[0, 1, 2, 3, 4].map((index) => (
        <div
          key={index}
          aria-hidden
          style={{
            display: "grid",
            gridTemplateColumns:
              "48px minmax(0, 1.4fr) repeat(4, minmax(0, 1fr))",
            gap: 12,
            alignItems: "center",
            padding: "7px 0",
            borderTop: index > 0 ? `1px solid ${theme.divider}` : 0,
          }}
        >
          <Bar height={10} theme={theme} />
          <Bar width={`${80 - (index % 2) * 20}%`} height={10} theme={theme} />
          {[0, 1, 2, 3].map((cell) => (
            <Bar key={cell} height={10} theme={theme} />
          ))}
        </div>
      ))}
    </div>
  );
}

/** More of the conversation still loading: two messages in grey. */
export function MessagesPlaceholder({ theme }: { theme: Theme }) {
  const t = useT();
  return (
    <div
      role="status"
      aria-label={t("Reading more messages")}
      style={{ display: "grid", gap: 8 }}
    >
      {[0, 1].map((index) => (
        <div
          key={index}
          aria-hidden
          style={{
            ...lines,
            gap: 7,
            borderTop: `1px solid ${theme.divider}`,
            paddingTop: 8,
          }}
        >
          <Bar width={120} height={10} theme={theme} />
          <Bar height={11} theme={theme} />
          <Bar width={`${68 - index * 20}%`} height={11} theme={theme} />
        </div>
      ))}
    </div>
  );
}
