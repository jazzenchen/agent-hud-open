// The pieces both kinds of the guide's window beside the HUD are made of.
// Buttons are the app's own (shadcn), so they answer the pointer as the
// HUD's panel and Settings do.
import type { ReactNode } from "react";
import { X } from "lucide-react";
import { Button } from "@agent-hud/ui/components/button";
import type { Text } from "./copy";

/** The arrow's colour: the system blue the HUD's links use. */
export const blue = "#0a84ff";

export function Header({
  label,
  close,
  t,
}: {
  label: string;
  close: () => void;
  t: Text;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", fontSize: 11 }}>
      <span style={{ flex: 1, opacity: 0.55 }}>{label}</span>
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label={t("Close the guide", "关闭引导")}
        onClick={close}
        className="opacity-60 hover:opacity-100"
      >
        <X />
      </Button>
    </div>
  );
}

export function Body({ title, body }: { title: string; body: string }) {
  return (
    <>
      <div style={{ fontSize: 15, fontWeight: 600, margin: "2px 0 3px" }}>
        {title}
      </div>
      <div style={{ opacity: 0.72 }}>{body}</div>
    </>
  );
}

export function Actions({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "flex-end",
        alignItems: "center",
        gap: 6,
        marginTop: 12,
      }}
    >
      {children}
    </div>
  );
}
