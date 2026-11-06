import type { Snapshot } from "@agent-hud/protocol";
import { snapshotLanguage } from "@agent-hud/ui/format";
import { indexingText } from "@agent-hud/ui/text/indexing";
import { useGround } from "../ground";
import { palette, SOURCE_PANEL } from "./theme";

/** While the usage ledger is read through (built for the first time, or
 * again after a data-version change): how far back it has read, a bar of
 * the days read, how far back the logs go and that totals are incomplete
 * meanwhile. Shows nothing otherwise. */
export function SourceIndexing({
  snapshot,
  light,
}: {
  snapshot: Snapshot;
  light: boolean;
}) {
  const colors = palette(useGround(light));
  const text = indexingText(snapshot.indexing, snapshotLanguage(snapshot));
  if (!text) return null;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <span
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          flexWrap: "wrap",
          columnGap: 8,
        }}
      >
        <span
          style={{
            fontSize: SOURCE_PANEL.rowSize,
            fontWeight: 600,
            color: colors.text,
          }}
        >
          {text.title}
        </span>
        {text.days ? (
          <span
            style={{
              fontSize: SOURCE_PANEL.detailSize,
              color: colors.secondary,
              fontVariantNumeric: "tabular-nums",
            }}
          >
            {text.days}
          </span>
        ) : null}
      </span>
      <span
        role="progressbar"
        aria-label={text.title}
        aria-valuetext={text.days || undefined}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.floor(text.fraction * 100)}
        style={{
          position: "relative",
          height: 4,
          background: colors.track,
          borderRadius: 4,
          overflow: "hidden",
        }}
      >
        <i
          style={{
            position: "absolute",
            inset: "0 auto 0 0",
            display: "block",
            height: 4,
            width: `${text.fraction * 100}%`,
            borderRadius: 4,
            background: colors.accent,
            transition: "width 600ms ease-out",
          }}
        />
      </span>
      <span
        style={{ fontSize: SOURCE_PANEL.detailSize, color: colors.secondary }}
      >
        {text.detail}
      </span>
    </div>
  );
}
