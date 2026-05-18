// Shared test fixtures.

import { Access, type System, SystemCtx } from "../src/index.ts";

/** Move every positioned entity by its velocity. */
export class Movement implements System {
  pos: number;
  vel: number;

  constructor(pos: number, vel: number) {
    this.pos = pos;
    this.vel = vel;
  }

  id(): number {
    return 1;
  }
  access(): Access {
    return Access.new().withReads([this.vel]).withWrites([this.pos]);
  }
  query(): number {
    return this.pos;
  }
  run(ctx: SystemCtx, e: bigint): void {
    const dx = ctx.readI64(this.vel, e, "dx");
    const dy = ctx.readI64(this.vel, e, "dy");
    ctx.mutate(this.pos, e, (r) => {
      r.setI64("x", r.getI64("x") + dx);
      r.setI64("y", r.getI64("y") + dy);
    });
  }
}
