# Shared parity fixtures

Cases that pin the behavior of this repository against the native Swift app, agent-hud-native-open. The Swift app is the floor, not the ceiling: by default this app must produce what the Swift app produces, and where this app is deliberately better, `decisions.json` records its value and the reason. Either way the case stays tested.

## Layout

- `contracts/<name>.json` — one contract per file: the function family, which side of this app implements it (`owner`: `rust` or `ts`), the Swift source it comes from, and its cases. Each case has a `name`, the Swift function (`fn`), its `args`, and `expected` per language (`en`, `zh`).
- `decisions.json`:
  - `gaps` — cases where this app does not yet match the Swift app, keyed `contract/case` or `contract/case/language`, with the parity-ledger row. A gap that starts passing fails the run until it is removed.
  - `improvements` — cases where this app intentionally differs, with its value (`next`, or `{en, zh}`) and the `reason`. An improvement equal to the Swift app's value fails the run.

## Running

```sh
pnpm run test:ts                         # TypeScript-owned contracts, with the packages' tests
cargo test -p agent-hud-core --test parity   # Rust-owned contracts
```

Both run in CI and need nothing but this repository.

## Adding or changing cases

Write the case's `name`, `fn` and `args`, then let Open compute `expected`:

```sh
pnpm run parity:oracle
```

The oracle (`src/tooling/parity-oracle`) is a Swift package that calls agent-hud-native-open's own functions. It needs Xcode and an agent-hud-native-open checkout beside this repository, or its path in `AGENT_HUD_NATIVE_OPEN`. It fills in `expected` in both languages and never changes inputs. Never edit `expected` by hand; record a deliberate difference in `decisions.json` instead.

The reference is agent-hud-native-open commit `014524c`, the same as the [parity ledger](../../../docs/parity/README.md).
