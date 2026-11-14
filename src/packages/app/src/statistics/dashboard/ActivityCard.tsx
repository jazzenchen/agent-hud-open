import { useState } from "react";
import type { StatisticsResponse } from "@agent-hud/protocol";
import { useT } from "@agent-hud/ui/format-context";
import type { StatsTheme } from "../StatsTheme";
import { Segments } from "../Segments";
import { DayCalendar } from "./DayCalendar";
import { HourGrid } from "./HourGrid";
import { Panel } from "./Panel";
import type { Pending } from "./pending";
import { GridPlaceholder } from "./Placeholder";

/** When the tokens were used, in one card: every day as a calendar heatmap
 * (a year at most, a click narrows the page to that day), or the range by
 * weekday and hour. */
export function ActivityCard({
  year,
  range,
  agents,
  selectedDay,
  onSelectDay,
  now,
  readBackTo = null,
  focus = null,
  pending,
  theme,
}: {
  /** The heatmap's year, for the days. */
  year: StatisticsResponse | null;
  /** The page's range, for the hours. */
  range: StatisticsResponse | null;
  agents: string[] | null;
  selectedDay: string | null;
  onSelectDay: (day: string) => void;
  now: number;
  /** While the history is read, how far back it has got. */
  readBackTo?: number | null;
  /** The page's range, which the day view brings forward. */
  focus?: { from: number; to: number } | null;
  /** The year's and the range's answers, while a filter change waits. */
  pending: { year: Pending; range: Pending };
  theme: StatsTheme;
}) {
  const t = useT();
  const [view, setView] = useState<"days" | "hours">("days");
  return (
    <Panel
      area="heatmap"
      pending={[view === "days" ? pending.year : pending.range]}
      theme={theme}
      title={t("When you use it")}
      meta={
        view === "days"
          ? t(
              "Tokens per day · hover a day for its agents and models, click it to look at it alone",
            )
          : t("Tokens by weekday and hour over the range")
      }
      actions={
        <Segments
          label={t("Heatmap")}
          value={view}
          onChange={(value) => setView(value as "days" | "hours")}
          choices={[
            { value: "days", label: t("By day") },
            { value: "hours", label: t("By hour") },
          ]}
          theme={theme}
          small
        />
      }
    >
      {view === "days" ? (
        <DayCalendar
          data={year}
          agents={agents}
          selectedDay={selectedDay}
          onSelectDay={onSelectDay}
          now={now}
          readBackTo={readBackTo}
          focus={focus}
          theme={theme}
        />
      ) : range ? (
        <HourGrid
          data={range.heatmap}
          available={range.available}
          theme={theme}
        />
      ) : (
        <div style={{ paddingLeft: 31, paddingTop: 16 }}>
          <GridPlaceholder columns={24} cell={20} theme={theme} />
        </div>
      )}
    </Panel>
  );
}
