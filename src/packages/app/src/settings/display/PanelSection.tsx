/* The panel that opens from the HUD on the selected screen: its density, its
 * three sections (each row shows or hides one, moves it up or down, and has a
 * small sample under it), and the order of the agent blocks. */
import { Fragment } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { Switch } from "@agent-hud/ui/components/switch";
import {
  agentVendorName,
  defaultHUDConfig,
  type HUDConfig,
} from "@agent-hud/hud/model";
import {
  MoreRows,
  Row,
  Section,
  Segments,
  ToggleRow,
} from "../SettingsPrimitives";
import { PartSample } from "../SettingsSamples";
import type { DisplayProps } from "./controls";
import { ReorderButtons } from "./ReorderButtons";

type PanelSectionKey = HUDConfig["panelSections"][number];

const sectionSettings = {
  blocks: { setting: "quotaDetails", sample: "quota" },
  tokens: { setting: "tokenUsage", sample: "tokens" },
  sessions: { setting: "sessionList", sample: "sessions" },
} as const;

export function PanelSection({
  snapshot,
  config,
  update,
  t,
}: DisplayProps & { snapshot: Snapshot }) {
  const sectionNames: Record<PanelSectionKey, string> = {
    blocks: t("Quota details", "额度明细"),
    tokens: t("Token usage", "Token 消耗"),
    sessions: t("Session list", "会话列表"),
  };
  // Saved order first; a section the saved order lacks keeps its default place.
  const sections = [
    ...new Set([...config.panelSections, ...defaultHUDConfig.panelSections]),
  ];
  const moveSection = (index: number, by: -1 | 1) => {
    const next = [...sections];
    [next[index], next[index + by]] = [next[index + by], next[index]];
    update({ panelSections: next });
  };

  const blocks = [
    ...new Map([
      ...snapshot.quotas.map(
        (row) =>
          [
            row.vendor,
            { id: row.vendor, title: agentVendorName(row.vendor) },
          ] as const,
      ),
      ...snapshot.billing.map(
        (account) =>
          [
            `api:${account.vendor}`,
            {
              id: `api:${account.vendor}`,
              title: `${agentVendorName(account.vendor)} · ${t("Balance", "余额")}`,
            },
          ] as const,
      ),
    ]).values(),
  ];
  const allBlockKeys = [
    ...new Set([...config.panelOrder, ...blocks.map((block) => block.id)]),
  ];
  const orderedBlocks = allBlockKeys.flatMap((key) => {
    const block = blocks.find((item) => item.id === key);
    return block ? [block] : [];
  });
  const moveBlock = (index: number, by: -1 | 1) => {
    const next = [...allBlockKeys],
      from = next.indexOf(orderedBlocks[index].id),
      to = next.indexOf(orderedBlocks[index + by].id);
    [next[from], next[to]] = [next[to], next[from]];
    update({ panelOrder: next });
  };

  return (
    <Section title={t("Panel", "展开面板")}>
      <Row title={t("Layout density", "布局密度")}>
        <Segments
          label={t("Layout density", "布局密度")}
          value={config.layoutDensity}
          options={[
            ["comfortable", t("Comfortable", "宽松")],
            ["normal", t("Normal", "普通")],
            ["compact", t("Compact", "紧凑")],
          ]}
          change={(value) =>
            update({ layoutDensity: value as HUDConfig["layoutDensity"] })
          }
        />
      </Row>
      {sections.map((section, index) => {
        const { setting, sample } = sectionSettings[section];
        const name = sectionNames[section];
        return (
          <Fragment key={section}>
            <Row
              title={name}
              below={
                <PartSample
                  kind={sample}
                  enabled={config[setting]}
                  config={config}
                  t={t}
                />
              }
            >
              <ReorderButtons
                name={name}
                index={index}
                count={sections.length}
                move={(by) => moveSection(index, by)}
                t={t}
              />
              <Switch
                aria-label={name}
                checked={config[setting]}
                onCheckedChange={(value) => update({ [setting]: value })}
              />
            </Row>
            {section === "blocks" && (
              <ToggleRow
                title={t("Reset countdown", "重置倒计时")}
                checked={config.resetCountdown}
                change={(value) => update({ resetCountdown: value })}
                disabled={!config.quotaDetails}
                reason={t(
                  "Shown with quota details. Turn those on first.",
                  "跟随额度明细显示，请先打开额度明细。",
                )}
              />
            )}
          </Fragment>
        );
      })}
      {orderedBlocks.length > 1 && (
        <MoreRows label={t("Agent order", "智能体顺序")}>
          {orderedBlocks.map((block, index) => (
            <Row key={block.id} title={block.title}>
              <ReorderButtons
                name={block.title}
                index={index}
                count={orderedBlocks.length}
                move={(by) => moveBlock(index, by)}
                t={t}
              />
            </Row>
          ))}
        </MoreRows>
      )}
    </Section>
  );
}
