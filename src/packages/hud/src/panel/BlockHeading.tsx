/* A block's heading: its agent's logo and name, and the measure picker. A
 * compact block stacks them in the middle, the name over the picker, so the
 * block stays about as wide as the picker. */
import type { ReactNode } from "react";
import { agentVendorName } from "../agent-artwork";
import type { Logo } from "./theme";

export function BlockHeading({
  vendor,
  Logo,
  light,
  color,
  picker,
  compact,
}: {
  vendor: string;
  Logo: Logo;
  light: boolean;
  color: string;
  picker: ReactNode;
  compact: boolean;
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
          // One line in a narrow block: cut short, never wrapped.
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
            maxWidth: "100%",
          }}
        >
          {title}
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
