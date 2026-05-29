//! A world prepays engine fees, runs metered cranks, and protocol revenue, the
//! grants pool, and cranker rewards accrue exactly. Mirrored in
//! `sdk/test/economy.test.ts`.

use loom_engine_core::economy::{Economy, FeeSchedule};
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

const WORLD: u64 = 1;
const N: u64 = 250;

#[test]
fn world_pays_fees_crankers_and_grants_accrue() {
    let mut world = World::new(WORLD, [0u8; 32]);
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

    let mut econ = Economy::new(FeeSchedule::standard());
    econ.fund_world(WORLD, 1_000_000);

    // Bill the world for each crank. 250 entities at 100/crank => 3 cranks
    // (100, 100, 50).
    let movement = Movement { pos, vel };
    let budget = Budget::new(1_000, 10); // 100 entities/crank
    let mut cursor = Cursor::start();
    let mut total_cranker_reward = 0u64;
    let mut cranks = 0u32;
    while !cursor.done {
        let receipt = crank(&mut world, &movement, &mut cursor, budget, 0).unwrap();
        let bill = econ.charge_crank(WORLD, receipt.processed as u64).unwrap();
        total_cranker_reward += bill.cranker_reward;
        cranks += 1;
    }

    assert_eq!(cranks, 3);
    // Fees: 6000 + 6000 + 5500 = 17_500.
    assert_eq!(econ.ledger(WORLD).spent, 17_500);
    assert_eq!(total_cranker_reward, 7_000);
    assert_eq!(econ.treasury.protocol, 7_875);
    assert_eq!(econ.treasury.grants, 2_625);

    // Storage rent on the live Component bytes: 250 entities * (8 + 8) = 4000.
    assert_eq!(world.storage_bytes(), 4_000);
    let rent = econ.charge_storage(WORLD, world.storage_bytes()).unwrap();
    assert_eq!(rent, 4_000);
    assert_eq!(econ.treasury.protocol, 7_875 + 3_000);
    assert_eq!(econ.treasury.grants, 2_625 + 1_000);

    // Disburse from the grants pool.
    econ.disburse_grant(3_000).unwrap();
    assert_eq!(econ.treasury.grants, 3_625 - 3_000);

    // Final world balance: 1_000_000 - 17_500 - 4_000.
    assert_eq!(econ.balance(WORLD), 978_500);
}
