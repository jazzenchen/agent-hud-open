import { useCallback, useEffect, useState } from "react";
import { Input } from "@agent-hud/ui/components/input";
import {
  Divider,
  Row,
  secondary,
  Section,
  SmallButton,
  ToggleRow,
  useText,
  type SettingsPaneProps,
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
      <p style={{ ...secondary, fontSize: 13 }}>
        {error ?? t("Reading local sharing…", "正在读取局域网设置…")}
      </p>
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
  const lastSeen = (peer: Peer) =>
    peer.online
      ? t("Online", "在线")
      : t(
          `Last seen ${new Date(peer.lastSeenMs).toLocaleTimeString()}`,
          `最后连接 ${new Date(peer.lastSeenMs).toLocaleTimeString()}`,
        );
  return (
    <div style={{ display: "grid", gap: 24 }}>
      <Section title={t("Sharing", "共享")}>
        <ToggleRow
          title={t("Share with paired phones", "与已配对手机共享")}
          subtitle={t(
            "A paired iPhone reads this computer's quota and token usage on the same network or over your Tailscale. Sessions and conversations are not shared.",
            "已配对的 iPhone 可以在同一网络或你的 Tailscale 里查看本机的额度和 token 用量。会话和对话内容不会共享。",
          )}
          checked={status.on}
          disabled={!status.supported || pending}
          reason={t(
            "Sharing with phones is not available on this system yet.",
            "这个系统暂不支持与手机共享。",
          )}
          change={(enabled) => {
            setInvitation(null);
            void perform(() => share("set_enabled", { enabled }));
          }}
        />
        <Divider />
        <Row title={t("Address", "地址")} subtitle={addresses}>
          <span style={{ ...secondary, fontSize: 11 }}>
            {t("TLS · pinned certificate", "TLS · 证书固定校验")}
          </span>
        </Row>
      </Section>
      <Section title={t("Pair a phone", "配对手机")}>
        <Row
          title={t("Pairing code", "配对码")}
          subtitle={t(
            "Scan it with Agent HUD on iPhone. It works once and expires after five minutes. A paired phone can only read.",
            "用 iPhone 上的 Agent HUD 扫描。只能用一次，五分钟后失效。配对的手机只能查看。",
          )}
        >
          <SmallButton
            disabled={pending || !status.enabled}
            onClick={() =>
              void perform(async () => {
                setInvitation((await share("pair_invitation")) as Invitation);
                setCopied(false);
              })
            }
          >
            {t("Show code", "显示配对码")}
          </SmallButton>
        </Row>
        {invitation && (
          <div style={{ display: "grid", gap: 10, padding: "4px 16px 12px" }}>
            <img
              src={invitation.image}
              width={220}
              height={220}
              alt={t(
                "Single-use Agent HUD pairing code",
                "一次性 Agent HUD 配对码",
              )}
              style={{
                justifySelf: "center",
                background: "white",
                padding: 10,
                borderRadius: 8,
              }}
            />
            <Input value={invitation.uri} readOnly style={{ fontSize: 11 }} />
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
              }}
            >
              <span style={{ ...secondary, fontSize: 11 }}>
                {t(
                  `Expires at ${new Date(invitation.invitation.expiresAtMs).toLocaleTimeString()}`,
                  `${new Date(invitation.invitation.expiresAtMs).toLocaleTimeString()} 失效`,
                )}
              </span>
              <SmallButton
                onClick={() =>
                  void navigator.clipboard
                    .writeText(invitation.uri)
                    .then(() => setCopied(true))
                    .catch(() => setCopied(false))
                }
              >
                {copied ? t("Copied", "已复制") : t("Copy link", "复制链接")}
              </SmallButton>
            </div>
          </div>
        )}
        <Divider />
        <Row
          title={t("Paired phones", "已配对手机")}
          subtitle={t(
            `${status.pairedClients} saved`,
            `已保存 ${status.pairedClients} 台`,
          )}
        >
          <SmallButton
            disabled={pending || status.pairedClients === 0}
            onClick={() => {
              setInvitation(null);
              void perform(() => share("revoke_clients"));
            }}
          >
            {t("Remove all", "全部移除")}
          </SmallButton>
        </Row>
      </Section>
      <Section title={t("Phones", "手机")}>
        {status.peers.length ? (
          status.peers.map((peer, index) => (
            <div key={peer.id}>
              {index > 0 && <Divider />}
              <Row
                title={
                  (peer.name ?? t("Paired phone", "已配对手机")) +
                  (peer.version ? ` · ${peer.version}` : "")
                }
                subtitle={peer.address}
              >
                <span style={{ ...secondary, fontSize: 11 }}>
                  {lastSeen(peer)}
                </span>
              </Row>
            </div>
          ))
        ) : (
          <p style={{ ...secondary, fontSize: 12, padding: "8px 16px" }}>
            {t(
              "No phone has connected since Agent HUD started.",
              "这次启动后还没有手机连接。",
            )}
          </p>
        )}
      </Section>
      {(error ?? status.error) && (
        <p style={{ color: "var(--destructive)", fontSize: 12 }}>
          {error ?? status.error}
        </p>
      )}
    </div>
  );
}
