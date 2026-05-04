//! Loom engine - the on-chain program (Solana/Anchor shell). The deterministic
//! logic lives in `engine-core`; this exposes it as Solana accounts and
//! instructions.

use anchor_lang::prelude::*;

declare_id!("11111111111111111111111111111111");

#[program]
pub mod loom_engine {
    use super::*;

    pub fn initialize_world(ctx: Context<InitializeWorld>, world_id: u64) -> Result<()> {
        let world = &mut ctx.accounts.world;
        world.authority = ctx.accounts.authority.key();
        world.world_id = world_id;
        world.next_entity = 1; // entity 0 is the null entity
        world.next_component = 0;
        world.frozen = false;
        Ok(())
    }
}

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
