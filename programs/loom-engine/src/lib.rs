//! Loom Engine - the on-chain program (Solana/Anchor shell).
//!
//! This is the *deployable* face of the engine. Its deterministic logic - the
//! ECS state model, the PDA addressing scheme, schema layouts, bounded-work
//! semantics - is specified and host-tested in the `engine-core` crate; this
//! program is the thin Anchor wrapper that exposes that model as Solana accounts
//! and instructions.
//!
//! The PDA seed scheme matches `engine-core/src/addressing.rs` and the SDK's
//! `componentAddress` exactly, so off-chain indexers can find every Component
//! account from `(world_id, entity_id, component_id)` with no external index:
//!
//! ```text
//! World     : ["loom", "world",  world_id]
//! Schema    : ["loom", "schema", world_id, component_id]
//! Component : ["loom", "cmp",    world_id, entity_id, component_id]
//! ```
//!
//! The program id below matches `target/deploy/loom_engine-keypair.json`
//! (regenerate with `anchor keys sync` for your own deployment).

use anchor_lang::prelude::*;

declare_id!("26jAdoBMqG5zzNDQSRHHt7WA5hqbeZ6Lm33Sd2o96GiC");

/// Hard cap on a Component record's data region (keeps account sizing constant).
pub const MAX_COMPONENT_BYTES: usize = 512;
/// Hard cap on a Component schema name.
pub const MAX_NAME: usize = 32;

#[program]
pub mod loom_engine {
    use super::*;

    /// Create a new world owned by the signer.
    pub fn initialize_world(ctx: Context<InitializeWorld>, world_id: u64) -> Result<()> {
        let world = &mut ctx.accounts.world;
        world.authority = ctx.accounts.authority.key();
        world.world_id = world_id;
        world.next_entity = 1; // entity 0 is the null entity
        world.next_component = 0;
        world.frozen = false;
        Ok(())
    }

    /// Lock the world's rules permanently. Game state still changes; the schemas don't.
    pub fn freeze_world(ctx: Context<MutateWorld>) -> Result<()> {
        ctx.accounts.world.frozen = true;
        Ok(())
    }

    /// Register a Component schema; allocates the next component id.
    pub fn register_component(
        ctx: Context<RegisterComponent>,
        name: String,
        size: u16,
    ) -> Result<()> {
        require!(!ctx.accounts.world.frozen, LoomError::WorldFrozen);
        require!(name.len() <= MAX_NAME, LoomError::NameTooLong);
        require!(
            (size as usize) <= MAX_COMPONENT_BYTES && size > 0,
            LoomError::BadComponentSize
        );

        let world = &mut ctx.accounts.world;
        let meta = &mut ctx.accounts.meta;
        meta.world_id = world.world_id;
        meta.component_id = world.next_component;
        meta.size = size;
        meta.name = name;
        world.next_component = world
            .next_component
            .checked_add(1)
            .ok_or(LoomError::Overflow)?;
        Ok(())
    }

    /// Allocate a fresh entity id within the world.
    pub fn spawn_entity(ctx: Context<MutateWorld>) -> Result<()> {
        let world = &mut ctx.accounts.world;
        let entity = world.next_entity;
        world.next_entity = entity.checked_add(1).ok_or(LoomError::Overflow)?;
        emit!(EntitySpawned {
            world_id: world.world_id,
            entity,
        });
        Ok(())
    }

    /// Write a Component record onto an entity. Creates the Component account at
    /// its deterministic PDA on first write. The data length must match the
    /// schema's registered fixed size.
    pub fn set_component(
        ctx: Context<SetComponent>,
        entity: u64,
        component_id: u32,
        data: Vec<u8>,
    ) -> Result<()> {
        let meta = &ctx.accounts.meta;
        require!(entity != 0 && entity < ctx.accounts.world.next_entity, LoomError::UnknownEntity);
        require!(component_id == meta.component_id, LoomError::UnknownComponent);
        require!(data.len() == meta.size as usize, LoomError::BadRecordSize);

        let record = &mut ctx.accounts.record;
        record.bytes = data;
        Ok(())
    }
}

// --- accounts ----------------------------------------------------------------------

#[account]
pub struct World {
    pub authority: Pubkey,
    pub world_id: u64,
    pub next_entity: u64,
    pub next_component: u32,
    pub frozen: bool,
}
impl World {
    pub const SPACE: usize = 8 + 32 + 8 + 8 + 4 + 1;
}

#[account]
pub struct ComponentMeta {
    pub world_id: u64,
    pub component_id: u32,
    pub size: u16,
    pub name: String,
}
impl ComponentMeta {
    pub const SPACE: usize = 8 + 8 + 4 + 2 + 4 + MAX_NAME;
}

#[account]
pub struct ComponentRecord {
    pub bytes: Vec<u8>,
}
impl ComponentRecord {
    pub const SPACE: usize = 8 + 4 + MAX_COMPONENT_BYTES;
}

// --- instruction contexts ----------------------------------------------------------

#[derive(Accounts)]
#[instruction(world_id: u64)]
pub struct InitializeWorld<'info> {
    #[account(
        init,
        payer = authority,
        space = World::SPACE,
        seeds = [b"loom".as_ref(), b"world".as_ref(), world_id.to_le_bytes().as_ref()],
        bump
    )]
    pub world: Account<'info, World>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MutateWorld<'info> {
    #[account(
        mut,
        seeds = [b"loom".as_ref(), b"world".as_ref(), world.world_id.to_le_bytes().as_ref()],
        bump,
        has_one = authority
    )]
    pub world: Account<'info, World>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(name: String, size: u16)]
pub struct RegisterComponent<'info> {
    #[account(
        mut,
        seeds = [b"loom".as_ref(), b"world".as_ref(), world.world_id.to_le_bytes().as_ref()],
        bump,
        has_one = authority
    )]
    pub world: Account<'info, World>,
    #[account(
        init,
        payer = authority,
        space = ComponentMeta::SPACE,
        seeds = [b"loom".as_ref(), b"schema".as_ref(), world.world_id.to_le_bytes().as_ref(), world.next_component.to_le_bytes().as_ref()],
        bump
    )]
    pub meta: Account<'info, ComponentMeta>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(entity: u64, component_id: u32, data: Vec<u8>)]
pub struct SetComponent<'info> {
    #[account(
        seeds = [b"loom".as_ref(), b"world".as_ref(), world.world_id.to_le_bytes().as_ref()],
        bump
    )]
    pub world: Account<'info, World>,
    #[account(
        seeds = [b"loom".as_ref(), b"schema".as_ref(), world.world_id.to_le_bytes().as_ref(), component_id.to_le_bytes().as_ref()],
        bump
    )]
    pub meta: Account<'info, ComponentMeta>,
    #[account(
        init_if_needed,
        payer = authority,
        space = ComponentRecord::SPACE,
        seeds = [b"loom".as_ref(), b"cmp".as_ref(), world.world_id.to_le_bytes().as_ref(), entity.to_le_bytes().as_ref(), component_id.to_le_bytes().as_ref()],
        bump
    )]
    pub record: Account<'info, ComponentRecord>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// --- events & errors ---------------------------------------------------------------

#[event]
pub struct EntitySpawned {
    pub world_id: u64,
    pub entity: u64,
}

#[error_code]
pub enum LoomError {
    #[msg("world rules are frozen")]
    WorldFrozen,
    #[msg("component schema name is too long")]
    NameTooLong,
    #[msg("component size is zero or exceeds the maximum")]
    BadComponentSize,
    #[msg("unknown entity")]
    UnknownEntity,
    #[msg("unknown component")]
    UnknownComponent,
    #[msg("record size does not match the schema")]
    BadRecordSize,
    #[msg("arithmetic overflow")]
    Overflow,
}
