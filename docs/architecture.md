# Loom Architecture

How the pieces fit and why the design is the way it is.

## Two layers: deterministic core + chain shell

The engine is split deliberately:

```
                ┌─────────────────────────────────────────────┐
   off-chain    │  @loom/sdk (TypeScript)                      │
   clients      │  LoomClient · Indexer · codegen · runtime    │
                └───────────────┬─────────────────────────────┘
                                │ same instruction surface,
                                │ same PDA scheme, same bytes
                ┌───────────────▼─────────────────────────────┐
   on-chain     │  programs/loom-engine (Anchor)               │
                │  accounts + instructions                     │
                └───────────────┬─────────────────────────────┘
                                │ wraps
                ┌───────────────▼─────────────────────────────┐
   core         │  engine-core (Rust, no deps)                 │
                │  ECS · schema · tick · bridge · mods · hash  │
                └─────────────────────────────────────────────┘
```

`engine-core` is pure computation over bytes - no I/O, no clock, no randomness
(time is always an explicit `slot` input). That lets the same logic (a) run
on-chain where Solana execution is deterministic given inputs, (b) host-test with
`cargo test`, and (c) be mirrored by the TypeScript runtime so the two agree on a
[state hash](#determinism).

## The ECS state model

- An **Entity** is a `u64` id allocated by a world.
- A **Component** is a typed, fixed-size record keyed by
  `(world_id, entity_id, component_id)`. On Solana each is the data region of a
  PDA account; off-chain it is an entry in the runtime store.
- A **System** reads and writes Components under engine-enforced access control.
  It declares an [`Access`] (which Components it may read/write), names a query
  Component, and does bounded per-entity work.

### Addressing

Solana has no relational store, so Loom fixes a deterministic PDA addressing
scheme and an on-chain schema registry. Component layouts are registered on-chain,
which gives reflection (decode any Component from its schema) and lets one world
reference another's Components. Address derivation:

```
seeds = ["loom", "cmp", world_id.le, entity_id.le, component_id.le]
```

The on-chain program passes these to `find_program_address`; the SDK and core
derive the same 32-byte identity by hashing the seeds (FNV-1a, four domain-
separated lanes). The indexer uses this address as the join key, so no external
index is needed to find a Component.

## Bounded-work ticks

A Solana transaction cannot iterate thousands of entities - it would exceed the
compute-budget (CU) limit. So a System sweep is split across **cranks**:

- Each crank processes at most `budget.maxEntities()` entities, then writes a
  resumable **cursor** (the next entity id) back to the world.
- Anyone can submit the next crank; crankers are paid per entity advanced
  (`cranker_reward`).
- Iteration follows the store's key order, so cranks resume exactly where the last
  stopped - no entity skipped or processed twice.
- A **dirty set** lets a sweep touch only the entities that changed last epoch.

This is tested directly: 10,000 entities advance across exactly three CU-bounded
cranks.

## Compute bridge

Heavy work - pathfinding, physics, AI - runs off-chain and is settled on-chain.
v1 is optimistic: a worker posts a result with a bond, opening a fraud-proof
window. Unchallenged, it finalizes and a System consumes it; a challenger who
re-derives a different result inside the window slashes the poster. The
`post_verified` seam is where a ZK validity proof, checked by an on-chain
verifier, could finalize immediately with no window.

## Composability

A **mod** is a new System operating on an existing world's Components - no
redeploy. The world publishes a `ModPolicy` (which Components external Systems may
touch); `admit_mod` checks a candidate's declared `Access` against it, and the
engine's per-access enforcement keeps the mod within what it declared.
**Cross-world references** resolve a Component in another world by its
`layout_hash`, so world B can read world A's state as long as A still exposes a
byte-compatible layout.

## Token and value capture

Worlds prepay engine fees and are charged per crank, per entity processed, and per
Component byte of storage. Crankers take a share of each crank's fee; the
remainder splits between protocol revenue and a grants pool, and the token is the
settlement asset for the compute bridge. The accounting is exact integer math, so
the SDK reproduces every figure the chain computes. The [`economy`](../engine-core/src/economy.rs)
module is orthogonal to the core - the ECS, ticks, and bridge run whether or not
anyone is metering them.

## Governance

A world has an `authority` who may register schemas and `freeze` the world. Once
frozen, the schemas are locked permanently; game state still evolves via Systems,
but the rules do not.

## Agent NPCs

An NPC's behavior is a deterministic System policy. The reference world Smallholm
drives farmers and soldiers whose decisions are computed on-chain from world state
and a per-agent seed, with no off-chain authority needed for them to act.

## Determinism

A world reduces to a single FNV-1a digest over a canonical encoding: world id,
frozen flag, schema layouts in id order, then every Component record in
`(component, entity)` order. The Rust core and the TypeScript runtime compute the
same digest for the same state, checked in `conformance/`.

[`Access`]: ../engine-core/src/system.rs
