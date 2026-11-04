import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { Account } from "@agent-hud/protocol";
import { accountReadLabel } from "./account-labels";
import { useFormat } from "@agent-hud/ui/format-context";
import { MiddleTruncated } from "./MiddleTruncated";
import { palette } from "./theme";
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
}: {
  account: Account;
  now: number;
  light: boolean;
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
        gap: 2,
        padding: "2px 6px 0",
        fontSize: 11,
        lineHeight: "13px",
      }}
    >
      <div
        style={{ display: "flex", alignItems: "center", gap: 6, minHeight: 15 }}
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
        <span style={{ flex: "1 1 8px", minWidth: 8 }} />
        <WholeOrNothing color={colors.tertiary}>
          {accountReadLabel(account, now, language)}
        </WholeOrNothing>
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
