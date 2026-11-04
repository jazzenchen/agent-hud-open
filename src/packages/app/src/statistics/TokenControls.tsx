import { useMemo, type Dispatch, type SetStateAction } from "react";
import { ChevronDown } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import type { StatisticsQuery, StatisticsTokenKind } from "@agent-hud/protocol";
import {
  freshKinds,
  kindLabel,
  kinds,
  plain,
  row,
  type StatsTheme,
} from "./StatsTheme";
import { Segments } from "./Segments";

export type Range = "5h" | "24h" | "7d" | "30d";
export const validBucketsByRange: Record<
  Range,
  StatisticsQuery["bucketMinutes"][]
> = {
  "5h": [15, 30, 60],
  "24h": [15, 30, 60],
  "7d": [60, 1440],
  "30d": [1440],
};

/** The Tokens page's sticky toolbar: the token kinds counted, the time bucket and the range. */
export function TokenControls({
  theme,
  label,
  selectedKinds,
  setKinds,
  range,
  setRange,
  bucketMinutes,
  effectiveBucketMinutes,
  setBucketMinutes,
}: {
  theme: StatsTheme;
  label: string;
  selectedKinds: StatisticsTokenKind[];
  setKinds: Dispatch<SetStateAction<StatisticsTokenKind[]>>;
  range: Range;
  setRange: (range: Range) => void;
  bucketMinutes: StatisticsQuery["bucketMinutes"];
  effectiveBucketMinutes: StatisticsQuery["bucketMinutes"];
  setBucketMinutes: (minutes: StatisticsQuery["bucketMinutes"]) => void;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const validBuckets = validBucketsByRange[range];
  const rangeChoices = useMemo(
    () => [
      { value: "5h", label: zh ? "5 小时" : "5 h" },
      { value: "24h", label: zh ? "24 小时" : "24 h" },
      { value: "7d", label: zh ? "7 天" : "7 days" },
      { value: "30d", label: zh ? "30 天" : "30 days" },
    ],
    [zh],
  );
  const bucketChoices = useMemo(() => {
    const all: { value: StatisticsQuery["bucketMinutes"]; label: string }[] = [
      { value: 15, label: zh ? "15 分钟" : "15min" },
      { value: 30, label: zh ? "30 分钟" : "30min" },
      { value: 60, label: zh ? "1 小时" : "1hr" },
      { value: 1440, label: zh ? "1 天" : "1d" },
    ];
    return all
      .filter((choice) => validBuckets.includes(choice.value))
      .map((choice) => ({ value: String(choice.value), label: choice.label }));
  }, [validBuckets, zh]);
  return (
    <div
      className="statistics-token-controls"
      style={{
        ...row,
        position: "sticky",
        top: 44,
        zIndex: 15,
        background: `var(--statistics-ground, ${theme.ground})`,
        gap: 10,
        minHeight: 28,
        paddingTop: 6,
        paddingBottom: 12,
        marginBottom: 4,
        borderBottom: `1px solid ${theme.divider}`,
        flexWrap: "wrap",
      }}
    >
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label={t("Token kinds counted", "计入的 Token 类型")}
            style={{
              ...plain,
              ...row,
              fontSize: 12,
              fontWeight: 500,
              height: 28,
              padding: "0 10px",
              borderRadius: 6,
              background: theme.segment,
            }}
          >
            Token · {label}
            <ChevronDown size={11} style={{ opacity: 0.7 }} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
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
      <span style={{ flex: 1, minWidth: 12 }} />
      {bucketChoices.length > 1 && (
        <Segments
          label={t("Time bucket", "时间粒度")}
          value={String(effectiveBucketMinutes)}
          onChange={(value) =>
            setBucketMinutes(Number(value) as StatisticsQuery["bucketMinutes"])
          }
          choices={bucketChoices}
          theme={theme}
        />
      )}
      <Segments
        label={t("Statistics range", "统计范围")}
        value={range}
        onChange={(value) => {
          const nextRange = value as Range;
          setRange(nextRange);
          const allowed = validBucketsByRange[nextRange];
          if (!allowed.includes(bucketMinutes)) {
            setBucketMinutes(
              nextRange === "7d" ? 60 : allowed[allowed.length - 1],
            );
          }
        }}
        choices={rangeChoices}
        theme={theme}
      />
    </div>
  );
}
