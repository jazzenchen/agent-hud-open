/* The building blocks every Settings page is made of, so all pages share one
 * look: a section is a heading over a card, a card holds rows, a row is words
 * on the left and its control on the right. Spacing, type and dividers live in
 * settings.css; pages never style these by hand. */
import { useId, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import type { NativeMethod, Snapshot } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import { Slider } from "@agent-hud/ui/components/slider";
import { Switch } from "@agent-hud/ui/components/switch";
import { Tabs, TabsList, TabsTrigger } from "@agent-hud/ui/components/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@agent-hud/ui/components/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-hud/ui/components/tooltip";
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

const nativeUnavailable =
  "This native integration is not available in this version.";

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
  children,
}: {
  title?: ReactNode;
  subtitle?: ReactNode;
  action?: ReactNode;
  footer?: ReactNode;
  plain?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="settings-section">
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
  children,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  tag?: ReactNode;
  icon?: ReactNode;
  detail?: ReactNode;
  below?: ReactNode;
  muted?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className={`settings-row${muted ? " is-muted" : ""}`}>
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

/** A disabled control with the reason on hover. */
export function Unavailable({
  children,
  reason = nativeUnavailable,
}: {
  children: ReactNode;
  reason?: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span tabIndex={0} style={{ display: "inline-flex" }}>
          {children}
        </span>
      </TooltipTrigger>
      <TooltipContent style={{ maxWidth: 280 }}>{reason}</TooltipContent>
    </Tooltip>
  );
}

export function ToggleRow({
  title,
  subtitle,
  checked,
  change,
  disabled,
  tag,
  icon,
  reason,
  below,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  checked: boolean;
  change?: (value: boolean) => void;
  disabled?: boolean;
  tag?: ReactNode;
  icon?: ReactNode;
  reason?: string;
  below?: ReactNode;
}) {
  const control = (
    <Switch
      aria-label={typeof title === "string" ? title : undefined}
      checked={checked}
      onCheckedChange={change}
      disabled={disabled}
    />
  );
  return (
    <Row title={title} subtitle={subtitle} tag={tag} icon={icon} below={below}>
      {disabled && reason ? (
        <Unavailable reason={reason}>{control}</Unavailable>
      ) : (
        control
      )}
    </Row>
  );
}

export function Segments({
  label,
  value,
  options,
  change,
  disabled = false,
}: {
  label: string;
  value: string;
  options: [string, string][];
  change: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <Tabs value={value} onValueChange={change} className="settings-segments">
      <TabsList aria-label={label}>
        {options.map(([key, name]) => (
          <TabsTrigger key={key} value={key} disabled={disabled}>
            {name}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  );
}

export function Menu({
  label,
  value,
  options,
  change,
  disabled = false,
  reason,
  width = 140,
  placeholder,
}: {
  label: string;
  value: string;
  options: [string, ReactNode][];
  change?: (value: string) => void;
  disabled?: boolean;
  reason?: string;
  width?: number | "auto";
  placeholder?: string;
}) {
  const control = (
    <Select value={value} onValueChange={change} disabled={disabled}>
      <SelectTrigger
        aria-label={label}
        className="settings-select"
        size="sm"
        style={{ width }}
      >
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {options.map(([key, name]) => (
          <SelectItem key={key} value={key} className="settings-select-item">
            {name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
  return disabled && reason ? (
    <Unavailable reason={reason}>{control}</Unavailable>
  ) : (
    control
  );
}

/** A slider row: name, track, value. */
export function Slide({
  title,
  value,
  min,
  max,
  step,
  change,
  format,
  disabled = false,
}: {
  title: string;
  value: number;
  min: number;
  max: number;
  step: number;
  change: (value: number) => void;
  format: (value: number) => string;
  disabled?: boolean;
}) {
  return (
    <div className="settings-row settings-slider-row">
      <div className="settings-row-main">
        <label className="settings-slider-label">{title}</label>
        <Slider
          aria-label={title}
          min={min}
          max={max}
          step={step}
          value={[value]}
          disabled={disabled}
          onValueChange={(values) => change(values[0])}
          className="settings-slider"
        />
        <output className="settings-slider-value">{format(value)}</output>
      </div>
    </div>
  );
}

export function SmallButton({
  children,
  onClick,
  disabled = false,
  variant = "outline",
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  variant?: "outline" | "default" | "destructive";
}) {
  return (
    <Button
      variant={variant}
      size="sm"
      disabled={disabled}
      onClick={onClick}
      className="settings-button"
    >
      {children}
    </Button>
  );
}

/** A text-only action, for "Change key", "Copy" and the like. */
export function LinkButton({
  children,
  onClick,
  href,
  disabled = false,
  tone = "accent",
}: {
  children: ReactNode;
  onClick?: () => void;
  href?: string;
  disabled?: boolean;
  tone?: "accent" | "destructive";
}) {
  const className = `settings-link${tone === "destructive" ? " is-destructive" : ""}`;
  return href ? (
    <a className={className} href={href} target="_blank" rel="noreferrer">
      {children}
    </a>
  ) : (
    <button
      type="button"
      className={className}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
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
