// Smallholm - runnable end-to-end demo.
//
// Run: `pnpm --filter @loom/world-smallholm start`
//
// One on-chain world that exercises the engine:
//   • ECS + ticks       - farmers, soldiers and a scout as entities/components
//   • agent NPCs         - behavior is a deterministic on-chain policy
//   • compute bridge     - the scout's pathfinding runs off-chain, settles on-chain
//   • mods               - a third-party tithe attaches mid-run
//   • token              - the engine charges fees; protocol + grants accrue
//   • determinism        - the world reduces to one reproducible hash

import { ComputeBridge, Economy, standardFees, toHex64 } from "@loom/sdk";
import { FARMER, GRID, SCOUT, createSmallholm, isBlocked, step, type Smallholm } from "./world.ts";
import { TitheMod, dispatchScout, settleScoutRoute, spawnSettlement } from "./agents.ts";

const G = Number(GRID);

function renderMap(s: Smallholm): string {
  const cells: string[][] = Array.from({ length: G }, (_row, y) =>
    Array.from({ length: G }, (_col, x) => (isBlocked(BigInt(x), BigInt(y)) ? "#" : "·")),
  );
  for (const e of s.loom.query(s.agent)) {
    const a = s.loom.get(s.agent, e)!;
    const p = s.loom.get(s.position, e)!;
    const x = Number(p.x);
    const y = Number(p.y);
    if (x >= 0 && x < G && y >= 0 && y < G) {
      cells[y][x] = !a.alive ? "✝" : a.kind === FARMER ? "f" : a.kind === SCOUT ? "c" : "S";
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
      const tag = a.kind === FARMER ? "farmer" : a.kind === SCOUT ? "scout " : "soldier";
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
  const { farmers, soldiers, scout } = spawnSettlement(s);
  console.log(
    `  Spawned ${farmers.length} farmers + ${soldiers.length} soldiers + 1 scout as entities.`,
  );
  console.log("  Legend:  f farmer   S soldier   c scout   # wall   ✝ fallen   · empty\n");

  // The world prepays fees into the token ledger.
  const econ = new Economy(standardFees());
  econ.fundWorld(s.loom.world.id, 1_000_000n);

  // Off-chain: the scout posts a pathfinding job to the compute bridge.
  const bridge = new ComputeBridge(3n); // 3-slot fraud window
  const dispatch = dispatchScout(bridge, [1, 1], [11, 11], new Uint8Array(32).fill(3), 1_000n, 0n);
  console.log(
    `  Scout posted a ${dispatch.pathLength}-cell pathfinding job to the bridge ` +
      `(bond 1000, fraud window 3 slots).`,
  );
  console.log(renderMap(s));

  const TICKS = 30;
  for (let t = 1; t <= TICKS; t++) {
    step(s, BigInt(t));

    if (t === 4) {
      // Window closed: finalize the off-chain result and settle it on-chain.
      bridge.finalize(dispatch.claim, BigInt(t));
      const fee = econ.chargeBridgeSettlement(s.loom.world.id, 1_000n, 100); // 1% of bond
      settleScoutRoute(s, bridge, dispatch, scout);
      banner(`tick 4 - fraud window closed; scout route settled on-chain (bridge fee ${fee})`);
      console.log("  The scout begins walking the obstacle-avoiding path it never computed itself.");
    }

    if (t === 13) {
      banner("tick 13 - a third-party tithe mod is admitted");
      s.loom.tick(new TitheMod(s.inventory, s.agent), BigInt(t));
      console.log("  The lord's tithe skims gold from every inventory.");
    }

    if (t % 6 === 0) {
      banner(`tick ${t}`);
      console.log(renderMap(s));
      console.log("\n  " + statsLine(s));
    }
  }

  banner("FINAL STATE");
  console.log(renderMap(s));

  // Storage rent on the world's live Component bytes.
  const rent = econ.chargeStorage(s.loom.world.id, s.loom.world.storageBytes());

  banner("engine revenue");
  console.log(`  world fee balance left : ${econ.balance(s.loom.world.id)}`);
  console.log(`  storage rent charged   : ${rent}  (${s.loom.world.storageBytes()} bytes)`);
  console.log(`  protocol treasury      : ${econ.treasury.protocol}`);
  console.log(`  grants pool            : ${econ.treasury.grants}`);

  banner("indexer - reconstructed view of the scout");
  const view = s.loom.indexer().entityView(scout);
  console.log("  " + JSON.stringify(view, (_k, v) => (typeof v === "bigint" ? v.toString() : v)));

  banner("determinism");
  console.log(`  world state hash = ${toHex64(s.loom.stateHash())}`);
  console.log("  (re-running with the same seed reproduces this exactly)\n");
}

main();
