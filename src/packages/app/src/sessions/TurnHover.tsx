import { presentKinds } from "../lib/sessions";
import {
  card,
  ColorKey,
  compact,
  cost,
  kindColor,
  kindLabel,
  numeric,
  row,
  timeLabel,
  type Entry,
  type Theme,
} from "./SessionTheme";
import { translator, type Language } from "@agent-hud/ui/format";
import { useFormat, useT } from "@agent-hud/ui/format-context";

/** The facts Core gives a turn, as Open's chart notes them. */
export function turnNotes(entry: Entry, language: Language): string[] {
  const t = translator(language);
  const notes: string[] = [];
  if (entry.callCount != null)
    notes.push(t("{{count}} calls", { count: entry.callCount }));
  if (entry.subagentNew)
    notes.push(
      t("Sub-agents added {{tokens}}", {
        tokens: compact(entry.subagentNew),
      }),
    );
  if (entry.recached != null)
    notes.push(
      t("Cache expired, {{tokens}} sent again", {
        tokens: compact(entry.recached),
      }),
    );
  if (entry.context != null)
    notes.push(
      entry.peakContext != null
        ? t("Context {{tokens}} · peak {{peak}}", {
            tokens: compact(entry.context),
            peak: compact(entry.peakContext),
          })
        : t("Context {{tokens}}", { tokens: compact(entry.context) }),
    );
  const turnCost = entry.listCost ?? entry.pricedCost ?? null;
  if (turnCost != null)
    notes.push(
      t("{{cost}} at API prices", {
        cost: cost(turnCost, language, entry.listCost == null),
      }),
    );
  return notes;
}

/** What one turn holds, beside the bar the pointer is on. */
export function TurnHover({ entry, theme }: { entry: Entry; theme: Theme }) {
  const t = useT();
  const { language } = useFormat();
  return (
    <div
      style={{
        ...card(theme, "9px 11px"),
        background: theme.surface,
        boxShadow: theme.light
          ? "0 4px 14px rgba(0,0,0,.12)"
          : "0 4px 14px rgba(0,0,0,.4)",
        display: "grid",
        gap: 3,
        fontSize: 11,
        width: 220,
      }}
    >
      <div
        style={{
          ...row,
          justifyContent: "space-between",
          color: theme.secondary,
        }}
      >
        <span>{`T${entry.index} · ${timeLabel(entry.timestamp, language)}`}</span>
        {entry.compacted && (
          <span style={{ color: theme.compaction, fontWeight: 600 }}>
            {t("Compacted")}
          </span>
        )}
      </div>
      <strong style={{ fontSize: 13, ...numeric }}>
        {compact(entry.newTokens ?? entry.input + entry.output)}{" "}
        <span style={{ fontWeight: 400, color: theme.secondary, fontSize: 11 }}>
          {t("new tokens")}
        </span>
      </strong>
      {presentKinds(entry.kinds).map(({ kind, value }) => (
        <span key={kind} style={{ ...row, gap: 6 }}>
          <ColorKey color={kindColor(theme, kind)}>
            <span style={{ color: theme.secondary }}>
              {kindLabel(kind, language)}
            </span>
          </ColorKey>
          <span style={{ marginLeft: "auto", ...numeric }}>
            {compact(value)}
          </span>
        </span>
      ))}
      {turnNotes(entry, language).map((note) => (
        <span key={note} style={{ color: theme.secondary }}>
          {note}
        </span>
      ))}
    </div>
  );
}
