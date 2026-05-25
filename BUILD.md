# Loom - Build Plan

Build bottom-up; each phase has a test that gates it.

- [x] P0 Scaffold - the monorepo builds.
- [x] P1 ECS core - create an entity, a System mutates it, the client reads it back.
- [x] P2 Tick coordinator - advance 10k entities across cranks within CU limits.
- [x] P3 Compute bridge - off-chain pathfinding settled on-chain and consumed.
- [x] P4 Composability - a third-party mod runs against a world without a redeploy.
- [ ] P5 Reference world - an autonomous world with agent NPCs.
- [ ] P6 Mainnet - SDK, docs, grants.

Layout: `engine-core` (Rust core, `cargo test`), `programs` (Anchor shell), `sdk`
(TypeScript client + local runtime), `worlds` (reference worlds).
