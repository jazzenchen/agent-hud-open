/* What every Display section shares: the selected screen's settings, the way
 * to change them, the language, and the slider and switch rows built on
 * them. */
import type { HUDConfig } from "@agent-hud/hud/model";
import { Slide, ToggleRow } from "../SettingsControls";
import type { Translate } from "@agent-hud/ui/format";

export interface DisplayProps {
  /** The selected screen's settings, with defaults filled in. */
  config: HUDConfig;
  update: (patch: Partial<HUDConfig>) => void;
  t: Translate;
}

type KeysOf<T> = {
  [K in keyof HUDConfig]: HUDConfig[K] extends T ? K : never;
}[keyof HUDConfig];

export function displayControls({ config, update, t }: DisplayProps) {
  const pt = (value: number) => `${Math.round(value)} pt`;
  const px = (value: number) => `${Math.round(value)} px`;
  const ms = (value: number) => `${Math.round(value)} ms`;
  const percent = (value: number) => `${Math.round(value * 100)}%`;
  const seconds = (value: number) =>
    t("{{seconds}} s", { seconds: value.toFixed(1) });
  const rows = (value: number) =>
    t("{{count}} rows", { count: Math.round(value) });

  const slider = (
    key: KeysOf<number>,
    title: string,
    min: number,
    max: number,
    step: number,
    format: (value: number) => string,
  ) => (
    <Slide
      title={title}
      value={config[key]}
      min={min}
      max={max}
      step={step}
      format={format}
      change={(value) => update({ [key]: value })}
    />
  );

  const toggle = (key: KeysOf<boolean>, title: string, subtitle?: string) => (
    <ToggleRow
      title={title}
      subtitle={subtitle}
      checked={config[key]}
      change={(value) => update({ [key]: value })}
    />
  );

  return { slider, toggle, pt, px, ms, percent, seconds, rows };
}
