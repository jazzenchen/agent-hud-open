/* The controls that sit at a Settings row's right: switches, segments, menus,
 * sliders and buttons, and the disabled state that says why. */
import type { ReactNode } from "react";
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
import { Row } from "./SettingsPrimitives";

const nativeUnavailable =
  "This native integration is not available in this version.";

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
  anchor,
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
  /** What the guide calls this row (`data-guide`). */
  anchor?: string;
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
    <Row
      title={title}
      subtitle={subtitle}
      tag={tag}
      icon={icon}
      below={below}
      anchor={anchor}
    >
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
