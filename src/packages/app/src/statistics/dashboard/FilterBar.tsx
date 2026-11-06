import type { Dispatch, SetStateAction } from "react";
import { ChevronDown, X } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { StatisticsTokenKind } from "@agent-hud/protocol";
import {
  freshKinds,
  kindLabel,
  kinds,
  plain,
  row,
  type StatsTheme,
} from "../StatsTheme";
import { Segments } from "../Segments";
import { dayStart, type Range, type RangeId } from "./model";

const chip = (theme: StatsTheme, selected: boolean) => ({
  ...plain,
  ...row,
  gap: 6,
  flex: "none",
  height: 28,
  padding: "0 11px",
  borderRadius: 14,
  fontSize: 12,
  fontWeight: selected ? 500 : 400,
  background: selected ? theme.selected : "transparent",
  boxShadow: selected
    ? theme.light
      ? "0 0 0 1px rgba(0,0,0,.10), 0 1px 2px rgba(0,0,0,.06)"
      : "0 0 0 1px rgba(255,255,255,.16)"
    : `inset 0 0 0 1px ${theme.border}`,
  transition: "background-color 160ms ease, box-shadow 160ms ease",
});

/** The overview's filters, which every section below follows: the agent,
 * the token kinds counted, and the range, or the day picked on the heatmap. */
export function FilterBar({
  agents,
  selected,
  setSelected,
  selectedKinds,
  setKinds,
  kindsLabel,
  range,
  setRange,
  theme,
  light,
}: {
  agents: { id: string; name: string }[];
  /** The agents picked; null for all. */
  selected: string[] | null;
  setSelected: (agents: string[] | null) => void;
  selectedKinds: StatisticsTokenKind[];
  setKinds: Dispatch<SetStateAction<StatisticsTokenKind[]>>;
  kindsLabel: string;
  range: Range;
  setRange: (range: Range) => void;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const ranges: { value: RangeId; label: string }[] = [
    { value: "today", label: t("Today", "今天") },
    { value: "7d", label: t("7 days", "7 天") },
    { value: "30d", label: t("30 days", "30 天") },
    { value: "90d", label: t("90 days", "90 天") },
    { value: "1y", label: t("Year", "一年") },
  ];
  return (
    <div
      className="stats-filter-bar"
      style={{
        ...row,
        position: "sticky",
        top: 44,
        zIndex: 15,
        gap: 12,
        padding: "6px 0 12px",
        background: `var(--statistics-ground, ${theme.ground})`,
        borderBottom: `1px solid ${theme.divider}`,
        flexWrap: "wrap",
      }}
    >
      <div
        role="group"
        aria-label={t("Agent", "Agent")}
        className="stats-agent-chips"
        style={{ ...row, gap: 6, flex: "1 1 320px", overflowX: "auto" }}
      >
        <button
          type="button"
          role="radio"
          aria-checked={selected === null}
          onClick={() => setSelected(null)}
          style={chip(theme, selected === null)}
        >
          {t("All agents", "全部 Agent")}
        </button>
        {agents.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="checkbox"
            aria-checked={Boolean(selected?.includes(entry.id))}
            title={t(
              "Click to see this agent alone; ⌘-click to add it",
              "点击只看这个 Agent；按住 ⌘ 点击可多选",
            )}
            onClick={(event) => {
              const on = Boolean(selected?.includes(entry.id));
              if (!event.metaKey) {
                setSelected(on && selected?.length === 1 ? null : [entry.id]);
                return;
              }
              const next = on
                ? (selected ?? []).filter((id) => id !== entry.id)
                : [...(selected ?? []), entry.id];
              setSelected(next.length ? next : null);
            }}
            style={chip(theme, Boolean(selected?.includes(entry.id)))}
          >
            <HUDLogo source={entry.id} size={14} light={light} />
            {entry.name}
          </button>
        ))}
      </div>
      <div style={{ ...row, gap: 8, flex: "none", marginLeft: "auto" }}>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label={t("Token kinds counted", "计入的 Token 类型")}
              style={{
                ...plain,
                ...row,
                fontSize: 12,
                height: 28,
                padding: "0 10px",
                borderRadius: 7,
                background: theme.segment,
              }}
            >
              {kindsLabel}
              <ChevronDown size={11} style={{ opacity: 0.7 }} />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuCheckboxItem
              checked={
                selectedKinds.length === freshKinds.length &&
                !selectedKinds.includes("cacheRead")
              }
              onSelect={(event) => event.preventDefault()}
              onCheckedChange={() => setKinds([...freshKinds])}
            >
              {t("Excl. cache reads", "不含缓存读取")}
            </DropdownMenuCheckboxItem>
            <DropdownMenuCheckboxItem
              checked={selectedKinds.length === kinds.length}
              onSelect={(event) => event.preventDefault()}
              onCheckedChange={() => setKinds([...kinds])}
            >
              {t("All tokens", "全部 Token")}
            </DropdownMenuCheckboxItem>
            <DropdownMenuSeparator />
            {kinds.map((kind) => (
              <DropdownMenuCheckboxItem
                key={kind}
                checked={selectedKinds.includes(kind)}
                onSelect={(event) => event.preventDefault()}
                onCheckedChange={(selected) =>
                  setKinds((previous) =>
                    selected
                      ? kinds.filter(
                          (value) => value === kind || previous.includes(value),
                        )
                      : previous.length > 1
                        ? previous.filter((value) => value !== kind)
                        : previous,
                  )
                }
              >
                {kindLabel(kind, zh)}
              </DropdownMenuCheckboxItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        {range.id === "day" && (
          <button
            type="button"
            onClick={() => setRange({ id: "30d" })}
            aria-label={t("Back to 30 days", "回到 30 天")}
            style={{ ...chip(theme, true), height: 28, gap: 5 }}
          >
            {new Date(dayStart(range.day)).toLocaleDateString(
              zh ? "zh-CN" : "en-US",
              { month: "short", day: "numeric", weekday: "short" },
            )}
            <X size={12} style={{ opacity: 0.7 }} />
          </button>
        )}
        <Segments
          label={t("Range", "范围")}
          value={range.id}
          onChange={(value) => setRange({ id: value as RangeId })}
          choices={ranges}
          theme={theme}
        />
      </div>
    </div>
  );
}
