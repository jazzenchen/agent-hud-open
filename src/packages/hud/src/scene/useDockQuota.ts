import { useEffect, useRef, useState } from "react";
import type { Quota } from "@agent-hud/protocol";
import {
  DOCK_QUOTA_NORMAL_MS,
  DOCK_QUOTA_WARNING_MS,
  dockQuotaWindows,
  quotaAlertCycle,
  takeQuotaAlert,
} from "../usage-meter/dock-quota";

export function useDockQuota(
  quotas: readonly Quota[],
  threshold: number,
  enabled: boolean,
) {
  const { primary, warning, warningWindows } = enabled
    ? dockQuotaWindows(quotas, threshold)
    : {};
  const warningId = warning?.id;
  const [shownWarning, setShownWarning] = useState<string>();
  const [alertId, setAlertId] = useState<string>();
  const fallbackSeen = useRef(new Map<string, string>());
  const eligible = JSON.stringify(
    warningWindows?.map(({ id, resetAt }) => ({ id, resetAt })) ?? [],
  );
  useEffect(() => {
    const windows: { id: string; resetAt: string | null }[] =
      JSON.parse(eligible);
    const fresh = windows.filter((row) => {
      const cycle = quotaAlertCycle(row);
      if (fallbackSeen.current.get(row.id) === cycle) return false;
      fallbackSeen.current.set(row.id, cycle);
      try {
        return takeQuotaAlert(window.localStorage, row);
      } catch {
        return true;
      }
    });
    if (!fresh.length) return;
    setAlertId(fresh.find((row) => row.id === warningId)?.id ?? fresh[0].id);
  }, [eligible, warningId]);
  useEffect(() => {
    if (!alertId) return;
    const timeout = setTimeout(
      () => setAlertId(undefined),
      DOCK_QUOTA_WARNING_MS,
    );
    return () => clearTimeout(timeout);
  }, [alertId]);
  useEffect(() => {
    setShownWarning(undefined);
    if (warningId == null) return;
    let timer: ReturnType<typeof setTimeout>;
    const start = () => {
      setShownWarning(warningId);
      timer = setTimeout(() => {
        setShownWarning(undefined);
        timer = setTimeout(start, DOCK_QUOTA_NORMAL_MS);
      }, DOCK_QUOTA_WARNING_MS);
    };
    timer = setTimeout(start, DOCK_QUOTA_NORMAL_MS);
    return () => clearTimeout(timer);
  }, [warningId]);
  const alert = warningWindows?.find((row) => row.id === alertId);
  const quota =
    alert ?? (warning && shownWarning === warning.id ? warning : primary);
  return { quota, alert };
}
