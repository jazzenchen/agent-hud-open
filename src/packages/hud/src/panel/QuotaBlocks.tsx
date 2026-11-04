import { useMemo, type ReactNode } from "react";
import type {
  PanelBlockLayout,
  PanelNode,
  Quota,
  Snapshot,
} from "@agent-hud/protocol";
import { blockRows, quotaRowKey, vendorKey, type HUDConfig } from "../model";
import { SourceProviderBlock } from "./QuotaSection";
import { SourceBalances } from "./SourceBalances";
import { blockLines, COMPACT_LEAST } from "./block-lines";
import {
  inPanelOrder,
  type densityMetrics,
  type Logo,
  type palette,
} from "./theme";

/** The panel's blocks in lines, a hairline between lines when the panel draws
 * separators. */
export function QuotaBlocks({
  blocks,
  custom,
  layouts,
  width,
  density,
  separated,
  colors,
}: {
  blocks: { key: string; content: ReactNode }[];
  custom: Record<string, PanelNode>;
  layouts?: Record<string, PanelBlockLayout>;
  width: number;
  density: (typeof densityMetrics)[keyof typeof densityMetrics];
  separated: boolean;
  colors: ReturnType<typeof palette>;
}) {
  const span = (key: string) => {
    const asked = custom[key]?.span ?? layouts?.[key]?.span;
    return typeof asked === "string" ? asked : undefined;
  };
  // Blocks that ask for a half, a third or a quarter share a line where the
  // blocks' width holds them: the panel's, or the column's a row of an
  // agent's gives them.
  // A ring with no legend makes a compact block, as narrow as its picker.
  const least = (key: string) => {
    const layout = layouts?.[key];
    return layout?.legend === false &&
      ["rings", "segments", "ring"].includes(layout.widget ?? "")
      ? COMPACT_LEAST
      : undefined;
  };
  const lines = blockLines(blocks, span, width, density.section, least);
  return (
    <div
      className="hud-source-blocks"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--hud-section-gap)",
      }}
    >
      {lines.map(({ blocks: line, columns }, index) => (
        <div
          key={line.map((b) => b.key).join("+")}
          style={{
            display: "grid",
            gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
            gap: density.section,
            ...(index > 0 && separated
              ? {
                  boxShadow: `inset 0 1px 0 ${colors.divider}`,
                  paddingTop: "var(--hud-divided-gap)",
                }
              : {}),
          }}
        >
          {line.map((block) => (
            <div
              key={block.key}
              data-hud-block={block.key}
              data-hud-half={columns > 1 ? "true" : undefined}
              style={{ minWidth: 0 }}
            >
              {block.content}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

/** The panel's blocks, in its order: each agent's quota block, each API's
 * balance block, and the controls an agent placed among them. */
export function usePanelBlocks({
  snapshot,
  focusVendor,
  config,
  light,
  Logo,
  active,
  onStats,
  custom,
  customAt,
}: {
  snapshot: Snapshot;
  focusVendor?: string;
  config: HUDConfig;
  light: boolean;
  Logo: Logo;
  active: boolean;
  onStats?: () => void;
  custom: Record<string, PanelNode>;
  customAt: (key: string) => ReactNode;
}) {
  const rows = useMemo(
    () =>
      snapshot.quotas.filter(
        (row) =>
          !focusVendor || vendorKey(row.vendor) === vendorKey(focusVendor),
      ),
    [snapshot.quotas, focusVendor],
  );
  const rowsByVendor = useMemo(() => {
    const groups = new Map<string, Quota[]>();
    for (const row of rows) {
      const group = groups.get(row.vendor) ?? [];
      group.push(row);
      groups.set(row.vendor, group);
    }
    return groups;
  }, [rows]);
  const vendorNames = useMemo(
    () =>
      [
        ...new Set([
          ...rows.map((row) => row.vendor),
          ...snapshot.providers
            .filter(
              (provider) =>
                provider.detected &&
                provider.enabled &&
                (!focusVendor ||
                  vendorKey(provider.id) === vendorKey(focusVendor)) &&
                !snapshot.billing.some(
                  (account) =>
                    vendorKey(account.vendor) === vendorKey(provider.id),
                ),
            )
            .map((provider) => provider.id),
        ]),
      ].filter((vendor) => !config.panelLayout.blocks?.[vendor]?.hidden),
    [
      rows,
      snapshot.providers,
      snapshot.billing,
      focusVendor,
      config.panelLayout,
    ],
  );
  const billing = snapshot.billing.filter(
    (account) =>
      !focusVendor || vendorKey(account.vendor) === vendorKey(focusVendor),
  );
  const blocks = inPanelOrder<{ key: string; content: ReactNode }>(
    [
      ...vendorNames.map((vendor) => ({
        key: vendor,
        content: (
          <SourceProviderBlock
            vendor={vendor}
            rows={blockRows(
              rowsByVendor.get(vendor) ?? [],
              config.panelLayout.blocks?.[vendor],
            ).map((row) => ({ ...row, key: quotaRowKey(row) }))}
            layout={config.panelLayout.blocks?.[vendor]}
            snapshot={snapshot}
            active={active}
            now={Date.parse(snapshot.generatedAt)}
            light={light}
            Logo={Logo}
            showReset={config.resetCountdown}
            accountInfo={snapshot.accounts}
            onStats={onStats}
          />
        ),
      })),
      ...[...new Set(billing.map((account) => account.vendor))]
        .filter(
          (vendor) => !config.panelLayout.blocks?.[`api:${vendor}`]?.hidden,
        )
        .map((vendor) => ({
          key: `api:${vendor}`,
          content: (
            <SourceBalances
              billing={billing.filter((account) => account.vendor === vendor)}
              layout={config.panelLayout.blocks?.[`api:${vendor}`]}
              light={light}
              Logo={Logo}
              onStats={onStats}
            />
          ),
        })),
      ...(config.panelOrder ?? [])
        .filter((key) => custom[key] && custom[key].hidden !== true)
        .map((key) => ({ key, content: customAt(key) })),
    ],
    config.panelOrder ?? [],
  );
  return { blocks, vendorNames };
}
