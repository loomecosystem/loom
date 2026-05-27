# Schema & Wire Reference

This is the byte-level contract shared by `engine-core` (Rust), `@loom/sdk`
(TypeScript), and the on-chain program. Both implementations encode identically;
that is what makes the [determinism conformance](../conformance/expected.json)
check meaningful.

## Field types

Components are fixed-size scalar records. Every field has a fixed width, a fixed
little-endian encoding, and a stable wire tag.

| Type | Size (bytes) | Wire tag | TS type | Notes |
|---|---|---|---|---|
| `u8` | 1 | 1 | `number` | |
| `u16` | 2 | 2 | `number` | LE |
| `u32` | 4 | 3 | `number` | LE |
| `u64` | 8 | 4 | `bigint` | LE |
| `i64` | 8 | 5 | `bigint` | LE, two's complement |
| `bool` | 1 | 6 | `boolean` | `0` / `1` |
| `pubkey` | 32 | 7 | `Uint8Array` | a Solana public key |
| `entity` | 8 | 8 | `bigint` | LE; an entity id reference |
| `bytes(n)` | n | 9 | `Uint8Array` | fixed-length blob |

A record's size is the sum of its fields' sizes. Fields are laid out in
declaration order with no padding.

## Identifiers

| Id | Type | Allocated by |
|---|---|---|
| `world_id` | `u64` | the deployer |
| `entity_id` | `u64` | the world, monotonically (`0` = null entity) |
| `component_id` | `u32` | the world's schema registry, monotonically |
| `system_id` | `u32` | the world author |

## Component addressing

The deterministic identity of a Component record. On-chain it is a PDA; off-chain
the same 32-byte value is derived by hashing the seeds.

```
seeds = ["loom", "cmp", world_id.le_u64, entity_id.le_u64, component_id.le_u32]
```

Off-chain derivation (`componentAddress`, `component_address`): four 8-byte lanes,
each `FNV1a( "loom:cmp" ‖ lane_u8 ‖ world_id.le ‖ entity_id.le ‖ component_id.le )`
written little-endian. Cross-checked vector:

```
componentAddress(world=1, entity=1, component=0)
  = eab4eb3cf4099ec1cd7f5bf8baab4771a41e67905fe5a5ae87e9d64b26874f5e
```

## FNV-1a 64

The hash underlying layout digests, addresses, and state hashes. Offset basis
`0xcbf29ce484222325`, prime `0x100000001b3`, wrapping `u64`. Integers are absorbed
little-endian. Vector: `FNV1a("loom") = 0xcdecefad70d5909c`.

## Layout hash

A structural digest of a Component schema; equal layouts are byte-compatible
(the condition for a cross-world reference). Encoding:

```
FNV1a(
  "loom:schema" ‖ id.le_u32 ‖ name_utf8
  ‖  for each field:  0xff ‖ name_utf8 ‖ tag_u8  [‖ n.le_u16  if bytes(n)]
)
```

## State hash

A canonical digest of an entire world. Encoding:

```
FNV1a(
  "loom:world" ‖ world_id.le_u64 ‖ frozen_u8
  ‖  for each schema, ascending component_id:   id.le_u32 ‖ layout_hash.le_u64
  ‖  for each record, ascending (component_id, entity_id):
        component_id.le_u32 ‖ entity_id.le_u64 ‖ len.le_u32 ‖ bytes
)
```

Ascending iteration order is part of the contract - both implementations sort by
`(component_id, entity_id)`. Conformance vector (`loom-conformance-v1`):
`0x80b9a6c42a0e765f`.
