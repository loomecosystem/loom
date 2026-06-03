# Loom - Build Plan

The phases, each with a verifiable gate and its status in this repo. Every gate is
backed by a runnable test, named below.

| Phase | Scope | Gate | Status |
|---|---|---|---|
| **P0** Scaffold | Monorepo: engine program + schema registry + TS SDK | Builds | ✅ |
| **P1** ECS core | Component PDAs + schema registry + System framework + codegen | Create entity → System mutates → client reads back | ✅ |
| **P2** Tick coordinator | Permissionless cranks + bounded-work scheduler + resumable cursors | Advance 10k entities across cranks within CU limits | ✅ |
| **P3** Compute bridge | Optimistic results + fraud window (ZK is a follow-up) | Off-chain pathfinding settled on-chain and consumed by a System | ✅ |
| **P4** Composability | External Systems (mods) + cross-world refs + permissioning | A third-party mod runs against an existing world without redeploying it | ✅ |
| **P5** Reference world | A flagship autonomous world on Loom, with agent NPCs | Playable end-to-end; includes agent-driven NPCs | ✅ |
| **Econ** Token & fees | Engine fees, treasury, cranker rewards, grants | A world pays metered fees; protocol + grants accrue exactly | ✅ |
| **P6** Mainnet | SDK + docs + world-builder grants | An *external* team ships a world on Loom | ◻ (out of scope here) |

The bridge ships optimistic-first; the reference world is an on-chain economy.

---

## How each gate is verified

Everything below is reproducible from a clean checkout after `pnpm install`.

### P0 - Builds

- Rust core builds and tests: `cargo test --manifest-path engine-core/Cargo.toml`
- On-chain program type-checks against the Anchor/Solana API:
  `cargo check -p loom-engine` (and `cargo build-sbf` for the deployable `.so` -
  see [On-chain program](#on-chain-program)).
- SDK and world run on Node >= 22.6 with native TypeScript: `pnpm -r test`.

### P1 - ECS core

> Create an entity → a System mutates it → the client reads it back.

- Rust: `engine-core/tests/p1_ecs.rs` -
  `create_entity_system_mutates_client_reads_back`, plus
  `access_control_is_enforced`.
- TypeScript: `sdk/test/ecs.test.ts` - the same flow through `LoomClient` and the
  `Indexer`, plus codegen (`sdk/test/codegen.test.ts`).

### P2 - Tick coordinator

> Advance 10k entities across multiple cranks within CU limits.

- Rust: `engine-core/tests/p2_tick.rs` -
  `advance_10k_entities_across_cranks_within_budget` asserts the sweep spans
  exactly 3 cranks, no crank exceeds the CU ceiling, and every entity advances
  exactly once. Also `dirty_set_touches_only_changed_entities`.
- TypeScript: `sdk/test/tick.test.ts` - the same, client side.

### P3 - Compute bridge

> Off-chain pathfinding result settled on-chain and consumed by a System.

- Rust: `engine-core/tests/p3_bridge.rs` - BFS computes a path around an obstacle
  off-chain; the result is posted optimistically, finalized after the fraud
  window, settled into a `Route` Component, and a `FollowPath` System walks the
  unit along it. Also covers the fraud path and that an honest result cannot be
  slashed.
- TypeScript: `sdk/test/bridge.test.ts` - the same settle-and-consume flow plus the
  fraud path.

### P4 - Composability

> A third-party mod runs against an existing world without redeploying it.

- Rust: `engine-core/tests/p4_mods.rs` - `third_party_mod_runs_against_existing_world`
  admits a `TaxMod` under a `ModPolicy` and runs it over live state; a mod reaching
  outside policy is refused; cross-world references resolve by layout.
- TypeScript: `worlds/smallholm/test/world.test.ts` - a `TitheMod` attaches to the
  running Smallholm world; a `RaidMod` outside policy is refused.

### P5 - Reference world

> Playable end-to-end; includes agent-driven NPCs.

- `worlds/smallholm` - an autonomous settlement economy. Tests in
  `worlds/smallholm/test/world.test.ts` cover determinism, autonomous NPC
  movement, the economy accruing, combat, live mods, and the compute bridge: a
  scout's path is computed off-chain, settled through the bridge, and walked by a
  System.
- `pnpm --filter @loom/world-smallholm start` renders the world tick by tick. It
  runs on the local runtime; the same world code targets a devnet deployment of
  `loom-engine` unchanged.

### Token & fees

> A world pays metered engine fees; protocol revenue and the grants pool accrue
> exactly, with crankers paid to advance world time.

- Rust: `engine-core/tests/economy.rs` - drives the tick loop, billing the world
  per crank, then charges storage rent and disburses a grant, asserting every
  figure to the unit.
- TypeScript: `sdk/test/economy.test.ts` - the same scenario, matching the Rust
  numbers exactly.

### Determinism (cross-cutting)

`engine-core/tests/conformance.rs` and `sdk/test/conformance.test.ts` replay one
fixed scenario and assert the same state hash `0x80b9a6c42a0e765f`, pinned in
`conformance/expected.json`. Rust ↔ TypeScript agreement guards against either
implementation drifting.

---

## On-chain program

`programs/loom-engine` is the Anchor program exposing the engine's instructions
(`initialize_world`, `register_component`, `spawn_entity`, `set_component`,
`freeze_world`) over Solana accounts. Its PDA seed scheme matches
`engine-core/src/addressing.rs` and the SDK's `componentAddress` exactly.

- `cargo check -p loom-engine` - type-checks against anchor-lang 0.31 /
  solana-program 2.3. Verified.
- `cargo build-sbf --manifest-path programs/loom-engine/Cargo.toml` - builds the
  deployable bytecode `target/deploy/loom_engine.so` (program id
  `26jAdoBMqG5zzNDQSRHHt7WA5hqbeZ6Lm33Sd2o96GiC`). Verified. The first run
  downloads the Solana platform-tools.
- `anchor deploy` (or `solana program deploy target/deploy/loom_engine.so`) to
  deploy to a cluster.

The deterministic logic is specified and host-tested in `engine-core`; the program
is the thin shell mapping it onto accounts.

**On-chain execution testing.** An in-process or live-validator test driving the
P1 flow (init world → register component → spawn → set → read) does not run on the
Windows box this was built on: the Rust test frameworks (`solana-program-test`,
`litesvm`) transitively need `openssl-sys`, which has no buildable OpenSSL here,
and `solana-test-validator` needs the symlink privilege (OS error 1314) that
requires Administrator / Developer Mode. Both build and run on Linux/CI; the
program is validated here by type-check + SBF build.

---

## Not built here

- **ZK verification**: the bridge ships optimistic-first. The `post_verified` seam
  (Rust and TS) is where an on-chain validity-proof verifier would finalize a
  result with no fraud window.
- **P6**: an external team shipping a world, mainnet, and a live grants program.
