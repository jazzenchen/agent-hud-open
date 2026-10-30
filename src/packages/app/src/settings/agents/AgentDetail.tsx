/* One agent's settings: its switches, where its accounts come from, what the
 * panel shows of it, the relays it uses and its API credentials. */
import { useState } from "react";
import { RefreshCw } from "lucide-react";
import type {
  NativeMethod,
  PanelLayout,
  Provider,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-hud/ui/components/tooltip";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import {
  LinkButton,
  Notice,
  Page,
  Section,
  ToggleRow,
  type Text,
} from "../SettingsPrimitives";
import CodexAccountHomes from "../CodexAccountHomes";
import AntigravityAccounts from "../AntigravityAccounts";
import { CodexModelSettings } from "../CodexModelSettings";
import { PanelBlocks } from "./PanelBlocks";
import { AgentRelays } from "./AgentRelays";
import { Connection } from "./Connection";

export interface AgentDetailProps {
  t: Text;
  snapshot: Snapshot;
  source: Provider;
  active?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
}

export function AgentDetail({
  t,
  snapshot,
  source,
  active,
  action,
  layout,
  onLayout,
  preferences,
  onPreferences,
}: AgentDetailProps) {
  // One save at a time. Its failure shows at the top of the detail, or through
  // `report` where the caller shows it beside its own buttons (Connection,
  // at the bottom of the page, where the top is out of view).
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function perform(
    job: () => Promise<unknown>,
    report: (message: string) => void = setError,
  ) {
    setPending(true);
    setError(null);
    try {
      await job();
      return true;
    } catch (error) {
      report(String(error));
      return false;
    } finally {
      setPending(false);
    }
  }

  const enabled = source.enabled;
  const live = preferences[source.id]?.live !== false;
  const reminders = preferences[source.id]?.reminders !== false;
  const offReason = t("Turn on Enabled first.", "请先启用。");
  const refreshLabel = t("Read all agents again", "重新读取所有智能体");

  return (
    <Page>
      <header className="agents-detail-header">
        <HUDLogo source={source.id} size={24} />
        <div className="agents-detail-title">
          <h2>{source.name}</h2>
          {!source.detected && (
            <p>{t("Not on this computer", "未在这台电脑上发现")}</p>
          )}
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={refreshLabel}
              disabled={pending}
              onClick={() => void perform(() => action("refresh"))}
            >
              <RefreshCw size={14} />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{refreshLabel}</TooltipContent>
        </Tooltip>
      </header>
      {error && (
        <Notice
          tone="error"
          action={
            <LinkButton onClick={() => setError(null)}>
              {t("Dismiss", "关闭")}
            </LinkButton>
          }
        >
          {error}
        </Notice>
      )}
      <Section footer={source.notice ?? undefined}>
        <ToggleRow
          title={t("Enabled", "启用")}
          checked={enabled}
          disabled={pending}
          change={(value) =>
            void perform(() =>
              action("set_provider", { id: source.id, enabled: value }),
            )
          }
        />
        <ToggleRow
          title={t("Live status", "实时状态")}
          subtitle={t(
            "Running and waiting sessions on the HUD.",
            "在 HUD 上显示运行和等待中的会话。",
          )}
          checked={live}
          disabled={pending || !enabled}
          reason={!enabled ? offReason : undefined}
          change={(value) =>
            void perform(() => onPreferences(source.id, { live: value }))
          }
        />
        <ToggleRow
          title={t("Completion reminders", "完成提醒")}
          checked={live && reminders}
          disabled={pending || !enabled || !live}
          reason={
            !enabled
              ? offReason
              : !live
                ? t("Needs Live status.", "需要先打开实时状态。")
                : undefined
          }
          change={(value) =>
            void perform(() => onPreferences(source.id, { reminders: value }))
          }
        />
      </Section>
      <div
        className={`agents-detail-rest${enabled ? "" : " is-off"}`}
        inert={!enabled}
      >
        {source.id === "codex" && (
          <CodexAccountHomes
            t={t}
            homes={snapshot.settings.codexHomes ?? []}
            onSave={(homes) =>
              action("set_setting", { key: "codexHomes", value: homes })
            }
          />
        )}
        {source.id === "antigravity" && (
          <AntigravityAccounts
            t={t}
            settings={snapshot.settings.antigravityCliSettings ?? []}
            action={action}
          />
        )}
        {source.id === "copilot" && (
          <Section>
            <ToggleRow
              title={t("Read GitHub Copilot quota", "读取 GitHub Copilot 额度")}
              subtitle={t(
                "Uses the GitHub sign-in on this computer.",
                "使用这台电脑上已登录的 GitHub 账号。",
              )}
              checked={snapshot.settings.readCopilotQuota === true}
              disabled={pending}
              change={(value) =>
                void perform(() =>
                  action("set_setting", { key: "readCopilotQuota", value }),
                )
              }
            />
          </Section>
        )}
        <PanelBlocks
          t={t}
          snapshot={snapshot}
          source={source}
          layout={layout}
          onLayout={onLayout}
          perform={perform}
          pending={pending}
        />
        {source.id === "codex" && (
          <CodexModelSettings
            snapshot={snapshot}
            action={action}
            active={active}
            layout={layout}
            onLayout={onLayout}
          />
        )}
        <AgentRelays t={t} snapshot={snapshot} source={source} />
        <Connection
          t={t}
          source={source}
          action={action}
          perform={perform}
          pending={pending}
        />
      </div>
    </Page>
  );
}
