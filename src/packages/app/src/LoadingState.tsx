import { CloudOff, RefreshCw } from "lucide-react";
import { Button } from "@agent-hud/ui/components/button";

/// Centered placeholder for a pane whose data is not here yet.
/// While loading it shows a quiet spinner; on failure it shows the reason and a Retry button.
export function LoadingState({
  label,
  failed,
  detail,
  retryLabel,
  onRetry,
}: {
  label: string;
  failed?: boolean;
  detail?: string | null;
  retryLabel?: string;
  onRetry?: () => void;
}) {
  return (
    <div className="loading-state" role="status" aria-live="polite">
      {failed ? (
        <span className="loading-state-icon">
          <CloudOff size={20} />
        </span>
      ) : (
        <span className="loading-state-spinner" aria-hidden />
      )}
      <p className="loading-state-label">{label}</p>
      {failed && detail && <p className="loading-state-detail">{detail}</p>}
      {failed && onRetry && (
        <Button variant="outline" size="sm" onClick={onRetry}>
          <RefreshCw size={13} />
          {retryLabel ?? "Retry"}
        </Button>
      )}
    </div>
  );
}
