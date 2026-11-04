import { Component, type ReactNode } from "react";
import { Button } from "@agent-hud/ui/components/button";

export class ViewErrorBoundary extends Component<
  { children: ReactNode; onReset?: () => void; chinese?: boolean },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  render() {
    if (!this.state.error) return this.props.children;
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
        <span>
          {this.props.chinese
            ? "无法显示这个页面："
            : "Failed to render view: "}
          {this.state.error.message}
        </span>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            this.setState({ error: null });
            this.props.onReset?.();
          }}
        >
          {this.props.chinese ? "返回" : "Back"}
        </Button>
      </div>
    );
  }
}
