//! Create an entity, a System mutates it, read it back.

use loom_engine_core::ids::{ComponentId, EntityId, SystemId};
use loom_engine_core::schema::{Field, FieldType};
use loom_engine_core::system::{Access, System, SystemCtx};
use loom_engine_core::tick::{run_to_completion, Budget};
use loom_engine_core::{EngineError, World};

/// Move every positioned entity by its velocity.
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

#[test]
fn create_entity_system_mutates_client_reads_back() {
    let mut world = World::new(1, [0u8; 32]);
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

    let e = world.spawn_entity();
    {
        let mut p = world.record(pos).unwrap();
        p.set_i64("x", 10).unwrap().set_i64("y", 20).unwrap();
        world.set(pos, e, p.into_bytes()).unwrap();

        let mut v = world.record(vel).unwrap();
        v.set_i64("dx", 3).unwrap().set_i64("dy", -4).unwrap();
        world.set(vel, e, v.into_bytes()).unwrap();
    }

    let movement = Movement { pos, vel };
    run_to_completion(&mut world, &movement, Budget::new(1_000_000, 1_000), 0).unwrap();

    let p = world.read(pos, e).unwrap().unwrap();
    assert_eq!(p.get_i64("x").unwrap(), 13);
    assert_eq!(p.get_i64("y").unwrap(), 16);
}

#[test]
fn access_control_is_enforced() {
    // A System that writes a Component it never declared must be refused.
    struct Rogue {
        pos: ComponentId,
        secret: ComponentId,
    }
    impl System for Rogue {
        fn id(&self) -> SystemId {
            2
        }
        fn access(&self) -> Access {
            // declares only `pos`, but tries to write `secret`
            Access::new().writes([self.pos])
        }
        fn query(&self) -> ComponentId {
            self.pos
        }
        fn run(&self, ctx: &mut SystemCtx<'_>, e: EntityId) -> Result<(), EngineError> {
            ctx.mutate(self.secret, e, |r| {
                r.set_u64("v", 1)?;
                Ok(())
            })
        }
    }

    let mut world = World::new(1, [0u8; 32]);
    let pos = world
        .register_component("Position", vec![Field::new("x", FieldType::I64)])
        .unwrap();
    let secret = world
        .register_component("Secret", vec![Field::new("v", FieldType::U64)])
        .unwrap();
    let e = world.spawn_entity();
    world.set(pos, e, world.record(pos).unwrap().into_bytes()).unwrap();

    let rogue = Rogue { pos, secret };
    let err = run_to_completion(&mut world, &rogue, Budget::new(1_000_000, 1_000), 0).unwrap_err();
    assert_eq!(
        err,
        EngineError::AccessDenied {
            component: secret,
            write: true
        }
    );
}
