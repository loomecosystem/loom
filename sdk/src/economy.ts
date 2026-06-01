// Mirror of `engine-core/src/economy.rs`. Integer accounting in BigInt;
// matches the Rust core unit-for-unit.

import { EngineError } from "./errors.ts";

const BPS_DENOM = 10_000n;

export interface FeeSchedule {
  perCrank: bigint;
  perEntity: bigint;
  storagePerByte: bigint;
  crankerShareBps: number;
  grantShareBps: number;
}

/** Default schedule; matches `FeeSchedule::standard` in Rust. */
export function standardFees(): FeeSchedule {
  return {
    perCrank: 5_000n,
    perEntity: 10n,
    storagePerByte: 1n,
    crankerShareBps: 4_000,
    grantShareBps: 2_500,
  };
}

export interface Treasury {
  protocol: bigint;
  grants: bigint;
}

export interface WorldLedger {
  balance: bigint;
  spent: bigint;
}

export interface CrankBill {
  total: bigint;
  crankerReward: bigint;
  toProtocol: bigint;
  toGrants: bigint;
}

function mulBps(amount: bigint, bps: number): bigint {
  return (amount * BigInt(bps)) / BPS_DENOM;
}

export class Economy {
  fees: FeeSchedule;
  treasury: Treasury = { protocol: 0n, grants: 0n };
  private ledgers = new Map<bigint, WorldLedger>();

  constructor(fees: FeeSchedule) {
    this.fees = fees;
  }

  fundWorld(world: bigint, amount: bigint): void {
    const l = this.ledgers.get(world) ?? { balance: 0n, spent: 0n };
    l.balance += amount;
    this.ledgers.set(world, l);
  }

  balance(world: bigint): bigint {
    return this.ledgers.get(world)?.balance ?? 0n;
  }

  ledger(world: bigint): WorldLedger {
    return this.ledgers.get(world) ?? { balance: 0n, spent: 0n };
  }

  private debit(world: bigint, amount: bigint): void {
    const l = this.ledgers.get(world) ?? { balance: 0n, spent: 0n };
    if (l.balance < amount) {
      throw new EngineError("InsufficientBalance", "world cannot cover engine fee", {
        world,
        needed: amount,
        have: l.balance,
      });
    }
    l.balance -= amount;
    l.spent += amount;
    this.ledgers.set(world, l);
  }

  private accrueProtocol(amount: bigint): { toProtocol: bigint; toGrants: bigint } {
    const toGrants = mulBps(amount, this.fees.grantShareBps);
    const toProtocol = amount - toGrants;
    this.treasury.protocol += toProtocol;
    this.treasury.grants += toGrants;
    return { toProtocol, toGrants };
  }

  chargeCrank(world: bigint, processed: bigint): CrankBill {
    const total = this.fees.perCrank + this.fees.perEntity * processed;
    this.debit(world, total);
    const crankerReward = mulBps(total, this.fees.crankerShareBps);
    const { toProtocol, toGrants } = this.accrueProtocol(total - crankerReward);
    return { total, crankerReward, toProtocol, toGrants };
  }

  chargeStorage(world: bigint, totalBytes: bigint): bigint {
    const fee = this.fees.storagePerByte * totalBytes;
    this.debit(world, fee);
    this.accrueProtocol(fee);
    return fee;
  }

  chargeBridgeSettlement(world: bigint, bond: bigint, feeBps: number): bigint {
    const fee = mulBps(bond, feeBps);
    this.debit(world, fee);
    this.accrueProtocol(fee);
    return fee;
  }

  disburseGrant(amount: bigint): void {
    if (this.treasury.grants < amount) {
      throw new EngineError("InsufficientGrants", "grants pool cannot cover disbursement", {
        needed: amount,
        have: this.treasury.grants,
      });
    }
    this.treasury.grants -= amount;
  }
}
