// What a collapsed alert says: who it is about and its status, by kind.
import type { Approval, IslandEvent } from "@agent-hud/protocol";
import {
  systemCalendar,
  translator,
  type Language,
} from "@agent-hud/ui/format";
import { agentVendorName } from "../model";
import { clientDisplayName } from "../IslandNews";
import { projectName } from "../project-path";
import { quotaAlertCopy } from "./quota-alert-copy";

/** What a quota reminder says beside its agent (Open's QuotaAlertCopy.compactTitle). */
export function compactQuotaStatus(
  event: IslandEvent,
  language: Language = "en",
): string {
  return quotaAlertCopy(event, {
    language,
    calendar: systemCalendar(navigator.language),
  }).compactTitle;
}

export function compactAlertLabels(
  request: Approval | undefined,
  event: IslandEvent | undefined,
  requestCount: number,
  language: Language,
): { who: string; status: string; badge?: string } {
  const t = translator(language);
  if (request) {
    const who =
      projectName(request.cwd) ??
      request.vendor ??
      clientDisplayName(request.source);
    const status =
      request.kind === "question"
        ? t("Question")
        : request.kind === "plan"
          ? t("Plan")
          : t("Waiting");
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
      status: t("New reply"),
    };
  }
  if (event.kind === "sign-in-needed") {
    return {
      who: event.title ?? agentVendorName(event.source),
      status: t("Sign in again"),
    };
  }
  if (event.kind === "hooks-review-needed") {
    return {
      who: event.title ?? agentVendorName(event.source),
      status: t("Allow its hooks"),
    };
  }
  if (event.kind === "reset-credits") {
    const added = event.added ?? 1;
    return {
      who: agentVendorName(event.source),
      status: t("+{{count}} resets", { count: added }),
    };
  }
  return {
    who: agentVendorName(event.source),
    status: compactQuotaStatus(event, language),
  };
}
