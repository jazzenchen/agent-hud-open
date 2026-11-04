import { Tabs, TabsList, TabsTrigger } from "@agent-hud/ui/components/tabs";
import type { StatsTheme } from "./StatsTheme";

/** The Statistics window's segmented control; `small` on the Sessions page. */
export function Segments({
  value,
  onChange,
  choices,
  theme,
  label,
  small = false,
}: {
  value: string;
  onChange: (value: string) => void;
  choices: { value: string; label: string }[];
  theme: Pick<StatsTheme, "light" | "text" | "segment" | "selected">;
  label: string;
  small?: boolean;
}) {
  return (
    <Tabs
      value={value}
      onValueChange={onChange}
      style={{ gap: 0 }}
      aria-label={label}
    >
      <TabsList
        style={{
          background: theme.segment,
          borderRadius: small ? 6 : 7,
          height: small ? 26 : 28,
          padding: 2,
          gap: 1,
        }}
      >
        {choices.map((choice) => (
          <TabsTrigger
            key={choice.value}
            value={choice.value}
            style={{
              height: small ? 22 : 24,
              padding: small ? "2px 12px" : "3px 11px",
              border: 0,
              borderRadius: small ? 4 : 5,
              fontSize: small ? 11 : 12,
              fontWeight: value === choice.value ? 500 : 400,
              lineHeight: small ? "14px" : "16px",
              color: theme.text,
              boxShadow:
                value === choice.value && theme.light
                  ? "0 1px 2px rgba(0,0,0,.08)"
                  : "none",
              background:
                value === choice.value ? theme.selected : "transparent",
              transition:
                "background-color 160ms ease, color 160ms ease, box-shadow 160ms ease",
            }}
          >
            {choice.label}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  );
}
