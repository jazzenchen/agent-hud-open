import type { Dispatch, SetStateAction } from "react";
import { Check } from "lucide-react";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { StatisticsResponse } from "@agent-hud/protocol";
import {
  compact,
  modelColor,
  number,
  plain,
  row,
  truncate,
  type StatsTheme,
} from "./StatsTheme";

/** The Tokens section's model checklist, folded under its Models button:
 * each recorded model with its colour and its tokens in the range. */
export function ModelFilter({
  expanded,
  data,
  modelChoices,
  loading,
  selectedModels,
  setModels,
  theme,
  light,
}: {
  expanded: boolean;
  data: StatisticsResponse | null;
  modelChoices: StatisticsResponse["available"]["models"];
  loading: boolean;
  selectedModels: string[] | null;
  setModels: Dispatch<SetStateAction<string[] | null>>;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const pickModel = (id: string) =>
    setModels((previous) => {
      if (previous === null) return [id];
      if (previous.includes(id)) {
        const next = previous.filter((value) => value !== id);
        return next.length ? next : null;
      }
      return [...previous, id].sort();
    });
  return (
    <div
      style={{
        display: "grid",
        gridTemplateRows: expanded ? "1fr" : "0fr",
        opacity: expanded ? 1 : 0,
        visibility: expanded ? "visible" : "hidden",
        pointerEvents: expanded ? "auto" : "none",
        transition:
          "grid-template-rows 200ms cubic-bezier(0.22, 1, 0.36, 1), opacity 180ms ease",
      }}
    >
      <div style={{ overflow: "hidden", minHeight: 0 }}>
        <div style={{ display: "grid", gap: 6, paddingTop: 8 }}>
          <button
            type="button"
            onClick={() => setModels(null)}
            style={{
              ...plain,
              ...row,
              color: theme.secondary,
              fontSize: 11,
            }}
          >
            <Check
              size={11}
              style={{ opacity: selectedModels === null ? 1 : 0 }}
            />
            {t("All models", "全部模型")}
            {selectedModels !== null &&
              (zh ? " · 清除筛选" : " · Clear filter")}
          </button>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit,minmax(190px,1fr))",
              gap: "6px 16px",
            }}
          >
            {modelChoices.map((model) => {
              const value = data?.models.find((entry) => entry.id === model.id);
              const checked = selectedModels?.includes(model.id) ?? false;
              return (
                <button
                  key={model.id}
                  type="button"
                  role="checkbox"
                  aria-checked={checked}
                  aria-label={`Include ${model.name} from ${model.provider}`}
                  title={`${model.provider} · ${model.name}`}
                  onClick={() => pickModel(model.id)}
                  style={{
                    ...plain,
                    ...row,
                    width: "100%",
                    textAlign: "left",
                    color: checked ? theme.text : theme.secondary,
                    fontSize: 11,
                    padding: "3px 4px",
                    borderRadius: 4,
                    background: checked ? theme.segment : "transparent",
                  }}
                >
                  <span
                    aria-hidden
                    style={{
                      width: 13,
                      height: 13,
                      flexShrink: 0,
                      borderRadius: 3.5,
                      border: `1px solid ${checked ? "#007aff" : theme.divider}`,
                      background: checked ? "#007aff" : "transparent",
                      color: "#fff",
                      display: "inline-grid",
                      placeItems: "center",
                    }}
                  >
                    {checked && <Check size={9} strokeWidth={3} />}
                  </span>
                  <span
                    aria-hidden
                    style={{
                      width: 6,
                      height: 6,
                      flexShrink: 0,
                      borderRadius: 2,
                      background: modelColor(model.id),
                    }}
                  />
                  <HUDLogo source={model.provider} size={12} light={light} />
                  <span style={truncate}>{model.name}</span>
                  <span style={{ marginLeft: "auto", ...number }}>
                    {value ? compact(value.selectedTokens) : "—"}
                  </span>
                </button>
              );
            })}
          </div>
          {!modelChoices.length && (
            <span style={{ color: theme.secondary, fontSize: 11 }}>
              {loading
                ? t("Loading models…", "正在加载模型…")
                : t("No recorded models", "暂无已记录的模型")}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
