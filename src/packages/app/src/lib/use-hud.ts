import { useCallback, useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { NativeMethod, Snapshot } from "./types";

async function nativeRequest(
  method: NativeMethod,
  params: Record<string, unknown> = {},
) {
  if (isTauri()) return invoke("native_request", { method, params });
  const response = await fetch("/__bridge", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ method, params }),
  });
  if (!response.ok) {
    const body = await response.json().catch(() => null);
    throw new Error(
      body?.error?.message ??
        body?.error ??
        "Live data requires the desktop app or the local development bridge.",
    );
  }
  const body = await response.json();
  if (body.error) throw new Error(body.error.message ?? String(body.error));
  return body.result;
}

export function useHUD({ active = true }: { active?: boolean } = {}) {
  // Native collection belongs to the application service, never a window's timer.
  // `--demo` runs the same service on sample sources, so it reads the same way.
  const ownsPolling = !isTauri();
  const [demo, updateDemo] = useState<boolean | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const boot = async () => {
      try {
        const mode = isTauri()
          ? await invoke<{ demo: boolean }>("startup_mode")
          : { demo: false };
        updateDemo(mode.demo);
      } catch (failure) {
        setError(String(failure));
        setLoading(false);
      }
    };
    void boot();
  }, []);
  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const next = (await nativeRequest("refresh")) as Snapshot;
      if (!isTauri()) {
        setSnapshot(next);
        setError(null);
      }
    } catch (err) {
      if (!isTauri())
        setError(String(err instanceof Error ? err.message : err));
    } finally {
      setLoading(false);
    }
  }, []);
  useEffect(() => {
    if (demo === null || ownsPolling || !active) return;
    let disposed = false;
    let revision = 0;
    let latestClock: string | null = null;
    const receive = (value: Snapshot) => {
      if (disposed) return;
      revision += 1;
      setSnapshot(value);
      setLoading(false);
      setError(null);
    };
    const target = {
      kind: "WebviewWindow" as const,
      label: getCurrentWebviewWindow().label,
    };
    const subscription = listen<string>(
      "hud:snapshot-json",
      ({ payload }) => receive(JSON.parse(payload) as Snapshot),
      { target },
    );
    const failures = listen<string>(
      "hud:snapshot-error",
      ({ payload }) => {
        if (!disposed) {
          revision += 1;
          setError(payload);
          setLoading(false);
        }
      },
      { target },
    );
    const clock = listen<string>(
      "hud:clock",
      ({ payload }) => {
        if (disposed) return;
        latestClock = payload;
        setSnapshot((previous) =>
          previous && previous.generatedAt !== payload
            ? { ...previous, generatedAt: payload }
            : previous,
        );
      },
      { target },
    );
    void Promise.all([subscription, failures, clock])
      .then(async () => {
        if (disposed) return;
        const before = revision;
        const cached = await invoke<{
          snapshot: Snapshot | null;
          error: string | null;
        }>("cached_snapshot");
        if (!disposed && before === revision) {
          if (cached.snapshot) {
            const value = cached.snapshot;
            if (
              latestClock &&
              Date.parse(latestClock) > Date.parse(value.generatedAt)
            )
              value.generatedAt = latestClock;
            receive(value);
          }
          setError(cached.error);
          if (cached.snapshot || cached.error) setLoading(false);
        }
      })
      .catch((failure) => {
        if (!disposed) {
          setError(String(failure));
          setLoading(false);
        }
      });
    return () => {
      disposed = true;
      void subscription.then((unlisten) => unlisten());
      void failures.then((unlisten) => unlisten());
      void clock.then((unlisten) => unlisten());
    };
  }, [demo, ownsPolling, active]);
  useEffect(() => {
    if (demo === null || !ownsPolling || !active) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const next = (await nativeRequest("snapshot")) as Snapshot;
        if (!disposed) {
          setSnapshot(next);
          setError(null);
        }
      } catch (err) {
        if (!disposed)
          setError(String(err instanceof Error ? err.message : err));
      } finally {
        if (!disposed) {
          setLoading(false);
          timer = setTimeout(poll, 5000);
        }
      }
    };
    void poll();
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [demo, ownsPolling, active]);
  const act = useCallback(
    async (method: NativeMethod, params: Record<string, unknown> = {}) => {
      const result = await nativeRequest(method, params);
      if (
        !isTauri() &&
        result &&
        typeof result === "object" &&
        "schemaVersion" in result
      )
        setSnapshot(result as Snapshot);
      return result;
    },
    [],
  );
  useEffect(() => {
    const listener = () => {
      void refresh();
    };
    window.addEventListener("agenthud:refresh", listener);
    return () => window.removeEventListener("agenthud:refresh", listener);
  }, [refresh]);
  return { snapshot, loading, error, demo: demo === true, refresh, act };
}
