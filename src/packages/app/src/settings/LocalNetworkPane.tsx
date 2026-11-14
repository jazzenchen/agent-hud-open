import { useCallback, useEffect, useState } from "react";
import { locale, type Language, type Translate } from "@agent-hud/ui/format";
import { useFormat, useT } from "@agent-hud/ui/format-context";
import {
  Footnote,
  Notice,
  Page,
  Pill,
  Row,
  Section,
  type SettingsPaneProps,
} from "./SettingsPrimitives";
import {
  LinkButton,
  SmallButton,
  ToggleRow,
  Unavailable,
} from "./SettingsControls";
import { usePending } from "./use-pending";

interface Peer {
  id: string;
  name: string | null;
  version?: string | null;
  address: string;
  online: boolean;
  lastSeenMs: number;
}
interface ShareStatus {
  supported: boolean;
  on: boolean;
  enabled: boolean;
  endpoint?: string | null;
  alternateEndpoints?: string[];
  pairedClients: number;
  peers: Peer[];
  error?: string | null;
}
interface Invitation {
  uri: string;
  image: string;
  invitation: { expiresAtMs: number };
}

/** A clock time in the app's language, not the system's. */
const clock = (ms: number, language: Language) =>
  new Date(ms).toLocaleTimeString(locale(language));

/**
 * The Local Network page: paired phones read this computer's quota readings
 * and token usage over the local network (or the owner's Tailscale), and
 * nothing of its sessions. A phone pairs by scanning a single-use code.
 */
export function LocalNetworkPane({ action, active = true }: SettingsPaneProps) {
  const t = useT();
  const { language } = useFormat();
  const [status, setStatus] = useState<ShareStatus | null>(null);
  const [invitation, setInvitation] = useState<Invitation | null>(null);
  const [copied, setCopied] = useState(false);
  const { pending, error, setError, run } = usePending();
  const share = useCallback(
    (method: string, params: Record<string, unknown> = {}) =>
      action("phone_share", { method, params }),
    [action],
  );
  const read = useCallback(
    () =>
      share("status")
        .then((value) => setStatus(value as ShareStatus))
        .catch((failure) => setError(String(failure))),
    [share],
  );
  // Online state comes from the phones' own connections: read it again
  // while the page is on screen.
  useEffect(() => {
    if (!active) return;
    void read();
    const timer = window.setInterval(() => void read(), 5_000);
    return () => window.clearInterval(timer);
  }, [active, read]);
  const perform = async (job: () => Promise<unknown>) => {
    await run(job);
    void read();
  };
  if (!status)
    return (
      <Page>
        <Footnote>{t("Reading local sharing…")}</Footnote>
        {error && <Notice tone="error">{error}</Notice>}
      </Page>
    );
  const addresses = status.endpoint
    ? [
        status.endpoint,
        ...(status.alternateEndpoints ?? []).map(
          (address) => `Tailscale ${address}`,
        ),
      ].join(" · ")
    : status.on
      ? t("No usable network address")
      : t("Sharing is off");
  const create = (
    <SmallButton
      disabled={pending || !status.enabled}
      onClick={() =>
        void perform(async () => {
          setInvitation((await share("pair_invitation")) as Invitation);
          setCopied(false);
        })
      }
    >
      {invitation ? t("New invitation") : t("Create invitation")}
    </SmallButton>
  );
  const problem = error ?? status.error;
  const unsupported = t(
    "Sharing with phones is not available on this system yet.",
  );
  return (
    <Page>
      <Section>
        <ToggleRow
          title={t("Share with paired phones")}
          subtitle={t("Paired iPhones read quota and token usage only.")}
          checked={status.on}
          disabled={!status.supported || pending}
          reason={status.supported ? undefined : unsupported}
          change={(enabled) => {
            setInvitation(null);
            void perform(() => share("set_enabled", { enabled }));
          }}
        />
        <Row title={t("Connection")} detail={addresses} />
      </Section>
      <Section
        title={t("Pair a device")}
        footer={t(
          "An invitation works once and expires after five minutes. Paired devices can only read.",
        )}
      >
        <Row
          title={t("QR invitation")}
          subtitle={
            invitation ? t("Scan it with Agent HUD on iPhone.") : undefined
          }
          below={
            invitation && (
              // A white ground whatever the theme, so a phone camera reads it.
              <div style={{ display: "grid", justifyItems: "center" }}>
                <img
                  src={invitation.image}
                  width={200}
                  height={200}
                  alt={t("Single-use Agent HUD pairing invitation")}
                  style={{ background: "white", padding: 10, borderRadius: 8 }}
                />
              </div>
            )
          }
        >
          {!status.enabled && !pending ? (
            <Unavailable
              reason={
                !status.supported
                  ? unsupported
                  : status.on
                    ? t("Sharing isn’t running.")
                    : t("Turn on sharing first.")
              }
            >
              {create}
            </Unavailable>
          ) : (
            create
          )}
        </Row>
        {invitation && (
          <Row
            title={t("Pairing link")}
            subtitle={t("Expires at {{time}}", {
              time: clock(invitation.invitation.expiresAtMs, language),
            })}
            detail={invitation.uri}
          >
            <LinkButton
              onClick={() =>
                void navigator.clipboard
                  .writeText(invitation.uri)
                  .then(() => setCopied(true))
                  .catch(() => setCopied(false))
              }
            >
              {copied ? t("Copied") : t("Copy link")}
            </LinkButton>
          </Row>
        )}
      </Section>
      <Section
        title={t("Devices")}
        subtitle={pairedSummary(status.pairedClients, t)}
        action={
          status.pairedClients > 0 && (
            <LinkButton
              tone="destructive"
              disabled={pending}
              onClick={() => {
                setInvitation(null);
                void perform(() => share("revoke_clients"));
              }}
            >
              {t("Revoke paired devices")}
            </LinkButton>
          )
        }
      >
        {status.peers.length ? (
          status.peers.map((peer) => (
            <Row
              key={peer.id}
              title={
                (peer.name ?? t("Paired device")) +
                (peer.version ? ` · ${peer.version}` : "")
              }
              detail={peer.address}
            >
              {peer.online ? (
                <Pill tone="green" dot>
                  {t("Online")}
                </Pill>
              ) : (
                <Pill>
                  {t("Last seen {{time}}", {
                    time: clock(peer.lastSeenMs, language),
                  })}
                </Pill>
              )}
            </Row>
          ))
        ) : (
          <Row
            muted
            title={t("No device has connected since Agent HUD started.")}
          />
        )}
      </Section>
      {problem && <Notice tone="error">{problem}</Notice>}
    </Page>
  );
}

/** "2 paired devices": the saved pairings, not this launch's connections,
 * which the rows below list. */
function pairedSummary(grants: number, t: Translate) {
  return t("{{count}} paired devices", { count: grants });
}
