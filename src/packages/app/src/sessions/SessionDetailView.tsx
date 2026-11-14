import { useState, type CSSProperties } from "react";
import { Card } from "@agent-hud/ui/components/card";
import { sessionClient } from "../lib/sessions";
import type { Session, SessionDetail, Snapshot } from "@agent-hud/protocol";
import type { Measure } from "./TurnBarsChart";
import { KindsCard, ModelsCard } from "./BreakdownCards";
import { DetailFigures } from "./SessionDetailFigures";
import { DetailHeader } from "./SessionDetailHeader";
import { SessionTranscript } from "./SessionTranscript";
import { TurnChartCard } from "./TurnChartCard";
import { TurnPanel } from "./TurnPanel";
import { reveal as fadeIn } from "./SessionMotion";
import { UsagePlaceholder } from "./SessionPlaceholders";
import { ProjectExecutionsCard, SubagentsCard } from "./WorkerCards";
import { card, CardTitle, type Theme } from "./SessionTheme";
import { useT } from "@agent-hud/ui/format-context";

const stack: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 12,
};

/** A session's page: its header and figures, its turns, where its tokens
 * went, its sub-agents and its conversation. A picked turn opens under the
 * chart, or (`turnBeside`) in the column beside, which draws it instead.
 * Each session's head, and what Core reads of it, fades in as it comes. */
export function InlineDetail({
  session,
  detail,
  loading,
  error,
  snapshot,
  theme,
  selectedEntry,
  onEntry,
  onSession,
  pickedSession = null,
  turnBeside = false,
  reveal,
  loadMessages,
  loadingMessages,
}: {
  session: Session;
  detail: SessionDetail | null;
  loading: boolean;
  error: string | null;
  snapshot: Snapshot;
  theme: Theme;
  selectedEntry: string | null;
  onEntry: (id: string | null) => void;
  onSession: (id: string) => void;
  /** The sub-agent open in the column beside. */
  pickedSession?: string | null;
  turnBeside?: boolean;
  reveal: () => void;
  loadMessages: () => void;
  loadingMessages: boolean;
}) {
  const t = useT();
  const [measure, setMeasure] = useState<Measure>("tokens");
  const client = sessionClient(session, snapshot.providers);
  const now = Date.parse(snapshot.generatedAt);
  const entries = detail?.turns ?? [];
  const byPrompt = detail?.turnsBy === "prompt";
  const picked = entries.find((entry) => entry.id === selectedEntry);
  const lastMessage = session.liveFacts?.message?.trim();
  const children = detail?.children ?? [];
  const helped = Boolean(detail?.subagents) || children.length > 0;
  const projectHasLocalWorkers =
    (detail?.session.desktopProject?.localWorkerCount ??
      session.desktopProject?.localWorkerCount ??
      0) > 0;
  // One model says nothing the figures do not (Open shows the card from two).
  const models = (detail?.models?.length ?? 0) > 1;
  return (
    <div style={stack}>
      <div key={session.id} className={fadeIn} style={stack}>
        <DetailHeader
          session={session}
          detail={detail}
          client={client}
          now={now}
          theme={theme}
          // Project sessions use Core's return route and label. When Core marks
          // that route unavailable, never fall back to revealing a local log.
          reveal={
            session.desktopProject && !session.desktopProject.openAvailable
              ? null
              : session.navigationTarget || session.logPath
                ? reveal
                : null
          }
          revealDisabled={Boolean(
            session.desktopProject && !session.desktopProject.openAvailable,
          )}
          revealLabel={
            session.desktopProject?.openLabel ??
            (session.navigationTarget
              ? t("Return to session")
              : snapshot.connection.platform === "macos"
                ? t("Reveal log in Finder")
                : t("Reveal log"))
          }
        />
        <DetailFigures session={session} detail={detail} theme={theme} />
      </div>
      {!detail && loading && <UsagePlaceholder theme={theme} />}
      {error && (
        <div role="alert" style={{ color: theme.secondary, fontSize: 11 }}>
          {error}
        </div>
      )}
      {detail && (
        <div className={fadeIn} style={stack}>
          <TurnChartCard
            detail={detail}
            byPrompt={byPrompt}
            measure={measure}
            onMeasure={setMeasure}
            selectedEntry={selectedEntry}
            onEntry={onEntry}
            theme={theme}
          />
          {picked && !turnBeside && (
            <TurnPanel
              key={picked.id}
              entry={picked}
              detail={detail}
              byPrompt={byPrompt}
              theme={theme}
              close={() => onEntry(null)}
            />
          )}
          {/* Token/model breakdown beside the session's related workers. */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))",
              gap: 12,
              alignItems: "start",
            }}
          >
            <div style={{ display: "grid", gap: 12, minWidth: 0 }}>
              <KindsCard detail={detail} theme={theme} />
              {models && helped && (
                <ModelsCard detail={detail} session={session} theme={theme} />
              )}
            </div>
            {detail && projectHasLocalWorkers ? (
              <ProjectExecutionsCard
                detail={detail}
                theme={theme}
                onSession={onSession}
                selectedId={pickedSession}
              />
            ) : helped ? (
              <SubagentsCard
                detail={detail}
                theme={theme}
                onSession={onSession}
                selectedId={pickedSession}
              />
            ) : (
              models && (
                <ModelsCard detail={detail} session={session} theme={theme} />
              )
            )}
          </div>
          {lastMessage && (
            <Card style={card(theme)}>
              <CardTitle title={t("Last reply")} theme={theme} />
              <div
                style={{
                  fontSize: 12,
                  lineHeight: 1.45,
                  whiteSpace: "pre-wrap",
                  userSelect: "text",
                  maxHeight: "18em",
                  overflow: "auto",
                }}
              >
                {lastMessage}
              </div>
            </Card>
          )}
          <SessionTranscript
            detail={detail}
            theme={theme}
            loadMessages={loadMessages}
            loadingMessages={loadingMessages}
          />
        </div>
      )}
    </div>
  );
}
