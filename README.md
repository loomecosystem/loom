<p align="center">
  <img src="banner.png" alt="Loom" width="320">
</p>

# Loom

CA : yJMYWHBMTTdk5RxVErGGdxAa7MBFSFGVKq1YDhULoom

A game engine for on-chain worlds, supplied as a protocol.

Most "fully on-chain" games rebuild the same plumbing - state model, ticks,
access control, indexing - from scratch. Loom provides it once: an ECS runtime, a
tick scheduler, and a compute bridge, running on Solana. Worlds built on it share
one state model, so they can reference each other's state.

This is a working implementation of the design in
[`../Loom/PROJECT_SPEC.md`](../Loom/PROJECT_SPEC.md). [`BUILD.md`](BUILD.md) tracks
the status of each phase.

## Layout

```
Loom-codes/
  engine-core/   Rust, no deps - the deterministic core (ECS, schema, ticks, bridge, mods, economy)
  programs/      Rust/Anchor - the on-chain program
  sdk/           TypeScript - client, indexer, codegen, local runtime
  worlds/        reference world - Smallholm
  conformance/   the cross-implementation determinism vector
  docs/          architecture, schema reference, quickstart
```

The engine is split into a deterministic core and a chain shell:

- **`engine-core`** (Rust, zero dependencies) is the state model: an ECS keyed by
  `(world_id, entity_id, component_id)`, an on-chain schema registry, a
  bounded-work tick coordinator with resumable cursors and a dirty set, an
  optimistic compute bridge with a fraud-proof window, policy-gated mods with
  cross-world references, and a fee/treasury/grants ledger. It is pure computation
  over bytes - no I/O, no clock, no randomness - so it host-tests with `cargo test`
  and compiles into the on-chain program behind `no_std`.

- **`@loom/sdk`** (TypeScript) is the client: a typed `LoomClient`, an `Indexer`
  that rebuilds world state from Component accounts, schema-driven codegen, and an
  in-process runtime that mirrors the on-chain program for fast local iteration.

The Rust core and the TypeScript runtime compute the same 64-bit world state hash
for the same state - see [Determinism](#determinism).

## Quick start

Prerequisites: Node >= 22.6 (for native TypeScript), Rust, pnpm. The on-chain
program also needs the Solana and Anchor toolchains.

```bash
pnpm install

# Rust core (38 tests):
cargo test --manifest-path engine-core/Cargo.toml

# on-chain program: type-check + build the deployable SBF bytecode
cargo check -p loom-engine
cargo build-sbf --manifest-path programs/loom-engine/Cargo.toml

# SDK (TypeScript):
pnpm --filter @loom/sdk test

# reference world:
pnpm --filter @loom/world-smallholm test
pnpm --filter @loom/world-smallholm start    # watch it run
```

## Smallholm (reference world)

`worlds/smallholm` is a small autonomous settlement economy that uses the whole
engine. Farmers walk to their plots and turn grain into gold; two rival soldiers
march out and fight; a scout's path around a wall is computed off-chain and
settled through the compute bridge, then walked by a System; and a third-party
tithe mod attaches to the running world without a redeploy. Each NPC's behavior is
a deterministic policy computed from world state and a per-agent seed.
`pnpm --filter @loom/world-smallholm start` renders it tick by tick.

## Determinism

The same scenario, replayed by the Rust core and the TypeScript runtime, produces
the same 64-bit state hash:

```
loom-conformance-v1  ->  0x80b9a6c42a0e765f
```

Both sides check it (`engine-core/tests/conformance.rs` and
`sdk/test/conformance.test.ts`) against
[`conformance/expected.json`](conformance/expected.json). The hash is a digest
over the world: id, frozen flag, schema layouts in id order, then every Component
record in `(component, entity)` order.
