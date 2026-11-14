import { invoke, isTauri } from "@tauri-apps/api/core";
import type { NativeMethod, Provider } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import { Row, Section } from "../SettingsPrimitives";
import type { Translate } from "@agent-hud/ui/format";

/** A quota only the vendor's site shows: the user signs in to it in Agent
 * HUD's own window, and can sign out here. Agent HUD keeps only the session
 * the window ends with, on this computer, and opens the window only when the
 * user asks. */
export function AgentWebSignIn({
  t,
  source,
  action,
  perform,
  pending,
}: {
  t: Translate;
  source: Provider;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  perform: (work: () => Promise<unknown>) => void;
  pending: boolean;
}) {
  const web = source.accountStatus?.webSignIn;
  if (!web?.sites.length) return null;
  const ended = source.accountStatus?.failure === "signedOut";
  return (
    <Section
      title={t("Quota sign-in")}
      footer={t(
        "This quota is shown only on the vendor's site. Sign in there in Agent HUD's own window; Agent HUD keeps that sign-in on this computer and never reads your browser's. Google may refuse to sign in inside such a window; another way the site offers works.",
      )}
    >
      {web.sites.map((site) => (
        <Row
          key={site.host}
          title={site.name}
          subtitle={
            site.signedIn
              ? ended
                ? t("Sign-in ended; sign in again")
                : t("Signed in on {{host}}", { host: site.host })
              : t("Not signed in on {{host}}", { host: site.host })
          }
        >
          <div style={{ display: "flex", gap: 6 }}>
            {(!site.signedIn || ended) && (
              <Button
                size="sm"
                disabled={pending || !isTauri()}
                onClick={() =>
                  perform(() =>
                    invoke("web_sign_in", {
                      provider: web.provider,
                      host: site.host,
                    }),
                  )
                }
              >
                {t("Sign in")}
              </Button>
            )}
            {site.signedIn && (
              <Button
                size="sm"
                variant="outline"
                disabled={pending}
                onClick={() =>
                  perform(() =>
                    action("web_sign_out", {
                      provider: web.provider,
                      host: site.host,
                    }),
                  )
                }
              >
                {t("Sign out")}
              </Button>
            )}
          </div>
        </Row>
      ))}
    </Section>
  );
}
