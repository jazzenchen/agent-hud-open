// The browser extension, on Experimental: it picks an element or an area of
// a page and sends it through the inspector. Says where it last
// reached Agent HUD from, turning as the extension first speaks, and opens
// its Chrome Web Store page in a browser that can add it (Chrome, or the
// system's browser when that is Chromium), not in Safari.
import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  /** The window is on screen: the status is read again as it comes back,
   * and follows the extension while it stays. */
  active: boolean;
  t: Translate;
}) {
  const [status, setStatus] = useState<BrowserStatus | null>(null);
  const [opening, setOpening] = useState(false);
  const openStore = () => {
    setOpening(true);
    void invoke("inspect_request", { method: "open_store", params: {} })
      .catch((error) => console.warn("The store page could not open", error))
      .finally(() => setOpening(false));
  };
  useEffect(() => {
    if (!active || !isTauri()) return;
    // Subscribed before the read, so a hello between the two isn't missed.
    const subscription = listen<BrowserStatus>(
      "inspect:browser",
      ({ payload }) => setStatus(payload),
    );
    invoke<BrowserStatus>("inspect_request", {
      method: "browser",
      params: {},
    }).then(setStatus, () => {});
    return () => void subscription.then((unlisten) => unlisten());
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
        <LinkButton onClick={openStore} disabled={opening}>
          {t("Chrome Web Store")}
        </LinkButton>
      )}
    </Row>
  );
}
