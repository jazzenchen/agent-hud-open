import { useEffect, useRef, useState } from "react";
import type { PanelLayout, StatisticsResponse } from "@agent-hud/protocol";
import { systemTimeZone } from "@agent-hud/ui/format";
import { Button } from "@agent-hud/ui/components/button";
import { Switch } from "@agent-hud/ui/components/switch";
import { ChevronDown, ChevronUp } from "lucide-react";
import {
  Divider,
  Row,
  secondary,
  type SettingsPaneProps,
  useText,
} from "./SettingsPrimitives";

/** Models are discovered from the usage ledger, not a promised list of available models. */
export function CodexModelSettings({
  snapshot,
  action,
  active,
  layout,
  onLayout,
}: Pick<SettingsPaneProps, "snapshot" | "action" | "active"> & {
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
}) {
  const t = useText(snapshot);
  const [models, setModels] = useState<
    StatisticsResponse["available"]["models"]
  >([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [pending, setPending] = useState(false);
  const actionRef = useRef(action);
  actionRef.current = action;
  useEffect(() => {
    if (active === false) return;
    let current = true;
    const to = new Date(Math.ceil(Date.now() / 900000) * 900000);
    const from = new Date(to.getTime() - 30 * 86_400_000);
    void actionRef
      .current("statistics", {
        from: from.toISOString(),
        to: to.toISOString(),
        bucketMinutes: 1440,
        timeZone: systemTimeZone(),
        tokenKinds: ["input", "output", "cacheRead", "cacheWrite", "reasoning"],
      })
      .then((answer) => {
        if (!current) return;
        const result = answer as StatisticsResponse;
        if (result.kind !== "statistics")
          throw new Error("Unsupported statistics response");
        setModels(
          result.available.models.filter((row) => row.provider === "codex"),
        );
        setError(null);
        setLoaded(true);
      })
      .catch((failure: unknown) => {
        if (current)
          setError(
            failure instanceof Error ? failure.message : String(failure),
          );
      });
    return () => {
      current = false;
    };
  }, [active, snapshot.statisticsRevision]);
  const tokenLayout = layout.tokens ?? {};
  const hidden = tokenLayout.hiddenModels ?? [];
  const order = tokenLayout.modelOrder ?? [];
  const positions = new Map(order.map((id, index) => [id, index]));
  const shown = models
    .map((model, index) => ({ model, index }))
    .sort(
      (a, b) =>
        (positions.get(a.model.id) ?? Infinity) -
          (positions.get(b.model.id) ?? Infinity) || a.index - b.index,
    )
    .map(({ model }) => model);
  const change = async (patch: NonNullable<PanelLayout["tokens"]>) => {
    setPending(true);
    setError(null);
    try {
      await onLayout({ ...layout, tokens: { ...tokenLayout, ...patch } });
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setPending(false);
    }
  };
  const move = (id: string, offset: number) => {
    const ordered = [...new Set([...order, ...shown.map((row) => row.id)])];
    const current = shown.findIndex((row) => row.id === id);
    const other = shown[current + offset];
    if (!other) return;
    const a = ordered.indexOf(id),
      b = ordered.indexOf(other.id);
    [ordered[a], ordered[b]] = [ordered[b], ordered[a]];
    void change({ modelOrder: ordered });
  };
  return (
    <div>
      <Row
        title={t("Codex models", "Codex 模型")}
        subtitle={t(
          "Models used in the past 30 days; new models appear automatically. Changes apply to this screen’s token chart.",
          "最近 30 天使用过的模型，新模型会自动出现。此设置作用于本屏幕的 Token 图表。",
        )}
      >
        <span />
      </Row>
      {error && (
        <p
          style={{
            ...secondary,
            color: "var(--destructive)",
            padding: "0 16px",
          }}
        >
          {error}
        </p>
      )}
      {loaded && !shown.length && (
        <p style={{ ...secondary, fontSize: 12, padding: "0 16px" }}>
          {t(
            "No Codex models used in the last 30 days.",
            "最近 30 天没有 Codex 模型用量。",
          )}
        </p>
      )}
      {shown.map((row, index) => (
        <div key={row.id}>
          <Divider />
          <Row
            title={row.name}
            subtitle={
              tokenLayout.models?.length && !tokenLayout.models.includes(row.id)
                ? t(
                    "Excluded by the existing model filter",
                    "现有模型筛选未包含此项",
                  )
                : undefined
            }
          >
            <Switch
              checked={!hidden.includes(row.id)}
              disabled={pending}
              aria-label={`${t("Show model", "显示模型")} ${row.name}`}
              onCheckedChange={(show) =>
                void change({
                  hiddenModels: show
                    ? hidden.filter((id) => id !== row.id)
                    : [...new Set([...hidden, row.id])],
                })
              }
            />
            <Button
              variant="ghost"
              size="icon-xs"
              disabled={pending || index === 0}
              aria-label={`${t("Move up", "上移")} ${row.name}`}
              onClick={() => move(row.id, -1)}
            >
              <ChevronUp size={13} />
            </Button>
            <Button
              variant="ghost"
              size="icon-xs"
              disabled={pending || index === shown.length - 1}
              aria-label={`${t("Move down", "下移")} ${row.name}`}
              onClick={() => move(row.id, 1)}
            >
              <ChevronDown size={13} />
            </Button>
          </Row>
        </div>
      ))}
    </div>
  );
}
