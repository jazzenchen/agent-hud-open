import { useCallback, useEffect, useState } from "react";
import {
  Footnote,
  LinkButton,
  Notice,
  Page,
  Pill,
  Row,
  Section,
  SmallButton,
  ToggleRow,
  Unavailable,
  useText,
  type SettingsPaneProps,
  type Text,
} from "./SettingsPrimitives";

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
const clock = (ms: number, t: Text) =>
  new Date(ms).toLocaleTimeString(t("en-US", "zh-CN"));

/**
 * The Local Network page: paired phones read this computer's quota readings
 * and token usage over the local network (or the owner's Tailscale), and
 * nothing of its sessions. A phone pairs by scanning a single-use code.
 */
export function LocalNetworkPane({
  snapshot,
  action,
  active = true,
}: SettingsPaneProps) {
  const t = useText(snapshot);
  const [status, setStatus] = useState<ShareStatus | null>(null);
  const [invitation, setInvitation] = useState<Invitation | null>(null);
  const [copied, setCopied] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
    setPending(true);
    setError(null);
    try {
      await job();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setPending(false);
      void read();
    }
  };
  if (!status)
    return (
      <Page>
        <Footnote>
          {t("Reading local sharing…", "正在读取局域网设置…")}
        </Footnote>
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
      ? t("No usable network address", "没有可用的网络地址")
      : t("Sharing is off", "共享已关闭");
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
      {invitation
        ? t("New invitation", "重新创建")
        : t("Create invitation", "创建邀请")}
    </SmallButton>
  );
  const problem = error ?? status.error;
  const unsupported = t(
    "Sharing with phones is not available on this system yet.",
    "这个系统暂不支持与手机共享。",
  );
  return (
    <Page>
      <Section>
        <ToggleRow
          title={t("Share with paired phones", "与已配对手机共享")}
          subtitle={t(
            "Paired iPhones read quota and token usage only.",
            "已配对的 iPhone 只能查看额度和 token 用量。",
          )}
          checked={status.on}
          disabled={!status.supported || pending}
          reason={status.supported ? undefined : unsupported}
          change={(enabled) => {
            setInvitation(null);
            void perform(() => share("set_enabled", { enabled }));
          }}
        />
        <Row title={t("Connection", "连接")} detail={addresses} />
      </Section>
      <Section
        title={t("Pair a device", "配对设备")}
        footer={t(
          "An invitation works once and expires after five minutes. Paired devices can only read.",
          "邀请仅可使用一次，五分钟后失效。配对设备只能查看。",
        )}
      >
        <Row
          title={t("QR invitation", "二维码邀请")}
          subtitle={
            invitation
              ? t(
                  "Scan it with Agent HUD on iPhone.",
                  "用 iPhone 上的 Agent HUD 扫描。",
                )
              : undefined
          }
          below={
            invitation && (
              // A white ground whatever the theme, so a phone camera reads it.
              <div style={{ display: "grid", justifyItems: "center" }}>
                <img
                  src={invitation.image}
                  width={200}
                  height={200}
                  alt={t(
                    "Single-use Agent HUD pairing invitation",
                    "一次性 Agent HUD 配对邀请",
                  )}
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
                    ? t("Sharing isn’t running.", "共享未在运行。")
                    : t("Turn on sharing first.", "请先打开共享。")
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
            title={t("Pairing link", "配对链接")}
            subtitle={t(
              `Expires at ${clock(invitation.invitation.expiresAtMs, t)}`,
              `${clock(invitation.invitation.expiresAtMs, t)} 失效`,
            )}
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
              {copied ? t("Copied", "已复制") : t("Copy link", "复制链接")}
            </LinkButton>
          </Row>
        )}
      </Section>
      <Section
        title={t("Devices", "设备")}
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
              {t("Revoke paired devices", "撤销已配对设备")}
            </LinkButton>
          )
        }
      >
        {status.peers.length ? (
          status.peers.map((peer) => (
            <Row
              key={peer.id}
              title={
                (peer.name ?? t("Paired device", "已配对设备")) +
                (peer.version ? ` · ${peer.version}` : "")
              }
              detail={peer.address}
            >
              {peer.online ? (
                <Pill tone="green" dot>
                  {t("Online", "在线")}
                </Pill>
              ) : (
                <Pill>
                  {t(
                    `Last seen ${clock(peer.lastSeenMs, t)}`,
                    `最后连接 ${clock(peer.lastSeenMs, t)}`,
                  )}
                </Pill>
              )}
            </Row>
          ))
        ) : (
          <Row
            muted
            title={t(
              "No device has connected since Agent HUD started.",
              "这次启动后还没有设备连接。",
            )}
          />
        )}
      </Section>
      {problem && <Notice tone="error">{problem}</Notice>}
    </Page>
  );
}

/** "2 paired devices": the saved pairings, not this launch's connections,
 * which the rows below list. */
function pairedSummary(grants: number, t: Text) {
  return t(
    `${grants} paired ${grants === 1 ? "device" : "devices"}`,
    `已配对 ${grants} 台设备`,
  );
}
