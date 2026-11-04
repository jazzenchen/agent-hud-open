import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type {
  Session,
  SourcePreferences,
  SessionsQuery,
  SessionsResponse,
  StatisticsQuery,
  StatisticsResponse,
} from "@agent-hud/protocol";
import {
  noAnswer,
  settled,
  shownAnswer,
  type PanelAnswer,
} from "./panel-answers";
export interface PanelServices {
  query(
    method: "statistics" | "sessions",
    params: Record<string, unknown>,
  ): Promise<unknown>;
  selectSession?(session: Session): void;
  selectProvider?(provider: string): void;
  /** Resolves false when the session could not be reached (the reply card then offers a retry). */
  openSession?(session: Session): void | Promise<boolean | void>;
  /** Settings, on the page with this id, such as "agent-plugins". */
  openSettingsPage?(page: string): void;
  /** Agent HUD's own sign-in window on `host`, for `provider`'s quota. */
  webSignIn?(provider: string, host: string): void;
  /** The client's own app, brought forward or started, for a step only the
   * user can take there (allowing Agent HUD's hooks in Codex). */
  openClient?(source: string): void;
  sources?: Record<string, SourcePreferences>;
  draft?: {
    unsaved: boolean;
    /** What waits: this display's own layout or the shared one, a HUD style, the app's settings. */
    layout?: "screen" | "shared" | null;
    style?: boolean;
    app?: boolean;
    heldUntil: number | null;
    save(): Promise<unknown>;
    discard(): Promise<unknown>;
    release(): void;
  };
}
const Context = createContext<PanelServices | null>(null);
export function PanelServicesProvider({
  value,
  children,
}: {
  value: PanelServices;
  children: ReactNode;
}) {
  return <Context.Provider value={value}>{children}</Context.Provider>;
}
export const usePanelServices = () => useContext(Context);
/** Shared by the measuring and visible panel: one query per committed revision/key. */
const cache = new Map<string, Promise<unknown>>();
/** The last answer to each question (its parameters without the time
 * window), whatever revision it came from: a panel that opens shows it at
 * once, and a refresh changes it instead of blanking it. */
const latest = new Map<string, unknown>();
/** A section's query: its answer, or until that comes the last answer to the
 * same question (`shownAnswer`). An answer that comes after a newer query was
 * asked still counts while none newer has come (`settled`): while Core indexes,
 * its revision moves every 2 s and answers can take longer than that. */
export function usePanelQuery<T>(
  method: "statistics" | "sessions",
  params: StatisticsQuery | SessionsQuery,
  revision: number | undefined,
  enabled: boolean,
) {
  const services = usePanelServices();
  const key = JSON.stringify([method, revision, params]);
  const base = JSON.stringify([
    method,
    { ...params, from: undefined, to: undefined },
  ]);
  const [answer, setAnswer] = useState<PanelAnswer<T>>(() =>
    latest.has(base)
      ? { key: "", base, order: 0, data: latest.get(base) as T, error: null }
      : noAnswer,
  );
  const asked = useRef(0);
  useEffect(() => {
    if (!enabled || !services) return;
    const order = ++asked.current;
    let promise = cache.get(key);
    if (!promise) {
      promise = services.query(
        method,
        params as unknown as Record<string, unknown>,
      );
      cache.set(key, promise);
      while (cache.size > 32) cache.delete(cache.keys().next().value!);
      promise.catch(() => cache.delete(key));
    }
    promise.then(
      (value) => {
        latest.delete(base);
        latest.set(base, value);
        while (latest.size > 32) latest.delete(latest.keys().next().value!);
        setAnswer((current) =>
          settled(current, { key, base, order, data: value as T, error: null }),
        );
      },
      (error) =>
        setAnswer((current) =>
          settled(current, {
            key,
            base,
            order,
            data: null,
            error: String(error),
          }),
        ),
    );
  }, [key, base, enabled, services]);
  return { ...shownAnswer(answer, key, base), available: Boolean(services) };
}
export type PanelStatistics = StatisticsResponse;
export type PanelSessions = SessionsResponse;
