//! Cross-implementation determinism conformance.
//!
//! Builds a fixed scenario and asserts its [`World::state_hash`]. The same
//! scenario is replayed by the TypeScript SDK runtime
//! (`sdk/test/conformance.test.ts`), which must arrive at the same digest.
//!
//! The expected value is mirrored in `conformance/expected.json`.

use loom_engine_core::ids::{ComponentId, EntityId, SystemId};
use loom_engine_core::schema::{Field, FieldType};
use loom_engine_core::system::{Access, System, SystemCtx};
use loom_engine_core::tick::{run_to_completion, Budget};
use loom_engine_core::{EngineError, World};

/// Keep this scenario in lockstep with `sdk/test/conformance.test.ts`.
pub const SCENARIO_ID: &str = "loom-conformance-v1";
/// Pinned from the first run; a regression check thereafter.
const EXPECTED_STATE_HASH: u64 = 0x80b9_a6c4_2a0e_765f;

struct Movement {
    pos: ComponentId,
    vel: ComponentId,
}
impl System for Movement {
    fn id(&self) -> SystemId {
        1
    }
    fn access(&self) -> Access {
        Access::new().reads([self.vel]).writes([self.pos])
    }
    fn query(&self) -> ComponentId {
        self.pos
    }
    fn run(&self, ctx: &mut SystemCtx<'_>, e: EntityId) -> Result<(), EngineError> {
        let dx = ctx.read_i64(self.vel, e, "dx")?;
        let dy = ctx.read_i64(self.vel, e, "dy")?;
        ctx.mutate(self.pos, e, |r| {
            let x = r.get_i64("x")?;
            let y = r.get_i64("y")?;
            r.set_i64("x", x + dx)?;
            r.set_i64("y", y + dy)?;
            Ok(())
        })
    }
}

fn build_scenario() -> World {
    let mut world = World::new(42, [0u8; 32]);
    let pos = world
        .register_component(
            "Position",
            vec![
                Field::new("x", FieldType::I64),
                Field::new("y", FieldType::I64),
            ],
        )
        .unwrap();
    let vel = world
        .register_component(
            "Velocity",
            vec![
                Field::new("dx", FieldType::I64),
                Field::new("dy", FieldType::I64),
            ],
        )
        .unwrap();
    let health = world
        .register_component(
            "Health",
            vec![
                Field::new("hp", FieldType::U32),
                Field::new("alive", FieldType::Bool),
            ],
        )
        .unwrap();

    for i in 1..=6i64 {
        let e = world.spawn_entity();
        let mut p = world.record(pos).unwrap();
        p.set_i64("x", i).unwrap().set_i64("y", 2 * i).unwrap();
        world.set(pos, e, p.into_bytes()).unwrap();

        let mut v = world.record(vel).unwrap();
        v.set_i64("dx", i).unwrap().set_i64("dy", -i).unwrap();
        world.set(vel, e, v.into_bytes()).unwrap();

        let mut h = world.record(health).unwrap();
        h.set_u32("hp", (100 - 10 * i) as u32)
            .unwrap()
            .set_bool("alive", i % 2 == 1)
            .unwrap();
        world.set(health, e, h.into_bytes()).unwrap();
    }

    // Three sweeps of the Movement system.
    let movement = Movement { pos, vel };
    for _ in 0..3 {
        run_to_completion(&mut world, &movement, Budget::new(1_000_000, 1_000), 0).unwrap();
    }

    world
}

#[test]
fn scenario_state_hash_is_pinned() {
    let world = build_scenario();
    let h = world.state_hash();
    println!("CONFORMANCE {SCENARIO_ID} state_hash = {h:#018x}");

    // After 3 sweeps: x = 4*i, y = -i. Spot-check before hashing.
    let p3 = world.read(0, 3).unwrap().unwrap();
    assert_eq!(p3.get_i64("x").unwrap(), 12);
    assert_eq!(p3.get_i64("y").unwrap(), -3);

    assert_eq!(
        h, EXPECTED_STATE_HASH,
        "state hash drifted; if intentional, update EXPECTED_STATE_HASH and conformance/expected.json"
    );
}
