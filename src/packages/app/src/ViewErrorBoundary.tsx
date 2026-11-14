import { Component, type ReactNode } from "react";
import { Button } from "@agent-hud/ui/components/button";
import { useT } from "@agent-hud/ui/format-context";

export class ViewErrorBoundary extends Component<
  {
    children: ReactNode;
    onReset?: () => void;
  },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  render() {
    if (!this.state.error) return this.props.children;
    return (
      <ViewError
        message={this.state.error.message}
        onBack={() => {
          this.setState({ error: null });
          this.props.onReset?.();
        }}
      />
    );
  }
}

function ViewError({
  message,
  onBack,
}: {
  message: string;
  onBack: () => void;
}) {
  const t = useT();
  return (
    <div
      className="connection-placeholder"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 10,
      }}
    >
      <span>{t("Failed to render view: {{message}}", { message })}</span>
      <Button variant="outline" size="sm" onClick={onBack}>
        {t("Back")}
      </Button>
    </div>
  );
}
