//! Engine fees and the protocol treasury.
//!
//! - worlds pay engine fees: per crank, per entity processed, and storage rent
//!   per Component byte;
//! - crankers are paid out of the compute fee to keep world time advancing;
//! - a grants treasury, funded from protocol revenue, bootstraps world builders;
//! - the token settles the compute bridge.
//!
//! This module is orthogonal to the engine core: the ECS, ticks, and bridge run
//! whether or not anyone meters them. A coordinator charges the [`Economy`] after
//! doing work. Accounting is exact integers, no floats, so the Rust core and the
//! SDK agree to the last unit.

use crate::error::EngineError;
use crate::ids::WorldId;
use std::collections::BTreeMap;

const BPS_DENOM: u64 = 10_000;

/// What the engine charges. Shares are in basis points (1/10_000) so the split
/// is exact and deterministic.
#[derive(Clone, Copy, Debug)]
pub struct FeeSchedule {
    /// Flat fee per crank submitted (covers the transaction's overhead).
    pub per_crank: u64,
    /// Fee per entity processed in a crank (the compute charge).
    pub per_entity: u64,
    /// Recurring rent per Component byte, per storage epoch.
    pub storage_per_byte: u64,
    /// Share of each crank's fee paid to the cranker, in basis points.
    pub cranker_share_bps: u16,
    /// Share of the *protocol's* cut routed to the grants pool, in basis points.
    pub grant_share_bps: u16,
}

impl FeeSchedule {
    /// Default fee schedule.
    pub fn standard() -> Self {
        Self {
            per_crank: 5_000,
            per_entity: 10,
            storage_per_byte: 1,
            cranker_share_bps: 4_000, // 40% of compute fees to crankers
            grant_share_bps: 2_500,   // 25% of protocol revenue to grants
        }
    }
}

/// Where protocol revenue accrues.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Treasury {
    /// Net protocol revenue (after cranker rewards and grants allocation).
    pub protocol: u64,
    /// The grants pool that bootstraps world builders.
    pub grants: u64,
}

/// One world's prepaid engine-fee account.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldLedger {
    pub balance: u64,
    pub spent: u64,
}

/// The itemized result of charging for one crank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrankBill {
    pub total: u64,
    pub cranker_reward: u64,
    pub to_protocol: u64,
    pub to_grants: u64,
}

/// The engine's economic ledger.
#[derive(Clone, Debug)]
pub struct Economy {
    pub fees: FeeSchedule,
    pub treasury: Treasury,
    ledgers: BTreeMap<WorldId, WorldLedger>,
}

impl Economy {
    pub fn new(fees: FeeSchedule) -> Self {
        Self {
            fees,
            treasury: Treasury::default(),
            ledgers: BTreeMap::new(),
        }
    }

    /// A world deposits token to prepay engine fees.
    pub fn fund_world(&mut self, world: WorldId, amount: u64) {
        let ledger = self.ledgers.entry(world).or_default();
        ledger.balance = ledger.balance.saturating_add(amount);
    }

    pub fn balance(&self, world: WorldId) -> u64 {
        self.ledgers.get(&world).map(|l| l.balance).unwrap_or(0)
    }

    pub fn ledger(&self, world: WorldId) -> WorldLedger {
        self.ledgers.get(&world).copied().unwrap_or_default()
    }

    fn debit(&mut self, world: WorldId, amount: u64) -> Result<(), EngineError> {
        let ledger = self.ledgers.entry(world).or_default();
        if ledger.balance < amount {
            return Err(EngineError::InsufficientBalance {
                world,
                needed: amount,
                have: ledger.balance,
            });
        }
        ledger.balance -= amount;
        ledger.spent += amount;
        Ok(())
    }

    /// Split the protocol's portion of a fee between net protocol revenue and the
    /// grants pool, crediting both.
    fn accrue_protocol(&mut self, amount: u64) -> (u64, u64) {
        let to_grants = mul_bps(amount, self.fees.grant_share_bps);
        let to_protocol = amount - to_grants;
        self.treasury.protocol += to_protocol;
        self.treasury.grants += to_grants;
        (to_protocol, to_grants)
    }

    /// Charge a world for one crank that processed `processed` entities. The
    /// cranker's share is returned (to be paid out); the remainder accrues to the
    /// treasury and grants. Fails if the world's balance cannot cover the fee.
    pub fn charge_crank(
        &mut self,
        world: WorldId,
        processed: u64,
    ) -> Result<CrankBill, EngineError> {
        let total = self
            .fees
            .per_crank
            .saturating_add(self.fees.per_entity.saturating_mul(processed));
        self.debit(world, total)?;
        let cranker_reward = mul_bps(total, self.fees.cranker_share_bps);
        let (to_protocol, to_grants) = self.accrue_protocol(total - cranker_reward);
        Ok(CrankBill {
            total,
            cranker_reward,
            to_protocol,
            to_grants,
        })
    }

    /// Charge a world storage rent for `total_bytes` of Component data for one
    /// epoch. All of it accrues to the treasury (split with grants).
    pub fn charge_storage(
        &mut self,
        world: WorldId,
        total_bytes: u64,
    ) -> Result<u64, EngineError> {
        let fee = self.fees.storage_per_byte.saturating_mul(total_bytes);
        self.debit(world, fee)?;
        self.accrue_protocol(fee);
        Ok(fee)
    }

    /// Charge a world a bridge settlement fee, proportional to the posted bond,
    /// when it consumes off-chain compute.
    pub fn charge_bridge_settlement(
        &mut self,
        world: WorldId,
        bond: u64,
        fee_bps: u16,
    ) -> Result<u64, EngineError> {
        let fee = mul_bps(bond, fee_bps);
        self.debit(world, fee)?;
        self.accrue_protocol(fee);
        Ok(fee)
    }

    /// Disburse a grant from the grants pool to bootstrap a world builder.
    pub fn disburse_grant(&mut self, amount: u64) -> Result<(), EngineError> {
        if self.treasury.grants < amount {
            return Err(EngineError::InsufficientGrants {
                needed: amount,
                have: self.treasury.grants,
            });
        }
        self.treasury.grants -= amount;
        Ok(())
    }
}

/// `amount * bps / 10_000`, computed in u128 to avoid intermediate overflow.
fn mul_bps(amount: u64, bps: u16) -> u64 {
    ((amount as u128 * bps as u128) / BPS_DENOM as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crank_fee_splits_exactly() {
        let mut econ = Economy::new(FeeSchedule::standard());
        econ.fund_world(1, 1_000_000);
        // 5000 flat + 10*100 = 6000 total. cranker 40% = 2400. protocol portion
        // 3600, of which grants 25% = 900, protocol 2700.
        let bill = econ.charge_crank(1, 100).unwrap();
        assert_eq!(bill.total, 6_000);
        assert_eq!(bill.cranker_reward, 2_400);
        assert_eq!(bill.to_grants, 900);
        assert_eq!(bill.to_protocol, 2_700);
        // Conservation: nothing created or destroyed.
        assert_eq!(bill.cranker_reward + bill.to_protocol + bill.to_grants, bill.total);
        assert_eq!(econ.balance(1), 1_000_000 - 6_000);
        assert_eq!(econ.treasury.protocol, 2_700);
        assert_eq!(econ.treasury.grants, 900);
    }

    #[test]
    fn insufficient_balance_is_rejected() {
        let mut econ = Economy::new(FeeSchedule::standard());
        econ.fund_world(1, 100);
        assert!(matches!(
            econ.charge_crank(1, 100),
            Err(EngineError::InsufficientBalance { world: 1, .. })
        ));
        // A failed charge does not move any funds.
        assert_eq!(econ.balance(1), 100);
        assert_eq!(econ.treasury, Treasury::default());
    }

    #[test]
    fn grants_disburse_and_cannot_overdraw() {
        let mut econ = Economy::new(FeeSchedule::standard());
        econ.fund_world(1, 1_000_000);
        for _ in 0..10 {
            econ.charge_crank(1, 100).unwrap();
        }
        let pool = econ.treasury.grants;
        assert_eq!(pool, 9_000); // 900 per crank * 10
        econ.disburse_grant(5_000).unwrap();
        assert_eq!(econ.treasury.grants, 4_000);
        assert!(matches!(
            econ.disburse_grant(10_000),
            Err(EngineError::InsufficientGrants { .. })
        ));
    }

    #[test]
    fn storage_rent_accrues() {
        let mut econ = Economy::new(FeeSchedule::standard());
        econ.fund_world(7, 10_000);
        let fee = econ.charge_storage(7, 512).unwrap();
        assert_eq!(fee, 512);
        assert_eq!(econ.balance(7), 10_000 - 512);
        assert_eq!(econ.treasury.protocol + econ.treasury.grants, 512);
    }
}
