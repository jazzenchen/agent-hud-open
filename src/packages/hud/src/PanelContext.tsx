import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import type { EndsLink } from "./ends/ends-link";
export type { EndsLink, EndsOp } from "./ends/ends-link";
import type {
  PlanValueQuery,
  QuotaHistoryQuery,
  Session,
  SourcePreferences,
  SessionsQuery,
  SessionsResponse,
  StatisticsQuery,
  StatisticsResponse,
} from "@agent-hud/protocol";
import type {
  InspectSendResult,
  InspectTarget,
  InspectTargets,
  InspectView,
} from "@agent-hud/protocol";
/** The host's side of the clipboard window: each change answers the new
 * state, which every screen's HUD also hears through `subscribe`. */
export interface InspectServices {
  state(): Promise<InspectView>;
  subscribe(listener: (view: InspectView) => void): () => void;
  /** Reads the latest copy's content: the person points at it. */
  look(): Promise<InspectView>;
  add(): Promise<InspectView>;
  /** A comment on item `id`, or with none the note for the whole message. */
  edit(id: string | null, text: string): Promise<InspectView>;
  remove(id: string): Promise<InspectView>;
  clear(): Promise<InspectView>;
  targets(): Promise<InspectTargets>;
  /** Opens picture `id` in the markup window, to draw on and pin comments. */
  markPicture(id: string): Promise<unknown>;
  send(target: InspectTarget): Promise<InspectSendResult>;
}
/** What a panel section asks Core. */
export type PanelQueryMethod =
  "statistics" | "sessions" | "quota_history" | "plan_value";
import {
  noAnswer,
  settled,
  shownAnswer,
  type PanelAnswer,
} from "./panel-answers";
export interface PanelServices {
  query(
    method: PanelQueryMethod,
    params: Record<string, unknown>,
  ): Promise<unknown>;
  selectSession?(session: Session): void;
  selectProvider?(provider: string): void;
  /** Resolves false when the session could not be reached (the reply card then offers a retry). */
  openSession?(session: Session): void | Promise<boolean | void>;
  /** Settings, on the page with this id, such as "agents" (an agent's own: "agents/claude"). */
  openSettingsPage?(page: string): void;
  /** Agent HUD's own sign-in window on `host`, for `provider`'s quota. */
  webSignIn?(provider: string, host: string): void;
  /** The client's own app, brought forward or started, for a step only the
   * user can take there (allowing Agent HUD's hooks in Codex). */
  openClient?(source: string): void;
  sources?: Record<string, SourcePreferences>;
  /** The other screens' HUDs, for one set of dock ends on all of them. */
  ends?: EndsLink;
  /** The clipboard bubble's window, kept by the host for every screen. */
  inspect?: InspectServices;
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
 * window), whatever revision it came from and whichever panel asked it: a
 * panel that opens shows it at once, a refresh changes it instead of
 * blanking it, and every section follows it as it changes. The measuring
 * copy asks after the visible panel, at a newer revision when Core moved
 * on, so it measures the rows the visible panel shows rather than
 * "Loading" under them (Open #116). */
const latest = new Map<string, unknown>();
const listeners = new Set<() => void>();
const listen = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};
/** A section's query: its answer, or until that comes the last answer to the
 * same question (`shownAnswer`). An answer that comes after a newer query was
 * asked still counts while none newer has come (`settled`): while Core indexes,
 * its revision moves every 2 s and answers can take longer than that. */
export function usePanelQuery<T>(
  method: PanelQueryMethod,
  params: StatisticsQuery | SessionsQuery | QuotaHistoryQuery | PlanValueQuery,
  revision: number | undefined,
  enabled: boolean,
) {
  const services = usePanelServices();
  const key = JSON.stringify([method, revision, params]);
  const base = JSON.stringify([
    method,
    { ...params, from: undefined, to: undefined },
  ]);
  const [answer, setAnswer] = useState<PanelAnswer<T>>(noAnswer);
  const last = useSyncExternalStore(
    listen,
    () => latest.get(base) as T | undefined,
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
        listeners.forEach((listener) => listener());
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
  return {
    ...shownAnswer(answer, key, base, last),
    available: Boolean(services),
  };
}
export type PanelStatistics = StatisticsResponse;
export type PanelSessions = SessionsResponse;
