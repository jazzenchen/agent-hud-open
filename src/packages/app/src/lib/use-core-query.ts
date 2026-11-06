import { useEffect, useRef, useState } from "react";
import type { NativeMethod } from "@agent-hud/protocol";

/** The least time between a request and the next for newer data. */
const SPACING_MS = 1_500;

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/** Core's answer to `method` with `params` while `active`, asked again when
 * `refresh` changes. An answer to other params is dropped. A new `refresh`
 * while a request for the same params is out does not drop it: its answer
 * shows and the next request leaves as it lands. So while the history is
 * read and the ledger changes every few seconds, the page fills in as the
 * reading goes, rather than only once it stops. A failure keeps the last
 * answer, and its error stands only while the params are the same. */
export function useCoreQuery<T>(
  action: Action,
  method: NativeMethod,
  params: object,
  active: boolean,
  refresh?: unknown,
) {
  const actionRef = useRef(action);
  actionRef.current = action;
  const [data, setData] = useState<T | null>(null);
  const [failure, setFailure] = useState<{
    key: string;
    message: string;
  } | null>(null);
  const [fetching, setFetching] = useState(active);
  // Another round once a request lands that a newer refresh outdated.
  const [round, setRound] = useState(0);
  const key = JSON.stringify(params);
  const wanted = useRef({ method, key, refresh, active });
  wanted.current = { method, key, refresh, active };
  const out = useRef<{ method: string; key: string; refresh: unknown } | null>(
    null,
  );
  useEffect(() => {
    if (!active) return;
    if (out.current?.method === method && out.current.key === key) return;
    const request = { method, key, refresh };
    out.current = request;
    const same = () =>
      wanted.current.method === request.method &&
      wanted.current.key === request.key;
    const started = Date.now();
    setFetching(true);
    void actionRef
      .current(method, JSON.parse(key))
      .then((value) => {
        if (!same()) return;
        setData(value as T);
        setFailure(null);
      })
      .catch((error: unknown) => {
        if (!same()) return;
        setFailure({
          key,
          message: error instanceof Error ? error.message : String(error),
        });
      })
      .finally(() => {
        if (out.current === request) out.current = null;
        if (!same()) return;
        // The next round leaves at least 1.5 s after this one did, so a
        // ledger read in a hurry is not asked again on every pass.
        if (wanted.current.active && wanted.current.refresh !== request.refresh)
          window.setTimeout(
            () => setRound((value) => value + 1),
            Math.max(0, SPACING_MS - (Date.now() - started)),
          );
        else setFetching(false);
      });
  }, [method, key, active, refresh, round]);
  return {
    data,
    error: failure?.key === key ? failure.message : null,
    fetching,
  };
}
