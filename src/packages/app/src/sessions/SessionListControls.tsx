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
import { useFormat, useT } from "@agent-hud/ui/format-context";

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
  const t = useT();
  const { language } = useFormat();
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
          label={t("Session arrangement")}
          value={arrangement}
          onChange={(value) => onArrangement(value as Arrangement)}
          choices={[
            { value: "day", label: t("Day") },
            { value: "active", label: t("Active") },
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
          {arrangement === "active"
            ? t("{{total}} in 24 hours", { total: page?.total ?? 0 })
            : t("{{total}} in 7 days", { total: page?.total ?? 0 })}
        </span>
        <SourceSelect
          label={t("Session source")}
          value={source}
          onChange={onSource}
          width={185}
          theme={theme}
        >
          <SelectItem value={ALL}>{t("All sources")}</SelectItem>
          {sources.map((name) => (
            <SelectItem key={name} value={name}>
              {name}
            </SelectItem>
          ))}
        </SourceSelect>
      </div>
      <div style={{ ...row, gap: 12, height: 26 }}>
        <SourceSelect
          label={t("Project path")}
          value={project}
          onChange={onProject}
          width={260}
          theme={theme}
        >
          <SelectItem value={ALL}>{t("All projects")}</SelectItem>
          {projects.map((path) => (
            <SelectItem key={path} value={path}>
              {page?.projectLabels?.[path] ?? projectName(path, language)}
            </SelectItem>
          ))}
          {page?.projects.includes(null) && (
            <SelectItem value="__unassigned__">{t("Unassigned")}</SelectItem>
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
            placeholder={t("Search sessions or paths")}
            aria-label={t("Search sessions")}
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
              aria-label={t("Clear search")}
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
