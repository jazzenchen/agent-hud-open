/* A block's heading: its agent's logo and name, and the measure picker. A
 * compact block puts the name and its account's plan on one line and the
 * picker under them, so the block stays about as wide as the picker. */
import type { ReactNode } from "react";
import type { Account } from "@agent-hud/protocol";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@agent-hud/ui/components/tooltip";
import { agentVendorName } from "../agent-artwork";
import { useGround } from "../ground";
import { palette, type Logo } from "./theme";

export function BlockHeading({
  vendor,
  Logo,
  light,
  color,
  picker,
  compact,
  trailing,
}: {
  vendor: string;
  Logo: Logo;
  light: boolean;
  color: string;
  picker: ReactNode;
  compact: boolean;
  /** A compact block's plan, at the right of its name's line. */
  trailing?: ReactNode;
}) {
  const name = agentVendorName(vendor);
  const title = (
    <>
      <span
        style={{
          width: 20,
          flex: "0 0 20px",
          display: "inline-flex",
          justifyContent: "center",
        }}
      >
        <Logo source={vendor} size={14} light={light} />
      </span>
      <strong
        title={name}
        style={{
          fontSize: 13,
          fontWeight: 700,
          lineHeight: "16px",
          color,
          // One line in a narrow block: cut short, never wrapped. A compact
          // block's name keeps its whole width first; its plan gives way.
          ...(compact
            ? { flex: "0 0 auto", maxWidth: "calc(100% - 24px)" }
            : null),
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {name}
      </strong>
    </>
  );
  if (compact)
    return (
      <div
        className="hud-source-provider-heading"
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 4,
          padding: "1px 0",
          minWidth: 0,
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            minHeight: 20,
            width: "100%",
          }}
        >
          {title}
          <span style={{ flex: "1 1 4px", minWidth: 4 }} />
          {trailing}
        </div>
        {picker}
      </div>
    );
  return (
    <div
      className="hud-source-provider-heading"
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "1px 0",
        minHeight: 20,
      }}
    >
      {title}
      <span style={{ flex: 1 }} />
      {picker}
    </div>
  );
}

/** A compact block's plan beside its name: Ultra, Pro x5. Pointing at it
 * shows the account and what it notes; a dot says it notes something, in the
 * warning's colour for a warning. */
export function PlanBadge({
  account,
  light,
}: {
  account: Account;
  light: boolean;
}) {
  const colors = palette(useGround(light));
  const notes = [
    account.warning,
    account.notice !== account.warning ? account.notice : null,
  ].filter((note): note is string => Boolean(note));
  if (!account.planLabel && !notes.length) return null;
  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              minWidth: 0,
              fontSize: 11,
              lineHeight: "16px",
              color: colors.secondary,
              whiteSpace: "nowrap",
            }}
          >
            {account.planLabel && (
              // The plan shortens while a few letters fit and leaves below
              // that: an empty first item holds the one line shown, so the
              // plan wraps onto a hidden second line instead of a stub.
              <span
                style={{
                  display: "flex",
                  flexWrap: "wrap",
                  height: 16,
                  minWidth: 0,
                  overflow: "hidden",
                }}
              >
                <span />
                <span
                  style={{
                    flex: "1 1 28px",
                    minWidth: 28,
                    maxWidth: "max-content",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    textAlign: "end",
                  }}
                >
                  {account.planLabel}
                </span>
              </span>
            )}
            {notes.length > 0 && (
              <span
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: 3,
                  flex: "0 0 6px",
                  background: account.warning
                    ? colors.warningText
                    : colors.tertiary,
                }}
              />
            )}
          </span>
        </TooltipTrigger>
        <TooltipContent
          side="top"
          style={{ whiteSpace: "pre-line", maxWidth: 260 }}
        >
          {[account.name ?? account.label ?? account.id, ...notes].join("\n")}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
