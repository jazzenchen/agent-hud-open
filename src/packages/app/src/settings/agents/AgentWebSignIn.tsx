import { invoke, isTauri } from "@tauri-apps/api/core";
import type { NativeMethod, Provider } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import { Row, Section, type Text } from "../SettingsPrimitives";

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
  t: Text;
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
      title={t("Quota sign-in", "额度登录")}
      footer={t(
        "This quota is shown only on the vendor's site. Sign in there in Agent HUD's own window; Agent HUD keeps that sign-in on this computer and never reads your browser's.",
        "这项额度只在厂商网站上显示。在 Agent HUD 自己的窗口里登录后，Agent HUD 只在这台电脑上保存这次登录，不会读取浏览器里的登录。",
      )}
    >
      {web.sites.map((site) => (
        <Row
          key={site.host}
          title={site.name}
          subtitle={
            site.signedIn
              ? ended
                ? t("Sign-in ended; sign in again", "登录已失效，请重新登录")
                : t(`Signed in on ${site.host}`, `已在 ${site.host} 登录`)
              : t(`Not signed in on ${site.host}`, `未在 ${site.host} 登录`)
          }
        >
          <div style={{ display: "flex", gap: 6 }}>
            {(!site.signedIn || ended) && (
              <Button
                size="sm"
                disabled={pending || !isTauri()}
                onClick={() =>
                  void invoke("web_sign_in", {
                    provider: web.provider,
                    host: site.host,
                  })
                }
              >
                {t("Sign in", "登录")}
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
                {t("Sign out", "退出登录")}
              </Button>
            )}
          </div>
        </Row>
      ))}
    </Section>
  );
}
