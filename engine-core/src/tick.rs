//! The tick coordinator: bounded-work cranks.
//!
//! A single Solana transaction can't iterate over thousands of entities without
//! blowing the compute-budget (CU) limit. So a System sweep is split across many
//! **cranks**. Each crank processes at most `budget.max_entities()` entities, then
//! stops and writes a resumable [`Cursor`] back to the world. Anyone can submit the
//! next crank; crankers are paid in the protocol token per entity advanced
//! ([`cranker_reward`]). The cursor is just an entity id and iteration follows the
//! store's key order, so cranks resume exactly where the last one stopped, with no
//! lost or double-processed entities.
//!
//! [`crank_dirty`] handles the common case where only a few entities changed last
//! epoch: sweep just those instead of the whole set.

use crate::error::EngineError;
use crate::ids::EntityId;
use crate::system::{System, SystemCtx};
use crate::world::World;

/// A compute-budget envelope for one crank.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Compute-unit ceiling for the transaction (Solana's is 1.4M; worlds pick a
    /// per-crank target below that).
    pub max_cu: u64,
    /// Modeled cost of processing one entity through the System.
    pub cu_per_entity: u64,
}

impl Budget {
    pub fn new(max_cu: u64, cu_per_entity: u64) -> Self {
        debug_assert!(cu_per_entity > 0, "cu_per_entity must be positive");
        Self {
            max_cu,
            cu_per_entity,
        }
    }

    /// How many entities fit in this budget, clamped to at least one so a sweep
    /// always makes progress even if one entity's work exceeds the CU ceiling.
    pub fn max_entities(&self) -> u32 {
        ((self.max_cu / self.cu_per_entity).max(1)) as u32
    }
}

/// How far a System sweep has progressed. Persisted on-chain between cranks; here
/// the caller threads it through successive [`crank`] calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// The next entity id to consider. Entities with id `< resume_from` in this
    /// sweep have already been processed.
    pub resume_from: EntityId,
    /// Entities processed so far across the whole sweep.
    pub processed: u64,
    /// Number of cranks this sweep has taken.
    pub cranks: u32,
    /// Set once the sweep has covered the entire query set.
    pub done: bool,
}

impl Cursor {
    /// A fresh cursor at the start of a sweep.
    pub fn start() -> Self {
        Self {
            resume_from: 0,
            processed: 0,
            cranks: 0,
            done: false,
        }
    }
}

/// What one crank accomplished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrankReceipt {
    /// Entities processed in *this* crank.
    pub processed: u32,
    /// Compute units this crank would have consumed on-chain.
    pub cu_used: u64,
    /// Whether the sweep is now complete.
    pub done: bool,
}

/// Run one bounded crank of `system` over the world, advancing `cursor`.
///
/// Processes up to `budget.max_entities()` entities that have the System's query
/// Component, starting at `cursor.resume_from`. Returns a [`CrankReceipt`]; when
/// `receipt.done` is true the sweep is complete.
pub fn crank<S: System + ?Sized>(
    world: &mut World,
    system: &S,
    cursor: &mut Cursor,
    budget: Budget,
    slot: u64,
) -> Result<CrankReceipt, EngineError> {
    if cursor.done {
        return Ok(CrankReceipt {
            processed: 0,
            cu_used: 0,
            done: true,
        });
    }

    let max_n = budget.max_entities() as usize;
    let query = system.query();
    let access = system.access();

    // Pull one extra id to learn whether more remain after this batch. The Vec is
    // owned, so the immutable borrow of `world` ends before we mutate it below.
    let mut ids: Vec<EntityId> = world
        .entities_with_from(query, cursor.resume_from)
        .take(max_n + 1)
        .collect();
    let has_more = ids.len() > max_n;
    ids.truncate(max_n);

    // One ctx for the whole batch: access and slot are constant across the crank,
    // so there is no need to rebuild (and re-clone the access set) per entity.
    {
        let mut ctx = SystemCtx::new(world, access, slot);
        for &e in &ids {
            system.run(&mut ctx, e)?;
        }
    }

    let processed = ids.len() as u32;
    cursor.processed += processed as u64;
    cursor.cranks += 1;
    if let Some(&last) = ids.last() {
        cursor.resume_from = last + 1;
    }
    if !has_more {
        cursor.done = true;
    }

    Ok(CrankReceipt {
        processed,
        cu_used: processed as u64 * budget.cu_per_entity,
        done: cursor.done,
    })
}

/// Crank repeatedly until the sweep completes, returning the final cursor. For
/// tests and off-chain callers that want the whole sweep in one go.
pub fn run_to_completion<S: System + ?Sized>(
    world: &mut World,
    system: &S,
    budget: Budget,
    slot: u64,
) -> Result<Cursor, EngineError> {
    let mut cursor = Cursor::start();
    while !cursor.done {
        crank(world, system, &mut cursor, budget, slot)?;
    }
    Ok(cursor)
}

/// Crank only over dirty entities (changed since the last [`World::clear_dirty`])
/// that carry the System's query Component.
///
/// Returns the number of entities processed. Unlike [`crank`] this is unbounded,
/// so use it only when the dirty set is known to be small.
pub fn crank_dirty<S: System + ?Sized>(
    world: &mut World,
    system: &S,
    slot: u64,
) -> Result<u32, EngineError> {
    let query = system.query();
    let access = system.access();

    let targets: Vec<EntityId> = world
        .dirty()
        .filter(|&(c, _)| c == query)
        .map(|(_, e)| e)
        .collect();

    {
        let mut ctx = SystemCtx::new(world, access, slot);
        for &e in &targets {
            system.run(&mut ctx, e)?;
        }
    }
    Ok(targets.len() as u32)
}

/// Token paid to a cranker for advancing `processed` entities.
pub fn cranker_reward(processed: u64, rate_per_entity: u64) -> u64 {
    processed.saturating_mul(rate_per_entity)
}
