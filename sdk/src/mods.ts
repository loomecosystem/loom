// Mirror of `engine-core/src/mods.rs`. Mods are Systems admitted to a world
// under its policy; cross-world references are validated by layout hash.

import type { World } from "./world.ts";
import type { System, Access } from "./system.ts";
import { EngineError } from "./errors.ts";

/** What a world grants to external Systems. */
export class ModPolicy {
  writable: number[] = [];
  readable: number[] = [];
  openRead = false;

  static new(): ModPolicy {
    return new ModPolicy();
  }

  allowWrite(c: number): this {
    this.writable.push(c);
    return this;
  }
  allowRead(c: number): this {
    this.readable.push(c);
    return this;
  }
  withOpenRead(): this {
    this.openRead = true;
    return this;
  }

  permits(access: Access): void {
    for (const w of access.writes) {
      if (!this.writable.includes(w)) {
        throw new EngineError("ModPermissionDenied", `mod may not write component ${w}`, {
          component: w,
          write: true,
        });
      }
    }
    if (!this.openRead) {
      for (const r of access.reads) {
        if (!this.readable.includes(r) && !this.writable.includes(r)) {
          throw new EngineError("ModPermissionDenied", `mod may not read component ${r}`, {
            component: r,
            write: false,
          });
        }
      }
    }
  }
}

/** Admit an external System under a world policy; returns its id. */
export function admitMod(policy: ModPolicy, system: System): number {
  policy.permits(system.access());
  return system.id();
}

/** A reference from one world to a Component in another, validated by layout. */
export class CrossWorldRef {
  readonly world: bigint;
  readonly component: number;
  readonly expectedLayoutHash: bigint;

  constructor(world: bigint, component: number, expectedLayoutHash: bigint) {
    this.world = world;
    this.component = component;
    this.expectedLayoutHash = expectedLayoutHash;
  }

  resolve(foreign: World, entity: bigint): Uint8Array {
    if (foreign.id !== this.world) {
      throw new EngineError("CrossWorldMismatch", "wrong world", {
        world: this.world,
        component: this.component,
      });
    }
    const schema = foreign.schema(this.component);
    if (schema.layoutHash() !== this.expectedLayoutHash) {
      throw new EngineError("CrossWorldMismatch", "layout drifted", {
        world: this.world,
        component: this.component,
      });
    }
    const bytes = foreign.get(this.component, entity);
    if (!bytes) throw new EngineError("UnknownEntity", `unknown entity ${entity}`, { entity });
    return bytes;
  }
}
