/* The panel that opens from the HUD on the selected screen: its density, its
 * sections (each row shows or hides one and moves it up or down; the first
 * three have a small sample under them, and the quota history and the plan
 * value are off until asked for), and the order of the agent blocks. */
import { Fragment } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { Switch } from "@agent-hud/ui/components/switch";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import {
  agentVendorName,
  defaultHUDConfig,
  type HUDConfig,
} from "@agent-hud/hud/model";
import {
  addedSectionShown,
  isAddedSection,
  withAddedSection,
  type AddedSection,
} from "@agent-hud/hud/panel/added-sections";
import { MoreRows, Row, Section } from "../SettingsPrimitives";
import { Segments, ToggleRow } from "../SettingsControls";
import { PartSample } from "../PartSample";
import type { DisplayProps } from "./controls";
import { ReorderButtons } from "./ReorderButtons";
import { useLightTheme } from "../agent-plugins/use-status";

const sectionSettings = {
  blocks: { setting: "quotaDetails", sample: "quota" },
  tokens: { setting: "tokenUsage", sample: "tokens" },
  sessions: { setting: "sessionList", sample: "sessions" },
} as const;

type PanelSectionKey = keyof typeof sectionSettings | AddedSection;

const builtIn = (key: string): key is PanelSectionKey =>
  Object.hasOwn(sectionSettings, key) || isAddedSection(key);

export function PanelSection({
  snapshot,
  config,
  update,
  t,
}: DisplayProps & { snapshot: Snapshot }) {
  const light = useLightTheme();
  const sectionNames: Record<PanelSectionKey, string> = {
    blocks: t("Quota details", "额度明细"),
    tokens: t("Token usage", "Token 消耗"),
    sessions: t("Session list", "会话列表"),
    quotaHistory: t("Quota history", "额度历史"),
    planValue: t("Plan value", "订阅价值"),
  };
  // Saved order first; a section the saved order lacks keeps its default place.
  // A section an agent added to the panel has no row here and keeps its place
  // when the others move.
  const order = [
    ...new Set([...config.panelSections, ...defaultHUDConfig.panelSections]),
  ];
  const sections = order.filter(builtIn);
  const moveSection = (index: number, by: -1 | 1) => {
    const next = [...order],
      from = next.indexOf(sections[index]),
      to = next.indexOf(sections[index + by]);
    [next[from], next[to]] = [next[to], next[from]];
    update({ panelSections: next });
  };

  const blocks = [
    ...new Map([
      ...snapshot.quotas.map(
        (row) =>
          [
            row.vendor,
            {
              id: row.vendor,
              source: row.vendor,
              title: agentVendorName(row.vendor),
            },
          ] as const,
      ),
      ...snapshot.billing.map(
        (account) =>
          [
            `api:${account.vendor}`,
            {
              id: `api:${account.vendor}`,
              source: account.vendor,
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
        const name = sectionNames[section];
        const added = isAddedSection(section);
        const shown = added
          ? addedSectionShown(config.panelLayout, section)
          : config[sectionSettings[section].setting];
        const show = (value: boolean) =>
          isAddedSection(section)
            ? update({
                panelLayout: withAddedSection(
                  config.panelLayout,
                  order,
                  section,
                  value,
                ),
              })
            : update({ [sectionSettings[section].setting]: value });
        return (
          <Fragment key={section}>
            <Row
              title={name}
              below={
                !isAddedSection(section) && (
                  <PartSample
                    kind={sectionSettings[section].sample}
                    enabled={shown}
                    config={config}
                    t={t}
                  />
                )
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
                checked={shown}
                onCheckedChange={show}
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
            <Row
              key={block.id}
              title={block.title}
              icon={<HUDLogo source={block.source} size={18} light={light} />}
            >
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
