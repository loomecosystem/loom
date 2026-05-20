// Compute bridge - mirror of `engine-core/src/bridge.rs`. Optimistic settlement
// of off-chain work behind a fraud-proof window, plus a ZK verification path.

import { fnv1a } from "./hash.ts";
import { EngineError } from "./errors.ts";

export type ClaimStatus =
  | { kind: "challengeable"; untilSlot: bigint }
  | { kind: "finalized" }
  | { kind: "disputed" };

export interface ComputeClaim {
  id: bigint;
  task: bigint;
  inputHash: bigint;
  result: Uint8Array;
  resultHash: bigint;
  poster: Uint8Array;
  bond: bigint;
  status: ClaimStatus;
}

export class ComputeBridge {
  private claims = new Map<bigint, ComputeClaim>();
  private nextId = 0n;
  readonly windowSlots: bigint;
  totalSlashed = 0n;

  constructor(windowSlots: bigint) {
    this.windowSlots = windowSlots;
  }

  /** Post an optimistic result, opening the fraud-proof window. */
  postResult(
    task: bigint,
    inputHash: bigint,
    result: Uint8Array,
    poster: Uint8Array,
    bond: bigint,
    nowSlot: bigint,
  ): bigint {
    const id = this.nextId++;
    this.claims.set(id, {
      id,
      task,
      inputHash,
      result,
      resultHash: fnv1a(result),
      poster,
      bond,
      status: { kind: "challengeable", untilSlot: nowSlot + this.windowSlots },
    });
    return id;
  }

  /** Post with a validity proof; finalizes immediately, no window. */
  postVerified(
    task: bigint,
    inputHash: bigint,
    result: Uint8Array,
    poster: Uint8Array,
    proof: Uint8Array,
    verify: (inputHash: bigint, resultHash: bigint, proof: Uint8Array) => boolean,
  ): bigint {
    const resultHash = fnv1a(result);
    if (!verify(inputHash, resultHash, proof)) {
      throw new EngineError("FraudProofInvalid", "ZK proof did not verify");
    }
    const id = this.nextId++;
    this.claims.set(id, {
      id,
      task,
      inputHash,
      result,
      resultHash,
      poster,
      bond: 0n,
      status: { kind: "finalized" },
    });
    return id;
  }

  get(id: bigint): ComputeClaim {
    const c = this.claims.get(id);
    if (!c) throw new EngineError("ClaimNotFinalized", `no claim ${id}`, { id });
    return c;
  }

  /** Challenge with the correct recomputation; slashes the poster if it differs. */
  challenge(id: bigint, recomputed: Uint8Array, nowSlot: bigint): void {
    const claim = this.get(id);
    if (claim.status.kind !== "challengeable") {
      throw new EngineError("ClaimAlreadySettled", "claim already settled");
    }
    if (nowSlot >= claim.status.untilSlot) {
      throw new EngineError("ClaimWindowOpen", "fraud-proof window already closed");
    }
    if (fnv1a(recomputed) === claim.resultHash) {
      throw new EngineError("FraudProofInvalid", "recomputation matches; no fraud");
    }
    claim.status = { kind: "disputed" };
    this.totalSlashed += claim.bond;
  }

  /** Finalize a claim whose window elapsed unchallenged. */
  finalize(id: bigint, nowSlot: bigint): void {
    const claim = this.get(id);
    if (claim.status.kind !== "challengeable") {
      throw new EngineError("ClaimAlreadySettled", "claim already settled");
    }
    if (nowSlot < claim.status.untilSlot) {
      throw new EngineError("ClaimWindowOpen", "fraud-proof window still open");
    }
    claim.status = { kind: "finalized" };
  }

  /** Read a finalized result, checking it answers the expected request. */
  consume(id: bigint, expectedInputHash: bigint): Uint8Array {
    const claim = this.claims.get(id);
    if (!claim || claim.status.kind !== "finalized") {
      throw new EngineError("ClaimNotFinalized", "compute claim is not finalized", { id });
    }
    if (claim.inputHash !== expectedInputHash) {
      throw new EngineError("FraudProofInvalid", "result answers a different request");
    }
    return claim.result;
  }
}
