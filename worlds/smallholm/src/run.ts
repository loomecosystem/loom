// Smallholm - runnable end-to-end demo. Run with:
//   pnpm --filter @loom/world-smallholm start
//
// Farmers walk to their plots and turn grain into gold, two rival soldiers march
// out and fight, and a third-party tithe mod attaches to the running world. The
// whole run reduces to one reproducible state hash.

import { toHex64 } from "@loom/sdk";
import { FARMER, GRID, createSmallholm, step, type Smallholm } from "./world.ts";
import { TitheMod, spawnSettlement } from "./agents.ts";

const G = Number(GRID);

function renderMap(s: Smallholm): string {
  const cells: string[][] = Array.from({ length: G }, () =>
    Array.from({ length: G }, () => "·"),
  );
  for (const e of s.loom.query(s.agent)) {
    const a = s.loom.get(s.agent, e)!;
    const p = s.loom.get(s.position, e)!;
    const x = Number(p.x);
    const y = Number(p.y);
    if (x >= 0 && x < G && y >= 0 && y < G) {
      cells[y][x] = !a.alive ? "✝" : a.kind === FARMER ? "f" : "S";
    }
  }
  return cells.map((row) => "  " + row.join(" ")).join("\n");
}

function statsLine(s: Smallholm): string {
  return s.loom
    .query(s.agent)
    .map((e) => {
      const a = s.loom.get(s.agent, e)!;
      const st = s.loom.get(s.stats, e)!;
      const inv = s.loom.get(s.inventory, e)!;
      const tag = a.kind === FARMER ? "farmer" : "soldier";
      return `#${e} ${tag} hp=${st.hp} grain=${inv.grain} gold=${inv.gold}`;
    })
    .join("\n  ");
}

function banner(title: string): void {
  console.log("\n" + "─".repeat(64));
  console.log("  " + title);
  console.log("─".repeat(64));
}

function main(): void {
  banner("SMALLHOLM - an autonomous world on the Loom engine");
  const s = createSmallholm(1n);
  const { farmers, soldiers } = spawnSettlement(s);
  console.log(`  Spawned ${farmers.length} farmers + ${soldiers.length} soldiers as entities.`);
  console.log("  Legend:  f farmer   S soldier   ✝ fallen   · empty");
  console.log(renderMap(s));

  const TICKS = 24;
  for (let t = 1; t <= TICKS; t++) {
    step(s, BigInt(t));

    if (t === 13) {
      banner("tick 13 - a third-party tithe mod is admitted");
      s.loom.tick(new TitheMod(s.inventory, s.agent), BigInt(t));
      console.log("  The tithe skims gold from every inventory.");
    }

    if (t % 6 === 0) {
      banner(`tick ${t}`);
      console.log(renderMap(s));
      console.log("\n  " + statsLine(s));
    }
  }

  banner("FINAL STATE");
  console.log(renderMap(s));

  banner("indexer - reconstructed view of a farmer");
  const view = s.loom.indexer().entityView(farmers[0]);
  console.log("  " + JSON.stringify(view, (_k, v) => (typeof v === "bigint" ? v.toString() : v)));

  banner("determinism");
  console.log(`  world state hash = ${toHex64(s.loom.stateHash())}`);
}

main();
