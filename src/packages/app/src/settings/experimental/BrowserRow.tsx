// The browser extension, on Experimental: it picks an element or an area of
// a page and sends it through the inspector. Says where it last
// reached Agent HUD from, and links its Chrome Web Store page once it has
// one.
import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { dateFormat, type Translate } from "@agent-hud/ui/format";
import { LinkButton } from "../SettingsControls";
import { Pill, Row } from "../SettingsPrimitives";

interface BrowserStatus {
  seen: { browser: string; at: number } | null;
  storeUrl: string | null;
}

export function BrowserRow({
  active,
  t,
}: {
  /** The window is on screen: the status is read again as it comes back. */
  active: boolean;
  t: Translate;
}) {
  const [status, setStatus] = useState<BrowserStatus | null>(null);
  useEffect(() => {
    if (!active || !isTauri()) return;
    invoke<BrowserStatus>("inspect_request", {
      method: "browser",
      params: {},
    }).then(setStatus, () => {});
  }, [active]);
  const seen = status?.seen;
  return (
    <Row
      title={t("Browser extension")}
      subtitle={t(
        "Pick an element or an area of a page and send it through the inspector.",
      )}
      detail={
        seen ? (
          <Pill tone="green" dot>
            {t("Reached Agent HUD from {{browser}}, {{when}}", {
              browser: seen.browser,
              when: dateFormat(undefined, {
                month: "short",
                day: "numeric",
                hour: "numeric",
                minute: "2-digit",
              }).format(new Date(seen.at)),
            })}
          </Pill>
        ) : (
          <Pill>{t("Not connected yet")}</Pill>
        )
      }
    >
      {status?.storeUrl && (
        <LinkButton href={status.storeUrl}>{t("Chrome Web Store")}</LinkButton>
      )}
    </Row>
  );
}
