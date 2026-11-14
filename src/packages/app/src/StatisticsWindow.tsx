import {
  lazy,
  Suspense,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Toaster } from "sonner";
import { Tabs, TabsList, TabsTrigger } from "@agent-hud/ui/components/tabs";
import { TooltipProvider } from "@agent-hud/ui/components/tooltip";
import type { NativeMethod, Session, Snapshot } from "@agent-hud/protocol";
import { useFormat, useT } from "@agent-hud/ui/format-context";
import { LoadingState } from "./LoadingState";
import { ViewErrorBoundary } from "./ViewErrorBoundary";
import { themeFor } from "./statistics/StatsTheme";
import { PagePlaceholder } from "./statistics/dashboard/Placeholder";

// Statistics does not need the shader/bot renderer, nor the settings panes.
const Statistics = lazy(() => import("./Statistics"));
const SessionStatistics = lazy(() => import("./SessionStatistics"));

/** The Statistics window: its Tokens and Sessions tabs, and the tab or
 * session the host asks it to show. */
export function StatisticsWindow({
  snapshot,
  theme,
  act,
  windowActive,
  demo,
  error,
  refresh,
  sourceMenu,
}: {
  snapshot: Snapshot | null;
  theme: "dark" | "light";
  act: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  windowActive: boolean;
  demo: boolean;
  error: string | null;
  refresh: () => Promise<unknown>;
  sourceMenu: ReactNode;
}) {
  const t = useT();
  const { language } = useFormat();
  const [statisticsTab, setStatisticsTab] = useState("tokens");
  const [selectedSession, setSelectedSession] = useState<Session | null>(null);
  useEffect(() => {
    setSelectedSession(null);
  }, [demo]);
  useEffect(() => {
    if (!isTauri()) return;
    const subscription = getCurrentWebviewWindow().listen<string>(
      "statistics:tab",
      (event) => setStatisticsTab(event.payload),
    );
    return () => {
      void subscription.then((unlisten) => unlisten());
    };
  }, []);
  const sessionsRef = useRef<Session[]>([]);
  sessionsRef.current = snapshot?.sessions ?? [];
  useEffect(() => {
    if (!isTauri()) return;
    const applySession = (id: string) => {
      const session = sessionsRef.current.find((s) => s.id === id);
      setSelectedSession(session ?? ({ id } as Session));
      setStatisticsTab("sessions");
    };
    const pendingSession = getCurrentWebviewWindow().listen<string>(
      "statistics:session",
      (event) => {
        applySession(event.payload);
      },
    );
    const pendingProvider = getCurrentWebviewWindow().listen<{
      provider: string;
      at: number;
    }>("statistics:provider", () => {
      setSelectedSession(null);
      setStatisticsTab("tokens");
    });
    void invoke<{ tab?: string | null; sessionId?: string | null } | null>(
      "take_statistics_target",
    ).then((target) => {
      if (!target) return;
      if (target.sessionId) applySession(target.sessionId);
      else if (target.tab) setStatisticsTab(target.tab);
    });
    return () => {
      void pendingSession.then((unlisten) => unlisten());
      void pendingProvider.then((unlisten) => unlisten());
    };
  }, []);
  // Before the page has anything to draw, its shape in grey.
  const placeholder = (
    <PagePlaceholder theme={themeFor(theme === "light", language)} />
  );
  return (
    <TooltipProvider>
      <div className="statistics-window" data-theme={theme}>
        <header className="statistics-toolbar" data-tauri-drag-region>
          <span className="statistics-toolbar-spacer" data-tauri-drag-region />
          <Tabs value={statisticsTab} onValueChange={setStatisticsTab}>
            <TabsList>
              <TabsTrigger value="tokens">{t("Usage")}</TabsTrigger>
              <TabsTrigger value="sessions">{t("Sessions")}</TabsTrigger>
            </TabsList>
          </Tabs>
          <div>{sourceMenu}</div>
        </header>
        {snapshot ? (
          <ViewErrorBoundary onReset={() => setSelectedSession(null)}>
            <Suspense
              fallback={statisticsTab === "tokens" ? placeholder : null}
            >
              <div
                style={{
                  display: statisticsTab === "tokens" ? "block" : "none",
                }}
              >
                <Statistics
                  snapshot={snapshot}
                  light={theme === "light"}
                  action={act}
                  active={windowActive && statisticsTab === "tokens"}
                  onSelectSession={(session) => {
                    setSelectedSession(session);
                    setStatisticsTab("sessions");
                  }}
                />
              </div>
              <div
                style={{
                  display: statisticsTab === "sessions" ? "block" : "none",
                }}
              >
                <SessionStatistics
                  snapshot={snapshot}
                  light={theme === "light"}
                  action={act}
                  active={windowActive && statisticsTab === "sessions"}
                  initialSessionId={selectedSession?.id ?? null}
                  onClearSelection={() => setSelectedSession(null)}
                />
              </div>
            </Suspense>
          </ViewErrorBoundary>
        ) : !error && statisticsTab === "tokens" ? (
          placeholder
        ) : (
          <LoadingState
            label={
              error
                ? t("Couldn’t read local activity")
                : t("Reading local activity…")
            }
            failed={!!error}
            detail={error}
            retryLabel={t("Retry")}
            onRetry={() => void refresh()}
          />
        )}
        <Toaster theme={theme} />
      </div>
    </TooltipProvider>
  );
}
