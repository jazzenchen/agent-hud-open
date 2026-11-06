import type { ReactNode } from "react";
import { Search, X } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@agent-hud/ui/components/select";
import { Input } from "@agent-hud/ui/components/input";
import type { SessionsResponse } from "@agent-hud/protocol";
import { Segments } from "../statistics/Segments";
import {
  ALL,
  numeric,
  plain,
  projectName,
  row,
  type Arrangement,
  type Theme,
} from "./SessionTheme";

function SourceSelect({
  value,
  onChange,
  width,
  label,
  children,
  theme,
}: {
  value: string;
  onChange: (value: string) => void;
  width: number;
  label: string;
  children: ReactNode;
  theme: Theme;
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger
        aria-label={label}
        size="sm"
        style={{
          width,
          minWidth: 0,
          height: 26,
          padding: "2px 8px",
          fontSize: 11,
          borderRadius: 6,
          borderColor: theme.border,
          color: theme.text,
          background: theme.card,
          boxShadow: "none",
        }}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent>{children}</SelectContent>
    </Select>
  );
}

/** The Sessions list's controls, held at the top as it scrolls: by day or active and how many, the source, the project and a search. */
export function SessionListControls({
  arrangement,
  onArrangement,
  source,
  onSource,
  project,
  onProject,
  query,
  onQuery,
  page,
  theme,
  stickyTop = 44,
}: {
  arrangement: Arrangement;
  onArrangement: (arrangement: Arrangement) => void;
  source: string;
  onSource: (source: string) => void;
  project: string;
  onProject: (project: string) => void;
  query: string;
  onQuery: (query: string) => void;
  page: SessionsResponse | null;
  theme: Theme;
  /** Where the controls stick: under the window's toolbar, or at the top
   * of the list's own column. */
  stickyTop?: number;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const sources = page?.sources ?? [];
  const projects = (page?.projects ?? []).filter(
    (path): path is string => path !== null,
  );
  return (
    <div
      className="session-statistics-controls"
      style={{
        position: "sticky",
        top: stickyTop,
        zIndex: 15,
        background: `var(--statistics-ground, ${theme.ground})`,
        display: "flex",
        flexDirection: "column",
        gap: 10,
        paddingTop: 6,
        paddingBottom: 12,
        marginBottom: 14,
        borderBottom: `1px solid ${theme.divider}`,
      }}
    >
      <div style={{ ...row, gap: 12, minHeight: 26 }}>
        <Segments
          small
          label={t("Session arrangement", "会话排列")}
          value={arrangement}
          onChange={(value) => onArrangement(value as Arrangement)}
          choices={[
            { value: "day", label: t("Day", "按日期") },
            { value: "active", label: t("Active", "活跃") },
          ]}
          theme={theme}
        />
        <span
          style={{
            marginLeft: "auto",
            color: theme.secondary,
            fontSize: 11,
            whiteSpace: "nowrap",
            ...numeric,
          }}
        >
          {zh
            ? `${arrangement === "active" ? "24 小时" : "7 天"}内 ${page?.total ?? 0} 个`
            : `${page?.total ?? 0} in ${arrangement === "active" ? "24 hours" : "7 days"}`}
        </span>
        <SourceSelect
          label={t("Session source", "会话来源")}
          value={source}
          onChange={onSource}
          width={185}
          theme={theme}
        >
          <SelectItem value={ALL}>{t("All sources", "全部来源")}</SelectItem>
          {sources.map((name) => (
            <SelectItem key={name} value={name}>
              {name}
            </SelectItem>
          ))}
        </SourceSelect>
      </div>
      <div style={{ ...row, gap: 12, height: 26 }}>
        <SourceSelect
          label={t("Project path", "项目路径")}
          value={project}
          onChange={onProject}
          width={260}
          theme={theme}
        >
          <SelectItem value={ALL}>{t("All projects", "全部项目")}</SelectItem>
          {projects.map((path) => (
            <SelectItem key={path} value={path}>
              {page?.projectLabels?.[path] ?? projectName(path, zh)}
            </SelectItem>
          ))}
          {page?.projects.includes(null) && (
            <SelectItem value="__unassigned__">
              {t("Unassigned", "未归属项目")}
            </SelectItem>
          )}
        </SourceSelect>
        <div
          style={{
            ...row,
            gap: 6,
            height: 26,
            flex: 1,
            borderRadius: 6,
            padding: "0 8px",
            border: `1px solid ${theme.border}`,
            background: theme.light ? "#fff" : "#1c1c1e",
          }}
        >
          <Search size={12} color={theme.secondary} />
          <Input
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            placeholder={t("Search sessions or paths", "搜索会话标题或路径")}
            aria-label={t("Search sessions", "搜索会话")}
            style={{
              border: 0,
              background: "transparent",
              boxShadow: "none",
              height: 24,
              padding: 0,
              fontSize: 11,
              color: theme.text,
            }}
          />
          {query && (
            <button
              type="button"
              aria-label={t("Clear search", "清空搜索")}
              style={{ ...plain, color: theme.secondary }}
              onClick={() => onQuery("")}
            >
              <X size={11} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
