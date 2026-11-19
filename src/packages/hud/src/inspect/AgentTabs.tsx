import { useState } from "react";
import { HUDArtwork } from "../HUDArtwork";
import { clientDisplayName } from "../IslandNews";
import { useGround } from "../ground";
import { alertInk } from "../tokens";

/** The clients the window sends to, in the order their tabs show. */
const ORDER = ["claude", "codex", "antigravity", "deepseek", "pi", "opencode"];

/** The clients with a session or a new one to offer. */
export function tabClients(clients: Iterable<string>): string[] {
  const offered = new Set(clients);
  return ORDER.filter((client) => offered.has(client));
}

function Tab({
  client,
  chosen,
  onPick,
  light,
}: {
  client: string;
  chosen: boolean;
  onPick: () => void;
  light: boolean;
}) {
  const ground = useGround(light);
  const [hover, setHover] = useState(false);
  const tint = light ? "rgba(0,0,0," : "rgba(255,255,255,";
  return (
    <button
      type="button"
      role="tab"
      aria-selected={chosen}
      onClick={onPick}
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 5,
        // As wide as its name, sharing what is left: a longer name such as
        // Antigravity's is not cut while four tabs fit.
        flex: "1 1 auto",
        minWidth: 0,
        justifyContent: "center",
        padding: "5px 6px",
        border: 0,
        borderRadius: 7,
        background: chosen
          ? `${tint}${light ? 0.08 : 0.12})`
          : hover
            ? `${tint}${light ? 0.04 : 0.06})`
            : "transparent",
        color: alertInk(chosen ? 0.92 : 0.55, ground),
        fontSize: 11.5,
        fontWeight: chosen ? 600 : 500,
        cursor: "pointer",
      }}
    >
      <HUDArtwork source={client} size={12} light={light} />
      <span
        style={{
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {clientDisplayName(client)}
      </span>
    </button>
  );
}

/** One tab per client over the list of places: the list shows one client's
 * sessions at a time. */
export function AgentTabs({
  clients,
  chosen,
  pick,
  light,
}: {
  clients: string[];
  chosen: string;
  pick: (client: string) => void;
  light: boolean;
}) {
  return (
    <div role="tablist" style={{ display: "flex", gap: 2, padding: 1 }}>
      {clients.map((client) => (
        <Tab
          key={client}
          client={client}
          chosen={client === chosen}
          onPick={() => pick(client)}
          light={light}
        />
      ))}
    </div>
  );
}
