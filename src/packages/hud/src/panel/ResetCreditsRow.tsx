import { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Info, RotateCcw } from "lucide-react";
import type { ResetCredits } from "@agent-hud/protocol";
import { useFormat } from "@agent-hud/ui/format-context";
import { translate } from "@agent-hud/ui/format";
import { expiryLabel, nextExpiryLabel } from "./account-labels";
import { palette } from "./theme";
import { useGround } from "../ground";

/** Codex's earned usage resets, once beneath the signed-in account's windows:
 * how many are available and the first expiry, with every expiry in a card
 * while the row is hovered. The card is drawn above the panel, which scrolls
 * and would cut it. */
export function ResetCreditsRow({
  resets,
  showExpiry,
  light,
  compact = false,
}: {
  resets: ResetCredits;
  showExpiry: boolean;
  light: boolean;
  /** In a compact block: the icon and the count in the middle; the card tells the rest. */
  compact?: boolean;
}) {
  const colors = palette(useGround(light));
  const { language } = useFormat();
  const row = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState<DOMRect | null>(null);
  const available = resets.availableCount > 0;
  const label = nextExpiryLabel(resets, language);
  return (
    <div
      ref={row}
      className="hud-source-reset-credits"
      aria-label={translate(language, "Usage resets")}
      onMouseEnter={() =>
        available && setHovered(row.current?.getBoundingClientRect() ?? null)
      }
      onMouseLeave={() => setHovered(null)}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: compact ? "center" : undefined,
        gap: compact ? 6 : 12,
        padding: "2px 6px",
        fontSize: 12,
        lineHeight: "16px",
        borderRadius: 6,
        whiteSpace: "nowrap",
        color: colors.secondary,
        background: hovered ? colors.selected : "transparent",
        transition: "background 0.12s ease-out",
      }}
    >
      <RotateCcw
        size={11}
        strokeWidth={2.4}
        style={{ flex: "0 0 8px", width: 8 }}
      />
      {!compact && (
        <span style={{ flex: "0 0 152px", width: 152, fontWeight: 500 }}>
          {translate(language, "Usage resets")}
        </span>
      )}
      <span
        style={{
          fontWeight: 600,
          fontVariantNumeric: "tabular-nums",
          color: available ? colors.okText : colors.secondary,
        }}
      >
        {translate(language, "{{available}} available", {
          available: resets.availableCount,
        })}
      </span>
      {compact ? null : available && showExpiry ? (
        <ExpiryLabel full={label.full} short={label.short} />
      ) : (
        <span style={{ flex: "1 1 8px", minWidth: 8 }} />
      )}
      {available && (
        <Info
          size={11}
          style={{
            flex: "0 0 auto",
            color: hovered ? colors.secondary : colors.tertiary,
          }}
        />
      )}
      {hovered &&
        createPortal(
          <ResetCreditsCard resets={resets} anchor={hovered} light={light} />,
          document.body,
        )}
    </div>
  );
}

/** The whole label where it fits, else the date alone, else nothing; the card
 * tells every expiry. It takes the row's free space, so its width is the room. */
function ExpiryLabel({ full, short }: { full: string; short: string }) {
  const box = useRef<HTMLSpanElement>(null);
  const probe = useRef<HTMLSpanElement>(null);
  const [form, setForm] = useState<"full" | "short" | "none">("full");
  useLayoutEffect(() => {
    const element = box.current;
    const measure = probe.current;
    if (!element || !measure) return;
    const fit = () => {
      const width = (text: string) => {
        measure.textContent = text;
        return measure.offsetWidth;
      };
      const room = element.clientWidth - 8;
      setForm(
        width(full) <= room ? "full" : width(short) <= room ? "short" : "none",
      );
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  }, [full, short]);
  return (
    <span
      ref={box}
      style={{
        position: "relative",
        flex: "1 1 0",
        minWidth: 8,
        overflow: "hidden",
        textAlign: "right",
        fontSize: 11,
        fontVariantNumeric: "tabular-nums",
      }}
    >
      {form === "full" ? full : form === "short" ? short : ""}
      <span
        ref={probe}
        aria-hidden
        style={{
          position: "absolute",
          visibility: "hidden",
          whiteSpace: "nowrap",
        }}
      />
    </span>
  );
}

function ResetCreditsCard({
  resets,
  anchor,
  light,
}: {
  resets: ResetCredits;
  anchor: DOMRect;
  light: boolean;
}) {
  const colors = palette(useGround(light));
  const { language } = useFormat();
  const credits = resets.credits ?? [];
  const missing = resets.availableCount - credits.length;
  const width = 310;
  const left = Math.max(
    8,
    Math.min(anchor.right - width, window.innerWidth - width - 8),
  );
  return (
    <div
      role="tooltip"
      style={{
        position: "fixed",
        left,
        top: anchor.bottom + 6,
        width,
        zIndex: 1000,
        pointerEvents: "none",
        display: "grid",
        gap: 10,
        padding: 12,
        borderRadius: 10,
        fontSize: 11,
        color: colors.text,
        background: light ? "#f5f5f7" : "#1c1c1e",
        border: `1px solid ${colors.divider}`,
        boxShadow: "0 8px 24px rgba(0,0,0,0.35)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center" }}>
        <strong style={{ fontSize: 12, fontWeight: 600 }}>
          {translate(language, "Usage resets")}
        </strong>
        <span style={{ flex: 1 }} />
        <span
          style={{
            fontWeight: 600,
            fontVariantNumeric: "tabular-nums",
            color: colors.okText,
          }}
        >
          {translate(language, "{{available}} available", {
            available: resets.availableCount,
          })}
        </span>
      </div>
      <span style={{ fontSize: 10, color: colors.secondary }}>
        {translate(language, "Expires at · local time")}
      </span>
      <div style={{ display: "grid", gap: 8 }}>
        {credits.map((credit, index) => (
          <div key={credit.id} style={{ display: "flex" }}>
            <span style={{ color: colors.secondary }}>
              {translate(language, "Reset {{number}}", { number: index + 1 })}
            </span>
            <span style={{ flex: 1 }} />
            <span style={{ fontSize: 12, fontVariantNumeric: "tabular-nums" }}>
              {expiryLabel(credit.expiresAt, language)}
            </span>
          </div>
        ))}
      </div>
      {missing > 0 && (
        <span style={{ color: colors.secondary }}>
          {translate(language, "Expiry unavailable for {{count}} resets", {
            count: missing,
          })}
        </span>
      )}
    </div>
  );
}
