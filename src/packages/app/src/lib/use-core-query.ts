import { useEffect, useRef, useState } from "react";
import type { NativeMethod } from "@agent-hud/protocol";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/** Core's answer to `method` with `params` while `active`, asked again when
 * `refresh` changes. Only the latest request publishes; a failure keeps the
 * last answer, and its error stands only while the params are the same. */
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
  const key = JSON.stringify(params);
  useEffect(() => {
    if (!active) return;
    let current = true;
    setFetching(true);
    void actionRef
      .current(method, JSON.parse(key))
      .then((value) => {
        if (!current) return;
        setData(value as T);
        setFailure(null);
      })
      .catch((error: unknown) => {
        if (current)
          setFailure({
            key,
            message: error instanceof Error ? error.message : String(error),
          });
      })
      .finally(() => {
        if (current) setFetching(false);
      });
    return () => {
      current = false;
    };
  }, [method, key, active, refresh]);
  return {
    data,
    error: failure?.key === key ? failure.message : null,
    fetching,
  };
}
