// What a collapsed alert says: who it is about and its status, by kind.
import type { Approval, IslandEvent } from "@agent-hud/protocol";
import { systemCalendar } from "@agent-hud/ui/format";
import { agentVendorName } from "../model";
import { clientDisplayName, projectName } from "../IslandNews";
import { quotaAlertCopy } from "./quota-alert-copy";

/** What a quota reminder says beside its agent (Open's QuotaAlertCopy.compactTitle). */
export function compactQuotaStatus(event: IslandEvent, zh = false): string {
  return quotaAlertCopy(event, {
    language: zh ? "zh" : "en",
    calendar: systemCalendar(navigator.language),
  }).compactTitle;
}

export function compactAlertLabels(
  request: Approval | undefined,
  event: IslandEvent | undefined,
  requestCount: number,
  zh: boolean,
): { who: string; status: string; badge?: string } {
  if (request) {
    const who =
      projectName(request.cwd) ??
      request.vendor ??
      clientDisplayName(request.source);
    const status =
      request.kind === "question"
        ? zh
          ? "待回答"
          : "Question"
        : request.kind === "plan"
          ? zh
            ? "计划待审"
            : "Plan"
          : zh
            ? "待批准"
            : "Waiting";
    return {
      who,
      status,
      badge: requestCount > 1 ? `×${requestCount}` : undefined,
    };
  }
  if (!event) return { who: "", status: "" };
  if (event.kind === "completion") {
    return {
      who: clientDisplayName(event.source),
      status: zh ? "有新回复" : "New reply",
    };
  }
  if (event.kind === "sign-in-needed") {
    return {
      who: event.title ?? agentVendorName(event.source),
      status: zh ? "需要重新登录" : "Sign in again",
    };
  }
  if (event.kind === "hooks-review-needed") {
    return {
      who: event.title ?? agentVendorName(event.source),
      status: zh ? "回调待允许" : "Allow its hooks",
    };
  }
  if (event.kind === "reset-credits") {
    const added = event.added ?? 1;
    return {
      who: agentVendorName(event.source),
      status: zh
        ? `+${added} 次重置`
        : `+${added} reset${added === 1 ? "" : "s"}`,
    };
  }
  return {
    who: agentVendorName(event.source),
    status: compactQuotaStatus(event, zh),
  };
}
