/* The building blocks every Settings page is made of, so all pages share one
 * look: a section is a heading over a card, a card holds rows, a row is words
 * on the left and its control on the right (SettingsControls.tsx). Spacing,
 * type and dividers live in settings.css; pages never style these by hand. */
import { useId, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import type { NativeMethod, Snapshot } from "@agent-hud/protocol";
import type { HUDConfig } from "@agent-hud/hud/model";
import { snapshotLanguage } from "@agent-hud/ui/format";

export interface SettingsPaneProps {
  snapshot: Snapshot;
  active?: boolean;
  config: HUDConfig;
  update: (patch: Partial<HUDConfig>) => void;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
}

export type Text = (en: string, zh: string) => string;

export function useText(snapshot: Snapshot | null): Text {
  const chinese = snapshot ? snapshotLanguage(snapshot) === "zh" : false;
  return (en, zh) => (chinese ? zh : en);
}

/** A page's body: its sections, one under another. */
export function Page({ children }: { children: ReactNode }) {
  return <div className="settings-page">{children}</div>;
}

/** A heading over a card of rows. `action` sits at the heading's right (a
 * count, a Refresh button); `footer` is a short note under the card. With
 * `plain` the children go under the heading without a card. */
export function Section({
  title,
  subtitle,
  action,
  footer,
  plain = false,
  anchor,
  children,
}: {
  title?: ReactNode;
  subtitle?: ReactNode;
  action?: ReactNode;
  footer?: ReactNode;
  plain?: boolean;
  /** What the guide calls this section (`data-guide`). */
  anchor?: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-section" data-guide={anchor}>
      {(title || action) && (
        <header className="settings-section-header">
          <div>
            {title && <h2>{title}</h2>}
            {subtitle && <p>{subtitle}</p>}
          </div>
          {action && <div className="settings-section-action">{action}</div>}
        </header>
      )}
      {plain ? children : <div className="settings-card">{children}</div>}
      {footer && <Footnote>{footer}</Footnote>}
    </section>
  );
}

/** A card of rows outside a Section (inside a Collapsible, say). */
export function Card({ children }: { children: ReactNode }) {
  return <div className="settings-card">{children}</div>;
}

/** Words on the left, the control on the right. `icon` is a leading glyph or
 * logo; `tag` a small grey label after the title; `detail` a line of facts
 * under the subtitle; `below` content that spans the row under both. */
export function Row({
  title,
  subtitle,
  tag,
  icon,
  detail,
  below,
  muted = false,
  anchor,
  children,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  tag?: ReactNode;
  icon?: ReactNode;
  detail?: ReactNode;
  below?: ReactNode;
  muted?: boolean;
  /** What the guide calls this row (`data-guide`). */
  anchor?: string;
  children?: ReactNode;
}) {
  return (
    <div
      className={`settings-row${muted ? " is-muted" : ""}`}
      data-guide={anchor}
    >
      <div className="settings-row-main">
        {icon && <div className="settings-row-icon">{icon}</div>}
        <div className="settings-row-text">
          <div className="settings-row-title">
            <span>{title}</span>
            {tag && <span className="settings-tag">{tag}</span>}
          </div>
          {subtitle && <div className="settings-row-subtitle">{subtitle}</div>}
          {detail && <div className="settings-row-detail">{detail}</div>}
        </div>
        {children !== undefined && children !== null && children !== false && (
          <div className="settings-row-control">{children}</div>
        )}
      </div>
      {below && <div className="settings-row-below">{below}</div>}
    </div>
  );
}

export type PillTone = "green" | "blue" | "orange" | "red" | "gray";

/** A small status label: Ready, Active, Expired, Not added. */
export function Pill({
  tone = "gray",
  dot = false,
  children,
}: {
  tone?: PillTone;
  dot?: boolean;
  children: ReactNode;
}) {
  return (
    <span className={`settings-pill is-${tone}`}>
      {dot && <span className="settings-pill-dot" />}
      {children}
    </span>
  );
}

/** A short note under a card or section. */
export function Footnote({ children }: { children: ReactNode }) {
  return <div className="settings-footnote">{children}</div>;
}

/** A message that needs the reader: a problem, a warning, a next step. */
export function Notice({
  tone = "info",
  children,
  action,
}: {
  tone?: "info" | "warning" | "error";
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div
      className={`settings-notice is-${tone}`}
      role={tone === "error" ? "alert" : "status"}
    >
      <div>{children}</div>
      {action && <div className="settings-notice-action">{action}</div>}
    </div>
  );
}

/** A card that folds: a header row (icon, title, a short summary on the
 * right) that opens to show its rows. Closed unless `defaultOpen`. */
export function Collapsible({
  title,
  subtitle,
  icon,
  summary,
  defaultOpen = false,
  children,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  icon?: ReactNode;
  summary?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const id = useId();
  return (
    <div
      className={`settings-card settings-collapsible${open ? " is-open" : ""}`}
    >
      <button
        type="button"
        className="settings-collapsible-header"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((value) => !value)}
      >
        <ChevronRight className="settings-chevron" size={14} />
        {icon && <span className="settings-row-icon">{icon}</span>}
        <span className="settings-row-text">
          <span className="settings-row-title">
            <span>{title}</span>
          </span>
          {subtitle && (
            <span className="settings-row-subtitle">{subtitle}</span>
          )}
        </span>
        {summary && (
          <span className="settings-collapsible-summary">{summary}</span>
        )}
      </button>
      <div className="settings-collapsible-body" id={id} hidden={!open}>
        {children}
      </div>
    </div>
  );
}

/** Extra rows at the end of a card, behind a "More" row: fine-tuning that
 * most people leave alone. */
export function MoreRows({
  label,
  defaultOpen = false,
  children,
}: {
  label: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <>
      <button
        type="button"
        className={`settings-more${open ? " is-open" : ""}`}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <ChevronRight className="settings-chevron" size={13} />
        {label}
      </button>
      {open && children}
    </>
  );
}
