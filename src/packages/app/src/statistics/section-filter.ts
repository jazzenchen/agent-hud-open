import { useEffect, useState, type Dispatch, type SetStateAction } from "react";
import { readJSON } from "../lib/storage-keys.ts";

/** How a statistics section's agent and model filter reads and changes what
 * it shows. A model is shown or hidden by its statistics id
 * (`<source>-model:<model>`), wherever it is listed. */
export interface FilterState {
  agentShown: (id: string) => boolean;
  modelShown: (id: string) => boolean;
  setAgents: (ids: string[], shown: boolean) => void;
  setModels: (ids: string[], shown: boolean) => void;
  clear: () => void;
}

/** What a section hides, saved on this Mac. */
export interface Hidden {
  agents: string[];
  models: string[];
}

const strings = (value: unknown): string[] =>
  Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];

/** `list` with `ids` shown again, or hidden. */
function toggled(list: string[], ids: string[], shown: boolean): string[] {
  return shown
    ? list.filter((id) => !ids.includes(id))
    : [...new Set([...list, ...ids])].sort();
}

/** A filter that keeps what is hidden under `key`, so agents and models that
 * appear later show until they are hidden too. */
export function useHiddenFilter(key: string): FilterState & { hidden: Hidden } {
  const [hidden, setHidden] = useState<Hidden>(() => {
    const saved = readJSON(key) as Partial<Hidden> | null;
    return { agents: strings(saved?.agents), models: strings(saved?.models) };
  });
  useEffect(() => {
    try {
      if (hidden.agents.length || hidden.models.length)
        localStorage.setItem(key, JSON.stringify(hidden));
      else localStorage.removeItem(key);
    } catch {}
  }, [key, hidden]);
  return {
    hidden,
    agentShown: (id) => !hidden.agents.includes(id),
    modelShown: (id) => !hidden.models.includes(id),
    setAgents: (ids, shown) =>
      setHidden((previous) => ({
        ...previous,
        agents: toggled(previous.agents, ids, shown),
      })),
    setModels: (ids, shown) =>
      setHidden((previous) => ({
        ...previous,
        models: toggled(previous.models, ids, shown),
      })),
    clear: () => setHidden({ agents: [], models: [] }),
  };
}

/** A selection of `all` with `ids` added or taken out; null once it holds
 * every one of `all` again, as "everything" is asked for. */
export function narrowed(
  previous: string[] | null,
  all: string[],
  ids: string[],
  shown: boolean,
): string[] | null {
  const values = previous ?? all;
  const next = shown
    ? [...new Set([...values, ...ids])]
    : values.filter((value) => !ids.includes(value));
  return all.every((id) => next.includes(id)) ? null : next.sort();
}

/** The Tokens section's filter: the agents and models Core is asked for,
 * null for all of them. */
export function selectionFilter(
  providers: string[] | null,
  setProviders: Dispatch<SetStateAction<string[] | null>>,
  providerIds: string[],
  models: string[] | null,
  setModels: Dispatch<SetStateAction<string[] | null>>,
  modelIds: string[],
): FilterState {
  return {
    agentShown: (id) => providers === null || providers.includes(id),
    modelShown: (id) => models === null || models.includes(id),
    setAgents: (ids, shown) =>
      setProviders((previous) => narrowed(previous, providerIds, ids, shown)),
    setModels: (ids, shown) =>
      setModels((previous) => narrowed(previous, modelIds, ids, shown)),
    clear: () => {
      setProviders(null);
      setModels(null);
    },
  };
}
