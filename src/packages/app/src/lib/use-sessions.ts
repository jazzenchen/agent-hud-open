import { useCallback, useEffect, useRef, useState } from "react";
import type {
  NativeMethod,
  Session,
  SessionDetail,
  SessionsQuery,
  SessionsResponse,
} from "./types";
import { withMessagePage } from "./sessions";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;
type Page = SessionsResponse & { key: string };

function response(value: unknown): SessionsResponse {
  const result = value as SessionsResponse;
  if (
    !result ||
    result.kind !== "sessions" ||
    !Number.isFinite(result.revision) ||
    !Array.isArray(result.items) ||
    !Array.isArray(result.sources) ||
    !Array.isArray(result.projects) ||
    !Number.isFinite(result.total) ||
    (result.nextOffset !== null && !Number.isInteger(result.nextOffset))
  )
    throw new Error("The runtime returned an unsupported sessions response.");
  return result;
}
const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

/** Historical rows are local queries, owned only by the mounted Sessions page. */
export function useSessions(
  query: Omit<SessionsQuery, "offset" | "limit">,
  revision: number,
  active: boolean,
  action: Action,
) {
  const actionRef = useRef(action);
  actionRef.current = action;
  const sequence = useRef(0);
  const morePending = useRef(false);
  const [page, setPage] = useState<Page | null>(null);
  const current = useRef(page);
  current.current = page;
  const prevSearchRef = useRef(query.search);
  const [fetching, setFetching] = useState(active);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  const key = JSON.stringify(query);
  const retry = useCallback(() => setRefresh((value) => value + 1), []);
  useEffect(() => {
    const id = ++sequence.current;
    morePending.current = false;
    setLoadingMore(false);
    const searchChanged = prevSearchRef.current !== query.search;
    prevSearchRef.current = query.search;
    if (!active) {
      setFetching(false);
      return;
    }
    setFetching(true);
    setError(null);
    const run = () => {
      const limit = Math.max(
        100,
        current.current?.key === key ? current.current.items.length : 100,
      );
      void actionRef
        .current("sessions", { ...JSON.parse(key), offset: 0, limit })
        .then(response)
        .then((result) => {
          if (sequence.current !== id) return;
          if (result.revision < revision)
            throw new Error("Session history is catching up. Try again.");
          setPage({ ...result, key });
          setError(null);
        })
        .catch((failure: unknown) => {
          if (sequence.current === id) setError(message(failure));
        })
        .finally(() => {
          if (sequence.current === id) setFetching(false);
        });
    };
    if (searchChanged) {
      const timer = window.setTimeout(run, 80);
      return () => {
        window.clearTimeout(timer);
        sequence.current += 1;
      };
    }
    run();
    return () => {
      sequence.current += 1;
    };
  }, [key, query.search, revision, active, refresh]);

  const loading = !page && fetching;
  const updating = Boolean(page && fetching);

  const loadMore = useCallback(async () => {
    const before = current.current;
    if (
      !active ||
      fetching ||
      morePending.current ||
      !before ||
      before.key !== key ||
      before.revision < revision ||
      before.nextOffset === null
    )
      return;
    const id = sequence.current;
    morePending.current = true;
    setLoadingMore(true);
    setError(null);
    try {
      const result = response(
        await actionRef.current("sessions", {
          ...JSON.parse(key),
          offset: before.nextOffset,
          limit: 100,
        }),
      );
      if (sequence.current !== id) return;
      if (result.revision !== before.revision) {
        // A page from another version can never be appended to this list.
        retry();
        return;
      }
      setPage((previous) =>
        previous?.key === key && previous.revision === result.revision
          ? { ...result, key, items: [...previous.items, ...result.items] }
          : previous,
      );
    } catch (failure) {
      if (sequence.current === id) setError(message(failure));
    } finally {
      if (sequence.current === id) {
        morePending.current = false;
        setLoadingMore(false);
      }
    }
  }, [active, key, revision, fetching, retry]);
  return { page, loading, updating, loadingMore, error, loadMore, retry };
}

/** A collector clock or an unrelated session must not re-decode a cold detail. */
export function sessionFactsKey(session: Session | null | undefined) {
  if (!session) return null;
  return JSON.stringify([
    session.id,
    session.logPath,
    session.updatedAt,
    session.tokens,
    session.cost,
    session.pricedCost,
    session.unpricedTokens,
    session.contextPercent,
    session.parentId,
  ]);
}

export function useSessionDetail(
  id: string | null,
  observed: Session | null | undefined,
  active: boolean,
  action: Action,
  /** A picked turn's id: Core then adds that turn's calls. */
  turn: string | null = null,
) {
  const actionRef = useRef(action);
  actionRef.current = action;
  const sequence = useRef(0);
  const previousId = useRef<string | null>(null);
  const successful = useRef<{
    id: string;
    facts: string | null;
    refresh: number;
    turn: string | null;
  } | null>(null);
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const current = useRef(detail);
  current.current = detail;
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  const facts = sessionFactsKey(observed);
  const retry = useCallback(() => setRefresh((value) => value + 1), []);
  useEffect(() => {
    const version = ++sequence.current;
    if (previousId.current !== id) {
      previousId.current = id;
      current.current = null;
      setDetail(null);
      setError(null);
    }
    if (!id || !active) {
      setLoading(false);
      return;
    }
    const saved = successful.current;
    if (
      current.current?.session.id === id &&
      saved?.id === id &&
      saved.facts === facts &&
      saved.refresh === refresh &&
      saved.turn === turn
    ) {
      setLoading(false);
      return;
    }
    if (!current.current) setLoading(true);
    setError(null);
    void actionRef
      .current("session_detail", turn ? { id, turn } : { id })
      .then((value) => {
        if (sequence.current !== version) return;
        const next = value as SessionDetail;
        if (next?.session?.id !== id || !Array.isArray(next.turns))
          throw new Error(
            "The runtime returned an unsupported session detail.",
          );
        successful.current = { id, facts, refresh, turn };
        setDetail(next);
      })
      .catch((failure: unknown) => {
        if (sequence.current === version) setError(message(failure));
      })
      .finally(() => {
        if (sequence.current === version) setLoading(false);
      });
    return () => {
      sequence.current += 1;
    };
  }, [id, facts, active, refresh, turn]);
  const [loadingMessages, setLoadingMessages] = useState(false);
  const moreMessages = useRef(false);
  const loadMessages = useCallback(async () => {
    const before = current.current;
    if (
      !active ||
      !id ||
      !before ||
      before.session.id !== id ||
      before.nextMessageCursor == null ||
      moreMessages.current
    )
      return;
    const version = sequence.current;
    moreMessages.current = true;
    setLoadingMessages(true);
    try {
      const next = (await actionRef.current("session_detail", {
        id,
        messageCursor: before.nextMessageCursor,
        messageLimit: 100,
      })) as SessionDetail;
      if (sequence.current !== version) return;
      setDetail((previous) =>
        previous?.session.id === id
          ? withMessagePage(previous, next)
          : previous,
      );
    } catch (failure) {
      if (sequence.current === version) setError(message(failure));
    } finally {
      moreMessages.current = false;
      setLoadingMessages(false);
    }
  }, [active, id]);
  return {
    detail: detail?.session.id === id ? detail : null,
    loading,
    error,
    setError,
    loadMessages,
    loadingMessages,
    retry,
  };
}
