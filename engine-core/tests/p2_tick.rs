//! Advance 10k entities across multiple cranks within CU limits.

use loom_engine_core::ids::{ComponentId, EntityId, SystemId};
use loom_engine_core::schema::{Field, FieldType};
use loom_engine_core::system::{Access, System, SystemCtx};
use loom_engine_core::tick::{crank, Budget, Cursor};
use loom_engine_core::{EngineError, World};

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
        ctx.mutate(self.pos, e, |r| {
            let x = r.get_i64("x")?;
            r.set_i64("x", x + dx)?;
            Ok(())
        })
    }
}

const N: u64 = 10_000;

#[test]
fn advance_10k_entities_across_cranks_within_budget() {
    let mut world = World::new(1, [0u8; 32]);
    let pos = world
        .register_component("Position", vec![Field::new("x", FieldType::I64)])
        .unwrap();
    let vel = world
        .register_component("Velocity", vec![Field::new("dx", FieldType::I64)])
        .unwrap();

    for _ in 0..N {
        let e = world.spawn_entity();
        world.set(pos, e, world.record(pos).unwrap().into_bytes()).unwrap();
        let mut v = world.record(vel).unwrap();
        v.set_i64("dx", 1).unwrap();
        world.set(vel, e, v.into_bytes()).unwrap();
    }

    // A per-crank ceiling that does NOT fit all 10k entities: 1.0M CU at 250
    // CU/entity => 4000 entities per crank. So the sweep must span ceil(10000/4000)
    // = 3 cranks, each staying under the CU ceiling.
    let budget = Budget::new(1_000_000, 250);
    assert_eq!(budget.max_entities(), 4_000);

    let movement = Movement { pos, vel };
    let mut cursor = Cursor::start();

    let mut crank_count = 0u32;
    let mut max_in_one_crank = 0u32;
    while !cursor.done {
        let receipt = crank(&mut world, &movement, &mut cursor, budget, 0).unwrap();
        // No crank exceeds the CU ceiling.
        assert!(
            receipt.cu_used <= budget.max_cu,
            "crank used {} CU > ceiling {}",
            receipt.cu_used,
            budget.max_cu
        );
        max_in_one_crank = max_in_one_crank.max(receipt.processed);
        crank_count += 1;
    }

    assert_eq!(crank_count, 3, "10k / 4k => 3 cranks");
    assert!(max_in_one_crank <= 4_000);
    assert_eq!(cursor.processed, N, "every entity processed exactly once");

    // Every entity advanced by exactly +1.
    for e in 1..=N {
        let x = world.read(pos, e).unwrap().unwrap().get_i64("x").unwrap();
        assert_eq!(x, 1, "entity {e} should have advanced once");
    }
}

#[test]
fn dirty_set_touches_only_changed_entities() {
    use loom_engine_core::tick::crank_dirty;

    let mut world = World::new(1, [0u8; 32]);
    let pos = world
        .register_component("Position", vec![Field::new("x", FieldType::I64)])
        .unwrap();
    let vel = world
        .register_component("Velocity", vec![Field::new("dx", FieldType::I64)])
        .unwrap();

    for _ in 0..100 {
        let e = world.spawn_entity();
        world.set(pos, e, world.record(pos).unwrap().into_bytes()).unwrap();
        let mut v = world.record(vel).unwrap();
        v.set_i64("dx", 1).unwrap();
        world.set(vel, e, v.into_bytes()).unwrap();
    }

    // Start a fresh epoch: nothing is dirty.
    world.clear_dirty();
    assert_eq!(world.dirty_len(), 0);

    // Touch just three Positions.
    for e in [5u64, 17, 42] {
        let mut p = world.record(pos).unwrap();
        p.set_i64("x", 0).unwrap();
        world.set(pos, e, p.into_bytes()).unwrap();
    }

    let movement = Movement { pos, vel };
    let processed = crank_dirty(&mut world, &movement, 0).unwrap();
    assert_eq!(processed, 3, "only the 3 changed entities are swept");

    // The three moved; the rest stayed put.
    for e in 1..=100u64 {
        let x = world.read(pos, e).unwrap().unwrap().get_i64("x").unwrap();
        if [5, 17, 42].contains(&e) {
            assert_eq!(x, 1);
        } else {
            assert_eq!(x, 0);
        }
    }
}
