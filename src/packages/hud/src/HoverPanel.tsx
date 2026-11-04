import {
  memo,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { MotionConfig } from "motion/react";
import type { Snapshot } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import {
  defaultHUDConfig,
  type HUDConfig,
  type HUDUpdateNotice,
} from "./model";
import { usePanelServices } from "./PanelContext";
import { useGround } from "./ground";
import { PanelDraftBar } from "./panel/PanelDraftBar";
import { PanelFooter } from "./panel/PanelFooter";
import { QuotaBlocks, usePanelBlocks } from "./panel/QuotaBlocks";
import { SourceSessions } from "./panel/SessionsSection";
import {
  densityMetrics,
  inPanelOrder,
  palette,
  type Logo,
} from "./panel/theme";
import { SourceTokenChart } from "./panel/TokenChartSection";
import { SourceIndexing } from "./panel/IndexingSection";
import { CustomControl } from "./panel/custom/CustomControl";
import { heldSections } from "./panel/custom/custom-data";
import { snapshotLanguage } from "@agent-hud/ui/format";

export { SOURCE_PANEL } from "./panel/theme";

export interface SourceHoverPanelProps {
  config?: HUDConfig;
  snapshot: Snapshot;
  focusVendor?: string;
  light: boolean;
  Logo: Logo;
  onConfigure?: () => void;
  onStats?: () => void;
  /** The product's newer version, as Open's HUD showed it: an arrow beside
   * Statistics that opens the update in Settings. */
  update?: HUDUpdateNotice | null;
  onUpdate?: () => void;
  maxHeight?: number;
  active?: boolean;
  inlineAlert?: ReactNode;
}

/** Default HoverPanelView arrangement. Missing account plans and per-cycle measures remain unknown. */
export const SourceHoverPanel = memo(function SourceHoverPanel({
  config = defaultHUDConfig,
  snapshot,
  focusVendor,
  light,
  Logo,
  onConfigure,
  onStats,
  update,
  onUpdate,
  maxHeight,
  active = true,
  inlineAlert,
}: SourceHoverPanelProps) {
  const colors = palette(useGround(light));
  const services = usePanelServices();
  const panelElement = useRef<HTMLDivElement>(null),
    [contentWidth, setContentWidth] = useState<number>();
  useEffect(() => {
    const element = panelElement.current;
    if (!element) return;
    const measure = () => setContentWidth(element.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const density = densityMetrics[config.layoutDensity] ?? densityMetrics.normal;
  const separated = config.panelLayout.style?.separators !== false;
  // A section the panel's lists follow: a hairline below it.
  const dividedBelow = separated
    ? {
        boxShadow: `inset 0 -1px 0 ${colors.divider}`,
        paddingBottom: "var(--hud-divided-gap)",
      }
    : undefined;
  // Controls an agent added, placed among the blocks by `order` or among the
  // sections by `sections`; an agent's own panel shows its blocks alone.
  const custom = focusVendor ? {} : (config.panelLayout.custom ?? {});
  const customAt = (key: string, divided = false) => {
    const node = custom[key];
    return node && node.hidden !== true ? (
      <CustomControl
        node={node}
        snapshot={snapshot}
        light={light}
        Logo={Logo}
        active={active}
        showReset={config.resetCountdown}
        onStats={onStats}
        divided={divided}
        section={(key, width, bare) => sectionContent(key, width, bare)}
        separated={separated}
      />
    ) : null;
  };
  const { blocks, vendorNames } = usePanelBlocks({
    snapshot,
    focusVendor,
    config,
    light,
    Logo,
    active,
    onStats,
    custom,
    customAt,
  });
  // The panel's own sections, at its top or at the width a row, column or grid
  // of an agent's gives them, there with no line above where they head a column.
  const sectionContent = (
    key: string,
    width?: number,
    bare = false,
  ): ReactNode => {
    if (key === "blocks")
      return (
        config.quotaDetails &&
        blocks.length > 0 && (
          <QuotaBlocks
            blocks={blocks}
            custom={custom}
            layouts={config.panelLayout.blocks}
            width={width ?? contentWidth ?? config.panelWidth - 40}
            density={density}
            separated={separated}
            colors={colors}
          />
        )
      );
    if (key === "tokens")
      return (
        config.tokenUsage && (
          <SourceTokenChart
            snapshot={snapshot}
            focusVendor={focusVendor}
            light={light}
            Logo={Logo}
            layout={config.panelLayout.tokens}
            active={active}
            bare={bare}
          />
        )
      );
    if (key === "sessions")
      return (
        config.sessionList && (
          <SourceSessions
            snapshot={snapshot}
            focusVendor={focusVendor}
            light={light}
            Logo={Logo}
            onStats={onStats}
            layout={config.panelLayout.sessions}
            active={active}
            bare={bare}
          />
        )
      );
    return null;
  };
  // A section a row, column or grid of an agent's holds is drawn there instead.
  const held = new Set(
    ((config.panelSections ?? []) as string[]).flatMap((key) =>
      custom[key] ? heldSections(custom[key]) : [],
    ),
  );
  const sections = inPanelOrder<{
    key: string;
    content: ReactNode;
    custom?: boolean;
  }>(
    [
      ...["blocks", "tokens", "sessions"]
        .filter((key) => !held.has(key))
        .map((key) => ({ key, content: sectionContent(key) })),
      ...((config.panelSections ?? []) as string[])
        .filter((key) => custom[key] && custom[key].hidden !== true)
        .map((key) => ({ key, content: true, custom: true })),
    ],
    config.panelSections ?? defaultHUDConfig.panelSections,
  );
  const panel = (
    <div
      ref={panelElement}
      className="hud-source-hover-panel"
      data-density={config.layoutDensity}
      style={
        {
          "--hud-section-gap": `${density.section}px`,
          "--hud-block-gap": `${density.block}px`,
          "--hud-row-padding": `${density.row}px`,
          "--hud-session-gap": `${density.session}px`,
          "--hud-divided-gap": `${density.divided}px`,
          display: "flex",
          flexDirection: "column",
          gap: density.section,
          maxHeight,
          color: colors.text,
        } as CSSProperties
      }
    >
      <div
        data-hud-natural-scroll
        className={
          snapshot.connection.platform === "windows"
            ? "hud-windows-scroll"
            : undefined
        }
        style={{
          overflowY: "auto",
          minHeight: 0,
          scrollbarWidth:
            snapshot.connection.platform === "windows" ? "none" : "thin",
          flex: "1 1 auto",
        }}
      >
        <div
          className="hud-source-sections"
          style={{
            display: "flex",
            flexDirection: "column",
            gap: density.section,
          }}
        >
          {inlineAlert && (
            <div data-hud-section="inline-alert" style={dividedBelow}>
              {inlineAlert}
            </div>
          )}
          {snapshot.indexing?.kind && (
            <div data-hud-section="indexing" style={dividedBelow}>
              <SourceIndexing snapshot={snapshot} light={light} />
            </div>
          )}
          {focusVendor && !vendorNames.length && (
            <strong style={{ fontSize: 13, fontWeight: 600 }}>
              {focusVendor}
            </strong>
          )}
          {sections
            .filter((section) => section.content)
            .map((section, index) => (
              <div key={section.key} data-hud-section={section.key}>
                {section.custom
                  ? customAt(section.key, index > 0 && separated)
                  : section.content}
              </div>
            ))}
        </div>
      </div>
      <PanelDraftBar
        draft={services?.draft}
        language={snapshotLanguage(snapshot)}
        colors={colors}
      />
      {services?.draft?.heldUntil && (
        <Button size="xs" variant="ghost" onClick={services.draft.release}>
          {snapshotLanguage(snapshot) === "zh" ? "完成编辑" : "Finish editing"}
        </Button>
      )}
      <PanelFooter
        language={snapshotLanguage(snapshot)}
        colors={colors}
        onConfigure={onConfigure}
        onStats={onStats}
        update={update}
        onUpdate={onUpdate}
      />
    </div>
  );
  // The panel's charts move as the HUD does: not at all when animations are
  // off (the setting, a picture) or the system asks for reduced motion.
  return (
    <MotionConfig
      reducedMotion={
        config.animations && snapshot.settings.animations ? "user" : "always"
      }
    >
      {panel}
    </MotionConfig>
  );
});
