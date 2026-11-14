import type { Provider } from "@agent-hud/protocol";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { Translate } from "@agent-hud/ui/format";

/** The agents, found ones first; the rest under a small label, still
 * selectable so they can be set up before they are installed. */
export function AgentList({
  t,
  providers,
  selected,
  select,
}: {
  t: Translate;
  providers: Provider[];
  selected?: string;
  select: (id: string) => void;
}) {
  const found = providers.filter((provider) => provider.detected);
  const missing = providers.filter((provider) => !provider.detected);
  const item = (provider: Provider) => (
    <button
      key={provider.id}
      type="button"
      className={`agents-list-item${provider.id === selected ? " is-selected" : ""}${provider.enabled ? "" : " is-off"}`}
      aria-current={provider.id === selected ? "true" : undefined}
      onClick={() => select(provider.id)}
    >
      <HUDLogo source={provider.id} size={18} />
      <span className="agents-list-name">{provider.name}</span>
      {!provider.enabled && (
        <span className="agents-list-state">{t("Off")}</span>
      )}
    </button>
  );
  return (
    <nav
      className="agents-list"
      data-guide="agents-list"
      aria-label={t("Agents")}
    >
      {found.map(item)}
      {missing.length > 0 && (
        <>
          <div className="agents-list-label">{t("Not on this computer")}</div>
          {missing.map(item)}
        </>
      )}
    </nav>
  );
}
