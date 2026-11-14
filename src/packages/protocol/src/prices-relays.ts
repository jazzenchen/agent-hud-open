// The price page and the relays the clients send calls through.

/** A price per million tokens; an empty cache rate takes the input's. */
export interface CustomPrice {
  currency: "USD" | "CNY";
  input: number;
  output: number;
  cacheRead: number | null;
  cacheWrite: number | null;
}
/** One model used in the last 30 days, on the price page. */
export interface PriceRow {
  id: string;
  source: string;
  client: string;
  /** The model as recorded, with its route: the key of its custom price. */
  model: string;
  name: string;
  route: string | null;
  routeKind: "local-server" | "cloud" | "relay" | "local" | "provider" | null;
  tokens: number;
  /** Per million tokens at the base rates; null when nothing prices it. */
  price: {
    origin:
      "custom" | "local" | "relay" | "multiplier" | "catalog" | "external";
    currency: string;
    input: number;
    output: number;
    cacheRead: number;
    cacheWrite: number;
  } | null;
  custom: CustomPrice | null;
}
/** From Core (`price_list`): the price page. */
export interface PriceList {
  kind: "prices";
  models: PriceRow[];
  relays: { host: string; multiplier: number | null; listed: boolean }[];
  localHosts: { host: string; kind: "model" | "gateway" | null }[];
}
/** A relay, one per address: found in Claude Code's settings env or Codex's providers, or added by hand. */
export interface RelaySource {
  /** The relay's host. */
  provider: string;
  product: "relay";
  /** The clients configured with it. */
  clients: string[];
  /** Who uses it: those clients, then the agents whose calls went through it in the last 30 days. */
  agents: string[];
  consented: boolean;
  /** What it last reported, in its own money. */
  balance?: { label: string; amount: number; currency: string };
  /** Which template answered: newapi, sub2api, openrouter, custom. */
  kind?: string;
  /** Why no balance was read. */
  notice?: string;
  /** No template fit; the owner may choose one (`set_relay_kind`). */
  unrecognized?: boolean;
  /** The template the owner chose for this address, and a custom one's endpoint. */
  chosen?: RelayKind;
  custom?: RelayCustom;
  checkedAt?: string | null;
  /** Added by hand in Settings, with its id and the name given, if any. */
  manual?: boolean;
  id?: string;
  name?: string;
}
/** How a relay's balance is read: sniffed, one of the common templates, or the owner's own endpoint. */
export type RelayKind = "auto" | "newapi" | "sub2api" | "openrouter" | "custom";
/** A balance endpoint on the relay's own address, and where the balance is in its answer. */
export interface RelayCustom {
  url: string;
  remaining: string;
  divisor: number;
  currency: "USD" | "CNY";
  header?: string | null;
}
