import { useEffect, useRef, useState } from "react";
import type { PanelLayout, StatisticsResponse } from "@agent-hud/protocol";
import { systemTimeZone } from "@agent-hud/ui/format";
import { Switch } from "@agent-hud/ui/components/switch";
import {
  Notice,
  Row,
  Section,
  type SettingsPaneProps,
  useText,
} from "./SettingsPrimitives";
import { moveAmong, ReorderButtons } from "./agents/Reorder";

/** Which Codex models the token chart shows, and in what order. Models are
 * discovered from the usage ledger, not a promised list of available
 * models. Reads 30 days of statistics while it is on screen. */
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
  const move = (id: string, direction: -1 | 1) => {
    const ordered = [...new Set([...order, ...shown.map((row) => row.id)])];
    const next = moveAmong(
      ordered,
      shown.map((row) => row.id),
      id,
      direction,
    );
    if (next !== ordered) void change({ modelOrder: next });
  };
  return (
    <Section
      title={t("Token chart models", "Token 图表中的模型")}
      footer={t(
        "Models used in the last 30 days. Applies to the screen chosen on the Display page.",
        "最近 30 天用过的模型。作用于“显示”页面选中的屏幕。",
      )}
    >
      {error && <Notice tone="error">{error}</Notice>}
      {!loaded && !error && (
        <Row muted title={t("Reading models…", "正在读取模型…")} />
      )}
      {loaded && !shown.length && (
        <Row
          muted
          title={t(
            "No Codex models in the last 30 days",
            "最近 30 天没有 Codex 模型用量",
          )}
        />
      )}
      {shown.map((row, index) => (
        <Row
          key={row.id}
          title={row.name}
          subtitle={
            tokenLayout.models?.length && !tokenLayout.models.includes(row.id)
              ? t("Left out by the model filter", "模型筛选未包含此项")
              : undefined
          }
        >
          <ReorderButtons
            t={t}
            name={row.name}
            first={index === 0}
            last={index === shown.length - 1}
            disabled={pending}
            move={(direction) => move(row.id, direction)}
          />
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
        </Row>
      ))}
    </Section>
  );
}
