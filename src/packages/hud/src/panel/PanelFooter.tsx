import type { CSSProperties } from "react";
import {
  ChartNoAxesColumnIncreasing,
  CircleArrowDown,
  Settings,
} from "lucide-react";
import { motion } from "motion/react";
import { Button } from "@agent-hud/ui/components/button";
import { translator, type Language } from "@agent-hud/ui/format";
import type { HUDUpdateNotice } from "../model";
import type { palette } from "./theme";

/** The panel's footer: Settings, and Statistics with the newer version's
 * arrow beside it when there is one. */
export function PanelFooter({
  language,
  colors,
  lined,
  onConfigure,
  onStats,
  update,
  onUpdate,
}: {
  language: Language;
  colors: ReturnType<typeof palette>;
  /** A line above it, where the panel draws lines and not cards. */
  lined: boolean;
  onConfigure?: () => void;
  onStats?: () => void;
  update?: HUDUpdateNotice | null;
  onUpdate?: () => void;
}) {
  const t = translator(language);
  const footerButton = (color: string): CSSProperties => ({
    width: 28,
    height: 28,
    minHeight: 28,
    padding: 0,
    borderRadius: 0,
    color,
    background: "transparent",
  });
  return (
    <footer
      data-hud-footer
      className="hud-source-footer"
      style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        flex: "0 0 auto",
        paddingTop: lined ? 6 : 0,
        boxShadow: lined ? `inset 0 1px 0 ${colors.divider}` : undefined,
        color: colors.secondary,
      }}
    >
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t("Settings")}
        title={t("Settings")}
        disabled={!onConfigure}
        onClick={onConfigure}
        style={footerButton(colors.secondary)}
      >
        <Settings style={{ width: 14, height: 14 }} strokeWidth={1.7} />
      </Button>
      <div style={{ display: "flex", alignItems: "center" }}>
        {update && (
          // It arrives as the HUD's other changes do: it grows in, or with
          // reduced motion only fades in.
          <motion.span
            key={update.version}
            initial={{ opacity: 0, scale: 0.6 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ duration: 0.22, ease: "easeOut" }}
            style={{ display: "inline-flex" }}
          >
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t("New version available")}
              title={t("Update to {{version}}", { version: update.version })}
              disabled={!onUpdate}
              onClick={onUpdate}
              style={footerButton(colors.accent)}
            >
              <CircleArrowDown
                style={{ width: 14, height: 14 }}
                strokeWidth={1.7}
              />
            </Button>
          </motion.span>
        )}
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("Usage statistics")}
          title={t("Usage statistics")}
          disabled={!onStats}
          onClick={onStats}
          style={footerButton(colors.secondary)}
        >
          <ChartNoAxesColumnIncreasing
            style={{ width: 14, height: 14 }}
            strokeWidth={1.7}
          />
        </Button>
      </div>
    </footer>
  );
}
