import { useState } from "react";

/** One save at a time on an agent page. Its failure shows at the top of the
 * page, or through `report` where the caller shows it beside its own buttons
 * (API credentials, at the bottom of the page, where the top is out of view). */
export function usePerform() {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function perform(
    job: () => Promise<unknown>,
    report: (message: string) => void = setError,
  ) {
    setPending(true);
    setError(null);
    try {
      await job();
      return true;
    } catch (failure) {
      report(String(failure));
      return false;
    } finally {
      setPending(false);
    }
  }
  return { pending, error, setError, perform };
}

export type Perform = ReturnType<typeof usePerform>["perform"];
