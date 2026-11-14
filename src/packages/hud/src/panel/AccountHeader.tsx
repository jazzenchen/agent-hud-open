import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { Account } from "@agent-hud/protocol";
import { accountReadLabel } from "./account-labels";
import { useFormat, useT } from "@agent-hud/ui/format-context";
import { MiddleTruncated } from "./MiddleTruncated";
import { noButtonChrome, palette } from "./theme";
import { useGround } from "../ground";

/** The account above its windows (Open's AccountSectionHeader): its name,
 * shortened from the middle, its plan, and at the end of the line whether it
 * is current or when it was last read; under it the warning its client gave
 * (amber) and any other notice. Where the line is short the label gives way
 * first, then the name shortens. */
export function AccountHeader({
  account,
  now,
  light,
  compact = false,
}: {
  account: Account;
  now: number;
  light: boolean;
  /** Under a compact block's ring: the name and plan in the middle, no more. */
  compact?: boolean;
}) {
  const colors = palette(useGround(light));
  const { language } = useFormat();
  const current = account.current !== false;
  const name = account.name ?? account.label ?? account.id;
  return (
    <div
      className="hud-source-account-heading"
      style={{
        display: "grid",
        // As wide as the block, never its text: a long address shortens.
        gridTemplateColumns: "minmax(0, 1fr)",
        gap: 2,
        padding: "2px 6px 0",
        fontSize: 11,
        lineHeight: "13px",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: compact ? "center" : undefined,
          gap: 6,
          minHeight: 15,
          minWidth: 0,
        }}
      >
        <MiddleTruncated
          text={name}
          style={{
            flex: "0 1 auto",
            color: colors.text,
            opacity: current ? 0.85 : 0.6,
          }}
        />
        {account.planLabel && (
          <span
            style={{
              flex: "0 0 auto",
              color: colors.secondary,
              whiteSpace: "nowrap",
            }}
          >
            {account.planLabel}
          </span>
        )}
        {!compact && (
          <>
            <span style={{ flex: "1 1 8px", minWidth: 8 }} />
            <WholeOrNothing color={colors.tertiary}>
              {accountReadLabel(account, now, language)}
            </WholeOrNothing>
          </>
        )}
      </div>
      {account.warning && (
        <Notice text={account.warning} color={colors.warningText} />
      )}
      {account.notice && account.notice !== account.warning && (
        <Notice text={account.notice} color={colors.secondary} />
      )}
    </div>
  );
}

/** A block's notice where it has no account to stand under, as when its
 * client is signed out: set like the notice under an account's name. */
export function BlockNotice({
  text,
  light,
  warning = false,
}: {
  text: string;
  light: boolean;
  /** In the warning's text shade, as for a vendor outage. */
  warning?: boolean;
}) {
  const colors = palette(useGround(light));
  return (
    <div
      className="hud-source-block-notice"
      style={{
        display: "grid",
        padding: "2px 6px 0",
        fontSize: 11,
        lineHeight: "13px",
      }}
    >
      <Notice
        text={text}
        color={warning ? colors.warningText : colors.secondary}
      />
    </div>
  );
}

/** A quota only the vendor's site shows: the user signs in to it in Agent
 * HUD's own window, opened only by these buttons, one per site. */
export function WebSignInNotice({
  sites,
  again,
  light,
  signIn,
}: {
  sites: { host: string; name: string }[];
  /** The kept sign-in ended, so the user signs in again. */
  again: boolean;
  light: boolean;
  signIn: (host: string) => void;
}) {
  const colors = palette(useGround(light));
  const t = useT();
  const lead = again
    ? t("Sign in again to keep reading the quota: ")
    : t("Sign in to show the quota: ");
  return (
    <div
      className="hud-source-web-sign-in"
      style={{
        padding: "2px 6px 0",
        fontSize: 11,
        lineHeight: "13px",
        color: colors.secondary,
      }}
    >
      {lead}
      {sites.map((site, index) => (
        <span key={site.host}>
          {index > 0 && " · "}
          <button
            type="button"
            title={t("Sign in to {{host}} in Agent HUD's own window", {
              host: site.host,
            })}
            onClick={(event) => {
              event.stopPropagation();
              signIn(site.host);
            }}
            style={{
              ...noButtonChrome,
              color: colors.text,
              textDecoration: "underline",
              textUnderlineOffset: 2,
              cursor: "pointer",
            }}
          >
            {sites.length > 1 ? site.name : t("Sign in")}
          </button>
        </span>
      ))}
    </div>
  );
}

/** Shown whole or not at all; it gives way before anything else on its line. */
function WholeOrNothing({
  children,
  color,
}: {
  children: ReactNode;
  color: string;
}) {
  const box = useRef<HTMLSpanElement>(null);
  const [fits, setFits] = useState(true);
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const measure = () =>
      setFits(element.scrollWidth <= element.clientWidth + 0.5);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [children]);
  return (
    <span
      ref={box}
      style={{
        flex: "0 100 auto",
        minWidth: 0,
        overflow: "hidden",
        whiteSpace: "nowrap",
        color,
        visibility: fits ? undefined : "hidden",
      }}
    >
      {children}
    </span>
  );
}

function Notice({ text, color }: { text: string; color: string }) {
  return (
    <span
      style={{
        color,
        display: "-webkit-box",
        WebkitLineClamp: 2,
        WebkitBoxOrient: "vertical",
        overflow: "hidden",
      }}
    >
      {text}
    </span>
  );
}
