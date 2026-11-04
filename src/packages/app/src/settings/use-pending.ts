import { useState } from "react";

/** A Settings change in progress: `pending` while `run`'s job runs, and its
 * failure as `error`. `run` answers whether the job succeeded. */
export function usePending() {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function run(job: () => Promise<unknown>) {
    setPending(true);
    setError(null);
    try {
      await job();
      return true;
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
      return false;
    } finally {
      setPending(false);
    }
  }
  return { pending, error, setError, run };
}
