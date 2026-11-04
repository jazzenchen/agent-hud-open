/** The panel's requests that a picture waits for: the token chart's statistics
 * and the sessions list. */
export type PanelQuery = "statistics" | "sessions";

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
};

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
