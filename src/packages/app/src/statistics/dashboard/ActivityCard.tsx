import { useState } from "react";
import type { StatisticsResponse } from "@agent-hud/protocol";
import type { StatsTheme } from "../StatsTheme";
import { Segments } from "../Segments";
import { DayCalendar } from "./DayCalendar";
import { HourGrid } from "./HourGrid";
import { Panel } from "./Panel";
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
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const [view, setView] = useState<"days" | "hours">("days");
  return (
    <Panel
      area="heatmap"
      theme={theme}
      title={t("When you use it", "使用节奏")}
      meta={
        view === "days"
          ? t(
              "Tokens per day · hover a day for its agents and models, click it to look at it alone",
              "每天的 Token · 悬停看 Agent 和模型，点击单独查看这一天",
            )
          : t(
              "Tokens by weekday and hour over the range",
              "所选时间里每个星期几、每个小时的 Token",
            )
      }
      actions={
        <Segments
          label={t("Heatmap", "热力图")}
          value={view}
          onChange={(value) => setView(value as "days" | "hours")}
          choices={[
            { value: "days", label: t("By day", "按天") },
            { value: "hours", label: t("By hour", "按小时") },
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
