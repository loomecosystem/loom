# Quickstart - build a world on Loom

This walks through building a minimal world with the TypeScript SDK. The SDK runs
on Node ≥ 22.6 with native TypeScript (no build step). The same world code targets
a deployed `loom-engine` program unchanged.

## 1. Install

```bash
pnpm install
```

## 2. Define Components

Components are fixed-size typed records. Register them against a client:

```ts
import { LoomClient, field } from "@loom/sdk";

const loom = LoomClient.local(1n); // worldId = 1

const Position = loom.registerComponent("Position", [
  field("x", "i64"),
  field("y", "i64"),
]);
const Velocity = loom.registerComponent("Velocity", [
  field("dx", "i64"),
  field("dy", "i64"),
]);
```

Field types: `u8 u16 u32 u64 i64 bool pubkey entity` and `{ bytes: n }`. 64-bit
fields are `bigint` in TypeScript. See [schema-reference.md](schema-reference.md).

## 3. Spawn entities and write Components

```ts
const e = loom.spawn();
loom.set(Position, e, (r) => r.setI64("x", 10n).setI64("y", 20n));
loom.set(Velocity, e, (r) => r.setI64("dx", 1n).setI64("dy", -1n));
```

## 4. Write a System

A System declares its access, names the Component whose entities it iterates, and
does bounded per-entity work. The engine refuses any read/write it didn't declare.

```ts
import { Access, type System, SystemCtx } from "@loom/sdk";

class Movement implements System {
  constructor(private pos: number, private vel: number) {}
  id() { return 1; }
  access() { return Access.new().withReads([this.vel]).withWrites([this.pos]); }
  query() { return this.pos; }
  run(ctx: SystemCtx, e: bigint) {
    const dx = ctx.readI64(this.vel, e, "dx");
    const dy = ctx.readI64(this.vel, e, "dy");
    ctx.mutate(this.pos, e, (r) => {
      r.setI64("x", r.getI64("x") + dx);
      r.setI64("y", r.getI64("y") + dy);
    });
  }
}
```

> Note: in source that runs under Node's type-stripping, avoid constructor
> parameter properties - declare fields explicitly. (TypeScript build tooling
> accepts the shorthand above.)

## 5. Tick the world

```ts
loom.tick(new Movement(Position, Velocity)); // runs to completion across bounded cranks

const p = loom.get(Position, e);  // { x: 11n, y: 19n }
```

For explicit, bounded ticking (as on-chain crankers do):

```ts
import { startCursor, Budget } from "@loom/sdk";
const cursor = startCursor();
const budget = new Budget(1_400_000n, 1_000n); // CU ceiling, CU/entity
while (!cursor.done) loom.crank(system, cursor, slot, budget);
```

## 6. Read world state (indexing)

```ts
const idx = loom.indexer();
idx.entityView(e);            // { Position: {x,y}, Velocity: {dx,dy} }
idx.componentTable(Position); // [{ entity, address, fields }, ...]
idx.snapshot();               // everything, keyed by Component name
```

## 7. Generate a typed client

```ts
console.log(loom.codegen()); // typed interfaces + id constants from the schema
```

## Going on-chain

The local `LoomClient` mirrors the on-chain program. To target a real cluster,
build and deploy `programs/loom-engine`:

```bash
cargo build-sbf --manifest-path programs/loom-engine/Cargo.toml
anchor keys sync && anchor deploy
```

Your world code - Components, Systems, ticks - does not change; only the client's
transport does.

## See it run

```bash
pnpm --filter @loom/world-smallholm start
```

Renders the reference world (an autonomous economy with agent-driven NPCs) tick by
tick. Source: `worlds/smallholm/`.
