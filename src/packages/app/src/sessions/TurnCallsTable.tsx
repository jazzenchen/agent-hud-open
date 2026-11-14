import { RotateCw } from "lucide-react";
import { dateFormat, listSeparator } from "@agent-hud/ui/format";
import type { SessionCall } from "@agent-hud/protocol";
import {
  clipped,
  compact,
  dateLocale,
  numeric,
  row,
  type Theme,
} from "./SessionTheme";
import { useFormat, useT } from "@agent-hud/ui/format-context";

/** An MCP tool by its own name, without its server's (Open's SessionCharts.toolName). */
function toolName(name: string): string {
  return name.startsWith("mcp__") ? (name.split("__").at(-1) ?? name) : name;
}

/** A turn's calls, one row each: when, which model, what each added and read, and the tools it asked for. */
export function TurnCallsTable({
  calls,
  theme,
}: {
  calls: SessionCall[];
  theme: Theme;
}) {
  const t = useT();
  const { language } = useFormat();
  const reasoning = calls.some((call) => (call.reasoning ?? 0) > 0);
  const contexts = calls.some((call) => call.context != null);
  const tools = calls.some((call) => (call.tools?.length ?? 0) > 0);
  const cell = { padding: "5px 6px", textAlign: "right" as const };
  const head = {
    padding: "4px 6px",
    fontWeight: 500,
    textAlign: "right" as const,
  };
  return (
    <div
      style={{
        overflowX: "auto",
        borderTop: `1px solid ${theme.divider}`,
        paddingTop: 4,
      }}
    >
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontSize: 11,
          ...numeric,
        }}
      >
        <thead>
          <tr
            style={{
              color: theme.secondary,
              borderBottom: `1px solid ${theme.divider}`,
            }}
          >
            <th style={{ ...head, textAlign: "left", paddingLeft: 0 }}>
              {t("Time")}
            </th>
            <th style={{ ...head, textAlign: "left" }}>{t("Model")}</th>
            <th style={head}>{t("New")}</th>
            <th style={head}>{t("Cache W")}</th>
            <th style={head}>{t("Input")}</th>
            {reasoning && <th style={head}>{t("Reasoning")}</th>}
            <th style={head}>{t("Output")}</th>
            <th style={head}>{t("Cache R")}</th>
            {contexts && <th style={head}>{t("Context")}</th>}
            {tools && (
              <th style={{ ...head, textAlign: "left", paddingRight: 0 }}>
                {t("Asked for")}
              </th>
            )}
          </tr>
        </thead>
        <tbody>
          {calls.map((call, index) => (
            <tr
              key={call.id ?? index}
              style={{
                borderTop: index > 0 ? `1px solid ${theme.divider}` : 0,
              }}
            >
              <td
                style={{
                  ...cell,
                  textAlign: "left",
                  paddingLeft: 0,
                  color: theme.secondary,
                }}
              >
                {dateFormat(dateLocale(language), {
                  hour: "numeric",
                  minute: "numeric",
                  second: "numeric",
                  hour12: false,
                }).format(new Date(call.timestamp))}
              </td>
              <td style={{ ...cell, textAlign: "left", maxWidth: 160 }}>
                <span style={{ ...row, gap: 5 }}>
                  <span style={clipped}>
                    {call.modelName || call.model || "—"}
                  </span>
                  {call.subagent && (
                    <span
                      style={{
                        fontSize: 10,
                        fontWeight: 600,
                        padding: "0 5px",
                        borderRadius: 4,
                        background: theme.segment,
                        color: theme.secondary,
                        flexShrink: 0,
                      }}
                    >
                      {t("Sub-agent")}
                    </span>
                  )}
                  {call.recached != null && (
                    <RotateCw
                      size={10}
                      color={theme.warning}
                      strokeWidth={2.6}
                      aria-label={t("Cache expired")}
                    />
                  )}
                </span>
              </td>
              <td style={{ ...cell, fontWeight: 600 }}>
                {call.newTokens != null ? compact(call.newTokens) : "—"}
              </td>
              <td style={cell}>{compact(call.cacheWrite ?? 0)}</td>
              <td style={cell}>{compact(call.input)}</td>
              {reasoning && (
                <td style={cell}>{compact(call.reasoning ?? 0)}</td>
              )}
              <td style={cell}>{compact(call.output)}</td>
              <td style={{ ...cell, color: theme.secondary }}>
                {compact(call.cacheRead ?? 0)}
              </td>
              {contexts && (
                <td style={{ ...cell, color: theme.secondary }}>
                  {call.context == null ? "—" : compact(call.context)}
                </td>
              )}
              {tools && (
                <td
                  title={(call.tools ?? []).join(", ")}
                  style={{
                    ...cell,
                    textAlign: "left",
                    paddingRight: 0,
                    color: theme.secondary,
                    maxWidth: 170,
                    ...clipped,
                  }}
                >
                  {(call.tools ?? [])
                    .map(toolName)
                    .join(listSeparator(language)) || "—"}
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
