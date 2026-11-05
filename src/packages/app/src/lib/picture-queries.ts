import type {
  PlanValueResponse,
  QuotaHistoryResponse,
  SessionsResponse,
} from "@agent-hud/protocol";

/** The panel's requests that a picture waits for: the token chart's
 * statistics, the sessions list, the quota history and the plan value. */
export type PanelQuery =
  "statistics" | "sessions" | "quota_history" | "plan_value";

/** How long, from the picture page's start, its panel requests may take. The
 * host gives up on the whole picture after 25 s, and WebKit's own picture may
 * take up to 15 s of that. While the Mac was busy, one statistics read took
 * 15 s (2026-11-05), and panel pictures timed out although the panel had
 * drawn. What has not come by then is drawn as the panel shows it while
 * loading, and named in the picture's `loading`. */
export const QUERY_DEADLINE_MS = 8_000;

/** Which section of the panel each request fills, as get_hud names them. */
const SECTION: Record<PanelQuery, string> = {
  statistics: "tokens",
  sessions: "sessions",
  quota_history: "quotaHistory",
  plan_value: "planValue",
};

/** An answer as a picture shows it: sessions without their titles and
 * folders, accounts without their names. */
export function maskedAnswer(method: PanelQuery, value: unknown): unknown {
  if (method === "sessions") {
    const response = value as SessionsResponse;
    return {
      ...response,
      items: response.items.map((session, index) => ({
        ...session,
        title: `Session ${index + 1}`,
        cwd: null,
        logPath: null,
      })),
    };
  }
  if (method === "quota_history") {
    const response = value as QuotaHistoryResponse;
    const accounts = [
      ...new Set(response.windows.map((window) => window.accountId ?? "")),
    ];
    return {
      ...response,
      windows: response.windows.map((window) => ({
        ...window,
        account: window.account
          ? `Account ${accounts.indexOf(window.accountId ?? "") + 1}`
          : window.account,
      })),
    };
  }
  if (method === "plan_value") {
    const response = value as PlanValueResponse;
    return {
      ...response,
      accounts: response.accounts.map((account, index) => ({
        ...account,
        name: `Account ${index + 1}`,
      })),
    };
  }
  return value;
}

export class PictureQueries {
  private open = new Map<Promise<unknown>, PanelQuery>();

  track<T>(method: PanelQuery, promise: Promise<T>): Promise<T> {
    this.open.set(promise, method);
    void promise.then(
      () => this.open.delete(promise),
      () => this.open.delete(promise),
    );
    return promise;
  }

  /** Waits up to `waitMs` for the requests asked so far, and returns the
   * sections whose answers had not come by then. */
  async settle(waitMs: number): Promise<string[]> {
    const asked = [...this.open.keys()];
    let timer: ReturnType<typeof setTimeout> | undefined;
    await Promise.race([
      Promise.allSettled(asked),
      new Promise(
        (resolve) => (timer = setTimeout(resolve, Math.max(0, waitMs))),
      ),
    ]);
    clearTimeout(timer);
    const late = asked.filter((promise) => this.open.has(promise));
    return [
      ...new Set(late.map((promise) => SECTION[this.open.get(promise)!])),
    ];
  }
}
