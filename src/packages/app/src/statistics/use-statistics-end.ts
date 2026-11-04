import { useEffect, useState } from "react";

/** A quiet ledger still advances at the source's quarter-hour boundary and on focus. */
export function useStatisticsEnd(
  revision: number,
  refresh: number,
  active: boolean,
) {
  const [end, setEnd] = useState(() => Date.now());
  useEffect(() => {
    if (active) setEnd(Date.now());
  }, [revision, refresh, active]);
  useEffect(() => {
    if (!active) return;
    const quarter = 15 * 60_000;
    let timer: number;
    const advance = () => {
      const now = Date.now();
      setEnd(now);
      window.clearTimeout(timer);
      timer = window.setTimeout(advance, quarter - (now % quarter));
    };
    const now = Date.now();
    timer = window.setTimeout(advance, quarter - (now % quarter));
    window.addEventListener("focus", advance);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("focus", advance);
    };
  }, [active]);
  return end;
}
