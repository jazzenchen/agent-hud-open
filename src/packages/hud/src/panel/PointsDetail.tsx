import { useState } from "react";
import { useFormat } from "@agent-hud/ui/format-context";
import { useGround } from "../ground";
import { usePanelServices } from "../PanelContext";
import { palette, tabular } from "./theme";
import { pointsAmount } from "./points-detail";
import { GrantRow, RemainingRow } from "./PointsDetailRows";

/** TeleAgent's points detail in the side panel: the total, a green bar for what remains, and points that expire. */
export interface PointsGrant {
  name: string;
  points: number | null;
  expireDays: number | null;
}
export interface PointsGroup {
  total: number | null;
  remaining: number | null;
  percent: number | null;
  grant: PointsGrant | null;
}

const ROW =
  "20px 6px max-content minmax(48px, 1fr) max-content max-content" as const;

export function PointsDetail({
  groups,
  spent,
  vendor,
  light,
  mode = "remaining",
  onOpen,
}: {
  groups: readonly PointsGroup[];
  spent?: { perDay: number | null; days: number } | null;
  vendor?: string;
  light: boolean;
  /** Remaining share, or points already spent and the daily pace. */
  mode?: "remaining" | "used";
  onOpen?: () => void;
}) {
  const colors = palette(useGround(light));
  const { language } = useFormat();
  const services = usePanelServices();
  const [hovered, setHovered] = useState(false);
  const glow = light ? undefined : `0 0 8px ${colors.ok}`;
  const clickable = Boolean(vendor && (services?.selectProvider || onOpen));
  const open = () => {
    if (vendor) services?.selectProvider?.(vendor);
    onOpen?.();
  };
  return (
    <div style={{ display: "grid", rowGap: 10 }}>
      {groups.map((group, index) => {
        const showRemaining = group.remaining != null || group.percent != null;
        const grant = group.grant;
        const showGrant = Boolean(
          grant &&
          (grant.points != null || grant.expireDays != null || grant.name),
        );
        return (
          <div key={index} style={{ display: "grid", rowGap: 8 }}>
            {group.total != null && (
              <div
                style={{
                  paddingLeft: 26,
                  fontSize: 12,
                  lineHeight: "16px",
                  color: colors.secondary,
                  ...tabular,
                }}
              >
                {language === "zh" ? "总额度" : "Total quota"}{" "}
                {pointsAmount(group.total, language)}
              </div>
            )}
            <div
              role={clickable ? "button" : undefined}
              tabIndex={clickable ? 0 : undefined}
              onClick={clickable ? open : undefined}
              onKeyDown={
                clickable
                  ? (event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        open();
                      }
                    }
                  : undefined
              }
              onMouseEnter={() => setHovered(true)}
              onMouseLeave={() => setHovered(false)}
              style={{
                display: "grid",
                gridTemplateColumns: ROW,
                rowGap: 8,
                alignItems: "center",
                cursor: clickable ? "pointer" : undefined,
                borderRadius: 6,
                background:
                  hovered && clickable ? colors.selected : "transparent",
              }}
            >
              {showRemaining && (
                <RemainingRow
                  group={group}
                  perDay={spent?.perDay ?? null}
                  mode={mode}
                  language={language}
                  text={colors.text}
                  secondary={colors.secondary}
                  ok={colors.ok}
                  okText={colors.okText}
                  warning={colors.warning}
                  warningText={colors.warningText}
                  critical={colors.critical}
                  criticalText={colors.criticalText}
                  track={colors.track}
                  glow={glow}
                />
              )}
              {showGrant && grant && (
                <GrantRow
                  row={showRemaining ? 2 : 1}
                  grant={grant}
                  language={language}
                  text={colors.text}
                  secondary={colors.secondary}
                />
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
