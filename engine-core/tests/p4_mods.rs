//! A third-party mod runs against an existing world without redeploying it, and a
//! cross-world Component reference resolves through the shared schema layout.

use loom_engine_core::component::Record;
use loom_engine_core::ids::{ComponentId, EntityId, SystemId};
use loom_engine_core::mods::{admit_mod, CrossWorldRef, ModPolicy};
use loom_engine_core::schema::{Field, FieldType};
use loom_engine_core::system::{Access, System, SystemCtx};
use loom_engine_core::tick::run_to_completion;
use loom_engine_core::tick::Budget;
use loom_engine_core::{EngineError, World};

/// A world that already exists. Mods attach to this live state without
/// re-registering its schemas.
fn existing_world() -> (World, ComponentId, ComponentId) {
    let mut world = World::new(1, [7u8; 32]);
    let position = world
        .register_component(
            "Position",
            vec![
                Field::new("x", FieldType::I64),
                Field::new("y", FieldType::I64),
            ],
        )
        .unwrap();
    let gold = world
        .register_component("Gold", vec![Field::new("amount", FieldType::U64)])
        .unwrap();
    for _ in 0..5 {
        let e = world.spawn_entity();
        world.set(position, e, world.record(position).unwrap().into_bytes()).unwrap();
        let mut g = world.record(gold).unwrap();
        g.set_u64("amount", 1_000).unwrap();
        world.set(gold, e, g.into_bytes()).unwrap();
    }
    (world, position, gold)
}

/// A mod authored by a third party. It only knows the Component ids it was given.
struct TaxMod {
    gold: ComponentId,
}
impl System for TaxMod {
    fn id(&self) -> SystemId {
        100
    }
    fn access(&self) -> Access {
        Access::new().writes([self.gold]) // write implies read-back
    }
    fn query(&self) -> ComponentId {
        self.gold
    }
    fn run(&self, ctx: &mut SystemCtx<'_>, e: EntityId) -> Result<(), EngineError> {
        ctx.mutate(self.gold, e, |r| {
            let amount = r.get_u64("amount")?;
            r.set_u64("amount", amount - amount / 10)?; // 10% tax
            Ok(())
        })
    }
}

/// A mod that reaches for a Component the world never granted.
struct TeleportMod {
    position: ComponentId,
}
impl System for TeleportMod {
    fn id(&self) -> SystemId {
        101
    }
    fn access(&self) -> Access {
        Access::new().writes([self.position])
    }
    fn query(&self) -> ComponentId {
        self.position
    }
    fn run(&self, _ctx: &mut SystemCtx<'_>, _e: EntityId) -> Result<(), EngineError> {
        Ok(())
    }
}

#[test]
fn third_party_mod_runs_against_existing_world() {
    let (mut world, position, gold) = existing_world();

    // Policy: mods may write Gold, nothing else.
    let policy = ModPolicy::new().allow_write(gold);

    // The world admits TaxMod under policy, no redeploy or schema change.
    let tax = TaxMod { gold };
    let admitted = admit_mod(&policy, &tax).expect("tax mod is within policy");
    assert_eq!(admitted, tax.id());

    // It runs over the existing entities.
    run_to_completion(&mut world, &tax, Budget::new(1_000_000, 1_000), 0).unwrap();
    for e in 1..=5u64 {
        let amount = world.read(gold, e).unwrap().unwrap().get_u64("amount").unwrap();
        assert_eq!(amount, 900, "10% of 1000 taxed");
    }

    // A mod that reaches outside the policy is refused admission.
    let teleport = TeleportMod { position };
    let err = admit_mod(&policy, &teleport).unwrap_err();
    assert_eq!(err, EngineError::ModPermissionDenied { component: position });
}

#[test]
fn unadmitted_mod_is_still_stopped_by_access_control() {
    // Even if a mod skips admission, the per-access guard refuses any write it did
    // not declare. Here a mod declares Gold but tries to write Position at runtime.
    struct SneakyMod {
        gold: ComponentId,
        position: ComponentId,
    }
    impl System for SneakyMod {
        fn id(&self) -> SystemId {
            102
        }
        fn access(&self) -> Access {
            Access::new().writes([self.gold])
        }
        fn query(&self) -> ComponentId {
            self.gold
        }
        fn run(&self, ctx: &mut SystemCtx<'_>, e: EntityId) -> Result<(), EngineError> {
            ctx.mutate(self.position, e, |r| {
                r.set_i64("x", 999)?;
                Ok(())
            })
        }
    }

    let (mut world, position, gold) = existing_world();
    let sneaky = SneakyMod { gold, position };
    let err = run_to_completion(&mut world, &sneaky, Budget::new(1_000_000, 1_000), 0).unwrap_err();
    assert_eq!(err, EngineError::AccessDenied { component: position, write: true });
}

#[test]
fn cross_world_reference_resolves_through_shared_layout() {
    // World A owns a registry Component.
    let mut world_a = World::new(1, [1u8; 32]);
    let registry = world_a
        .register_component("FactionRegistry", vec![Field::new("reputation", FieldType::U64)])
        .unwrap();
    let faction = world_a.spawn_entity();
    let mut r = world_a.record(registry).unwrap();
    r.set_u64("reputation", 12_345).unwrap();
    world_a.set(registry, faction, r.into_bytes()).unwrap();

    let layout = world_a.schema(registry).unwrap().layout_hash();

    // World B references A's Component by its layout hash.
    let good_ref = CrossWorldRef {
        world: 1,
        component: registry,
        expected_layout_hash: layout,
    };
    let bytes = good_ref.resolve(&world_a, faction).unwrap();
    let view = Record::from_bytes(world_a.schema(registry).unwrap(), bytes.to_vec()).unwrap();
    assert_eq!(view.get_u64("reputation").unwrap(), 12_345);

    // A reference whose expected layout has drifted is rejected.
    let stale_ref = CrossWorldRef {
        world: 1,
        component: registry,
        expected_layout_hash: layout ^ 0xdead_beef,
    };
    assert_eq!(
        stale_ref.resolve(&world_a, faction).unwrap_err(),
        EngineError::CrossWorldMismatch { world: 1, component: registry }
    );

    // So is one pointed at the wrong world.
    let wrong_world = CrossWorldRef {
        world: 99,
        component: registry,
        expected_layout_hash: layout,
    };
    assert_eq!(
        wrong_world.resolve(&world_a, faction).unwrap_err(),
        EngineError::CrossWorldMismatch { world: 99, component: registry }
    );
}
