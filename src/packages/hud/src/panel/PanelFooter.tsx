import { useEffect, useRef, useState, type CSSProperties } from "react";
import {
  ChartNoAxesColumnIncreasing,
  CircleArrowDown,
  RefreshCw,
  Settings,
} from "lucide-react";
import { motion } from "motion/react";
import { Button } from "@agent-hud/ui/components/button";
import { translator, type Language } from "@agent-hud/ui/format";
import type { HUDUpdateNotice } from "../model";
import { readAgoLabel } from "./account-labels";
import type { palette } from "./theme";

/** The panel's footer: Settings; when the numbers were last read, with a
 * button that reads every agent again; and Statistics with the newer
 * version's arrow beside it when there is one. */
export function PanelFooter({
  language,
  colors,
  lined,
  onConfigure,
  onStats,
  update,
  onUpdate,
  readAt,
  onRefresh,
}: {
  language: Language;
  colors: ReturnType<typeof palette>;
  /** A line above it, where the panel draws lines and not cards. */
  lined: boolean;
  onConfigure?: () => void;
  onStats?: () => void;
  update?: HUDUpdateNotice | null;
  onUpdate?: () => void;
  /** When the newest reading was taken (`latestReading`). */
  readAt?: number | null;
  onRefresh?: () => Promise<unknown>;
}) {
  const t = translator(language);
  const [refreshing, setRefreshing] = useState(false);
  // The age moves on while the panel stays open.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 15_000);
    return () => clearInterval(timer);
  }, []);
  // The reading a refresh started from: the icon turns until a newer one
  // arrives, at most 15 s, as the app only queues the reads.
  const started = useRef<number | null | undefined>(undefined);
  useEffect(() => {
    setNow(Date.now());
    if (started.current !== undefined && readAt !== started.current) {
      started.current = undefined;
      setRefreshing(false);
    }
  }, [readAt]);
  useEffect(() => {
    if (!refreshing) return;
    const timer = setTimeout(() => {
      started.current = undefined;
      setRefreshing(false);
    }, 15_000);
    return () => clearTimeout(timer);
  }, [refreshing]);
  const refresh = () => {
    if (!onRefresh || refreshing) return;
    // It turns at once, before the app answers.
    started.current = readAt ?? null;
    setRefreshing(true);
    void onRefresh().catch(() => {
      started.current = undefined;
      setRefreshing(false);
    });
  };
  const refreshLabel = t("Read all agents again");
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
      {(readAt != null || onRefresh) && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            minWidth: 0,
            fontSize: 11,
            color: colors.secondary,
          }}
        >
          {readAt != null && (
            <span
              style={{
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                fontVariantNumeric: "tabular-nums",
              }}
            >
              {readAgoLabel(readAt, now, language)}
            </span>
          )}
          {onRefresh && (
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={refreshLabel}
              title={refreshLabel}
              aria-busy={refreshing || undefined}
              onClick={refresh}
              style={footerButton(colors.secondary)}
            >
              <motion.span
                style={{ display: "inline-flex" }}
                animate={refreshing ? { rotate: 360 } : { rotate: 0 }}
                transition={
                  refreshing
                    ? { duration: 0.8, ease: "linear", repeat: Infinity }
                    : { duration: 0 }
                }
              >
                <RefreshCw
                  style={{ width: 13, height: 13 }}
                  strokeWidth={1.7}
                />
              </motion.span>
            </Button>
          )}
        </div>
      )}
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
