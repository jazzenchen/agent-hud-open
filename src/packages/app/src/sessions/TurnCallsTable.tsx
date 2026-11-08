import { RotateCw } from "lucide-react";
import { dateFormat } from "@agent-hud/ui/format";
import type { SessionCall } from "@agent-hud/protocol";
import { clipped, compact, numeric, row, type Theme } from "./SessionTheme";

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
  const zh = Boolean(theme.zh);
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
              {zh ? "时间" : "Time"}
            </th>
            <th style={{ ...head, textAlign: "left" }}>
              {zh ? "模型" : "Model"}
            </th>
            <th style={head}>{zh ? "新增" : "New"}</th>
            <th style={head}>{zh ? "缓存写" : "Cache W"}</th>
            <th style={head}>{zh ? "输入" : "Input"}</th>
            {reasoning && <th style={head}>{zh ? "思考" : "Reasoning"}</th>}
            <th style={head}>{zh ? "输出" : "Output"}</th>
            <th style={head}>{zh ? "缓存读" : "Cache R"}</th>
            {contexts && <th style={head}>{zh ? "上下文" : "Context"}</th>}
            {tools && (
              <th style={{ ...head, textAlign: "left", paddingRight: 0 }}>
                {zh ? "发起工具" : "Asked for"}
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
                {dateFormat(zh ? "zh-CN" : "en-GB", {
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
                      {zh ? "子 agent" : "Sub-agent"}
                    </span>
                  )}
                  {call.recached != null && (
                    <RotateCw
                      size={10}
                      color={theme.warning}
                      strokeWidth={2.6}
                      aria-label={zh ? "缓存读取下降" : "Cache reads dropped"}
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
                  {(call.tools ?? []).map(toolName).join(zh ? "、" : ", ") ||
                    "—"}
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
