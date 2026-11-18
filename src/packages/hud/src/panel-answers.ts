/** An answer to a panel query: the query it answers (`key`: its parameters
 * and Core's revision; `base`: its parameters alone) and the order in which
 * that query was asked. */
export interface PanelAnswer<T> {
  key: string;
  base: string;
  order: number;
  data: T | null;
  error: string | null;
}

export const noAnswer: PanelAnswer<never> = {
  key: "",
  base: "",
  order: 0,
  data: null,
  error: null,
};

/** What a panel shows for the query `key`: its answer, else, until that
 * comes, the last answer to the same parameters at an earlier revision:
 * `last`, the last one any panel had, else its own. Core moves its revision
 * on with every read that changes usage, every 2 s while logs are indexed; a
 * section that showed only the current revision's answer would go blank each
 * time, and stay blank while answers take longer than that. Two panels asking
 * the same question show the same answer, whichever of them asked the query
 * that answered. */
export function shownAnswer<T>(
  answer: PanelAnswer<T>,
  key: string,
  base: string,
  last?: T,
): { data: T | null; error: string | null } {
  if (answer.key === key) return { data: answer.data, error: answer.error };
  return {
    data: last ?? (answer.base === base ? answer.data : null),
    error: null,
  };
}

/** The answer kept when `next` comes: an answer to a query asked later
 * replaces it; one to a query asked earlier that comes late does not. */
export function settled<T>(
  current: PanelAnswer<T>,
  next: PanelAnswer<T>,
): PanelAnswer<T> {
  return next.order < current.order ? current : next;
}
