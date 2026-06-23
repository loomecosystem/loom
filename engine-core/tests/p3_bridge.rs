//! An off-chain pathfinding result is settled on-chain through the optimistic
//! compute bridge and then consumed by a System: the unit walks the
//! obstacle-avoiding route the worker computed.
//!
//! Also covers the fraud path: a wrong result, challenged inside the window, is
//! disputed and the poster slashed.

use loom_engine_core::bridge::{ClaimStatus, ComputeBridge};
use loom_engine_core::hash::fnv1a;
use loom_engine_core::ids::{ComponentId, EntityId, SystemId};
use loom_engine_core::schema::{Field, FieldType};
use loom_engine_core::system::{Access, System, SystemCtx};
use loom_engine_core::tick::{run_to_completion, Budget};
use loom_engine_core::{EngineError, World};
use std::collections::VecDeque;

const GRID: usize = 8;
const MAX_WAYPOINTS: usize = 64;
const PATHFIND_TASK: u64 = 1;

// --- off-chain compute ----------

/// 8x8 grid; `true` = blocked. A wall down column x=3 with a single gap at the
/// bottom row forces any left-to-right route to detour down and back up.
fn obstacle_grid() -> [[bool; GRID]; GRID] {
    let mut g = [[false; GRID]; GRID];
    for y in 0..(GRID - 1) {
        g[y][3] = true; // block column x=3 for rows y=0..6 ...
    }
    // ... leaving the gap at g[7][3] (bottom row) open.
    g
}

/// Breadth-first shortest path from `start` to `goal`, avoiding blocked cells.
fn bfs(grid: &[[bool; GRID]; GRID], start: (u8, u8), goal: (u8, u8)) -> Vec<(u8, u8)> {
    let mut prev = vec![None::<(u8, u8)>; GRID * GRID];
    let mut seen = [[false; GRID]; GRID];
    let idx = |c: (u8, u8)| c.1 as usize * GRID + c.0 as usize;
    let mut q = VecDeque::new();
    q.push_back(start);
    seen[start.1 as usize][start.0 as usize] = true;

    while let Some(c) = q.pop_front() {
        if c == goal {
            break;
        }
        let (x, y) = (c.0 as i32, c.1 as i32);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= GRID as i32 || ny >= GRID as i32 {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if grid[ny][nx] || seen[ny][nx] {
                continue;
            }
            seen[ny][nx] = true;
            prev[idx((nx as u8, ny as u8))] = Some(c);
            q.push_back((nx as u8, ny as u8));
        }
    }

    // Reconstruct.
    let mut path = vec![goal];
    let mut cur = goal;
    while cur != start {
        cur = prev[idx(cur)].expect("goal must be reachable");
        path.push(cur);
    }
    path.reverse();
    path
}

fn pack_path(path: &[(u8, u8)]) -> Vec<u8> {
    let mut out = vec![0u8; MAX_WAYPOINTS * 2];
    for (i, &(x, y)) in path.iter().enumerate() {
        out[2 * i] = x;
        out[2 * i + 1] = y;
    }
    out
}

/// Binds a result to its request: worker and challenger hash the same inputs, so
/// a result can only settle against the request it answers.
fn request_hash(grid: &[[bool; GRID]; GRID], start: (u8, u8), goal: (u8, u8)) -> u64 {
    let mut bytes = Vec::new();
    for row in grid {
        for &cell in row {
            bytes.push(cell as u8);
        }
    }
    bytes.extend_from_slice(&[start.0, start.1, goal.0, goal.1]);
    fnv1a(&bytes)
}

// --- on-chain: a System that consumes the settled route ----------

/// Walks an entity one waypoint per tick along its settled Route.
struct FollowPath {
    pos: ComponentId,
    route: ComponentId,
}

impl System for FollowPath {
    fn id(&self) -> SystemId {
        7
    }
    fn access(&self) -> Access {
        Access::new().reads([self.route]).writes([self.pos, self.route])
    }
    fn query(&self) -> ComponentId {
        self.route
    }
    fn run(&self, ctx: &mut SystemCtx<'_>, e: EntityId) -> Result<(), EngineError> {
        let len = ctx.read_u8(self.route, e, "len")? as u64;
        let cursor = ctx.read_u8(self.route, e, "cursor")? as u64;
        if cursor >= len {
            return Ok(()); // arrived
        }
        let data = ctx.read_bytes(self.route, e, "data")?;
        let (x, y) = (data[2 * cursor as usize], data[2 * cursor as usize + 1]);
        ctx.mutate(self.pos, e, |p| {
            p.set_i64("x", x as i64)?;
            p.set_i64("y", y as i64)?;
            Ok(())
        })?;
        ctx.mutate(self.route, e, |r| {
            r.set_u8("cursor", (cursor + 1) as u8)?;
            Ok(())
        })
    }
}

/// Build a world with a single unit at `start`.
fn world_with_unit(start: (u8, u8)) -> (World, ComponentId, ComponentId, EntityId) {
    let mut world = World::new(1, [0u8; 32]);
    let pos = world
        .register_component(
            "Position",
            vec![
                Field::new("x", FieldType::I64),
                Field::new("y", FieldType::I64),
            ],
        )
        .unwrap();
    let route = world
        .register_component(
            "Route",
            vec![
                Field::new("len", FieldType::U8),
                Field::new("cursor", FieldType::U8),
                Field::new("input_hash", FieldType::U64),
                Field::new("data", FieldType::Bytes((MAX_WAYPOINTS * 2) as u16)),
            ],
        )
        .unwrap();
    let unit = world.spawn_entity();
    let mut p = world.record(pos).unwrap();
    p.set_i64("x", start.0 as i64)
        .unwrap()
        .set_i64("y", start.1 as i64)
        .unwrap();
    world.set(pos, unit, p.into_bytes()).unwrap();
    (world, pos, route, unit)
}

/// Fold a finalized bridge result into world state: write the settled route onto
/// the unit so a System can consume it.
fn settle_route(
    world: &mut World,
    bridge: &ComputeBridge,
    claim_id: u64,
    route: ComponentId,
    unit: EntityId,
    expected_input_hash: u64,
    len: usize,
) -> Result<(), EngineError> {
    let packed = bridge.consume(claim_id, expected_input_hash)?.to_vec();
    let mut r = world.record(route)?;
    r.set_u8("len", len as u8)?
        .set_u8("cursor", 0)?
        .set_u64("input_hash", expected_input_hash)?
        .set_bytes("data", &packed)?;
    world.set(route, unit, r.into_bytes())
}

#[test]
fn offchain_path_settled_on_chain_and_consumed_by_system() {
    let grid = obstacle_grid();
    let start = (0u8, 0u8);
    let goal = (7u8, 0u8); // straight line is blocked by the wall

    // Off-chain: compute the path.
    let path = bfs(&grid, start, goal);
    assert_eq!(path.first(), Some(&start));
    assert_eq!(path.last(), Some(&goal));
    for &(x, y) in &path {
        assert!(!grid[y as usize][x as usize], "path must avoid obstacles");
    }
    // The detour through the gap is longer than the 8-cell straight line.
    assert!(path.len() > GRID, "must detour around the wall");

    let input_hash = request_hash(&grid, start, goal);
    let packed = pack_path(&path);

    // Settle optimistically through the bridge.
    let mut bridge = ComputeBridge::new(20); // 20-slot fraud window
    let worker = [1u8; 32];
    let claim = bridge.post_result(PATHFIND_TASK, input_hash, packed, worker, 1_000, 0);

    // Before the window closes, the result is not consumable.
    assert_eq!(
        settle_for(&bridge, claim, input_hash).unwrap_err(),
        EngineError::ClaimNotFinalized
    );
    assert_eq!(
        bridge.finalize(claim, 5).unwrap_err(),
        EngineError::ClaimWindowOpen
    );

    // Window elapses unchallenged, finalize.
    bridge.finalize(claim, 25).unwrap();
    assert_eq!(bridge.get(claim).unwrap().status, ClaimStatus::Finalized);

    // On-chain settle: fold the route into world state.
    let (mut world, pos, route, unit) = world_with_unit(start);
    settle_route(&mut world, &bridge, claim, route, unit, input_hash, path.len()).unwrap();

    // Walk one waypoint per tick until arrival.
    let follow = FollowPath { pos, route };
    for _ in 0..path.len() {
        run_to_completion(&mut world, &follow, Budget::new(1_000_000, 1_000), 0).unwrap();
    }

    // The unit ends at the goal.
    let p = world.read(pos, unit).unwrap().unwrap();
    assert_eq!(
        (p.get_i64("x").unwrap() as u8, p.get_i64("y").unwrap() as u8),
        goal
    );
}

/// The bridge-consume half of settlement, for the pre-finalize assertion above.
fn settle_for(
    bridge: &ComputeBridge,
    claim: u64,
    input_hash: u64,
) -> Result<Vec<u8>, EngineError> {
    bridge.consume(claim, input_hash).map(|b| b.to_vec())
}

#[test]
fn fraudulent_result_is_challenged_and_slashed() {
    let grid = obstacle_grid();
    let start = (0u8, 0u8);
    let goal = (7u8, 0u8);
    let input_hash = request_hash(&grid, start, goal);

    let correct = pack_path(&bfs(&grid, start, goal));

    // Post a bogus result: the straight, wall-crossing line.
    let bogus = pack_path(&[(0, 0), (7, 0)]);
    let mut bridge = ComputeBridge::new(20);
    let liar = [9u8; 32];
    let claim = bridge.post_result(PATHFIND_TASK, input_hash, bogus, liar, 5_000, 0);

    // Recompute the real path within the window and dispute it.
    bridge.challenge(claim, &correct, 10).unwrap();
    assert_eq!(bridge.get(claim).unwrap().status, ClaimStatus::Disputed);
    assert_eq!(bridge.total_slashed, 5_000, "bogus poster's bond is slashed");

    // A disputed claim can neither be finalized nor consumed.
    assert_eq!(
        bridge.finalize(claim, 25).unwrap_err(),
        EngineError::ClaimAlreadySettled
    );
    assert_eq!(
        bridge.consume(claim, input_hash).unwrap_err(),
        EngineError::ClaimNotFinalized
    );
}

#[test]
fn honest_result_cannot_be_slashed() {
    let grid = obstacle_grid();
    let start = (0u8, 0u8);
    let goal = (7u8, 0u8);
    let input_hash = request_hash(&grid, start, goal);
    let correct = pack_path(&bfs(&grid, start, goal));

    let mut bridge = ComputeBridge::new(20);
    let claim = bridge.post_result(PATHFIND_TASK, input_hash, correct.clone(), [2u8; 32], 5_000, 0);

    // Challenging an honest result with the same recomputation fails, no slash.
    assert_eq!(
        bridge.challenge(claim, &correct, 10).unwrap_err(),
        EngineError::FraudProofInvalid
    );
    assert_eq!(bridge.total_slashed, 0);
    bridge.finalize(claim, 25).unwrap();
    assert_eq!(bridge.get(claim).unwrap().status, ClaimStatus::Finalized);
}

#[test]
fn consume_rejects_a_mismatched_request() {
    // A finalized result is bound to the request it answers. Consuming it against a
    // different input hash is its own error, not a fraud-proof failure - the result
    // is honest, it just does not answer this question.
    let grid = obstacle_grid();
    let start = (0u8, 0u8);
    let goal = (7u8, 0u8);
    let input_hash = request_hash(&grid, start, goal);
    let packed = pack_path(&bfs(&grid, start, goal));

    let mut bridge = ComputeBridge::new(20);
    let claim = bridge.post_result(PATHFIND_TASK, input_hash, packed, [1u8; 32], 1_000, 0);
    bridge.finalize(claim, 25).unwrap();

    // Against its own request: consumable.
    assert!(bridge.consume(claim, input_hash).is_ok());
    // Against a different request: a distinct mismatch error.
    assert_eq!(
        bridge.consume(claim, input_hash ^ 1).unwrap_err(),
        EngineError::ClaimInputMismatch { expected: input_hash ^ 1, got: input_hash }
    );
}

#[test]
fn a_verified_result_finalizes_without_a_window() {
    // The ZK lane: a result posted with a proof the verifier accepts finalizes
    // immediately - no bond, no fraud-proof window - and is consumable at once. A
    // proof the verifier rejects is refused outright, storing nothing.
    let grid = obstacle_grid();
    let start = (0u8, 0u8);
    let goal = (7u8, 0u8);
    let input_hash = request_hash(&grid, start, goal);
    let packed = pack_path(&bfs(&grid, start, goal));
    let result_hash = fnv1a(&packed);

    // Stand-in for an on-chain SNARK verifier: accept a fixed proof token, and only
    // when it binds the posted result's hash. A real verifier checks a validity proof.
    let verify = move |_inp: u64, res: u64, proof: &[u8]| proof == b"valid" && res == result_hash;

    let mut bridge = ComputeBridge::new(20);

    // A rejected proof is refused; nothing is stored.
    assert_eq!(
        bridge
            .post_verified(PATHFIND_TASK, input_hash, packed.clone(), [1u8; 32], b"forged", verify)
            .unwrap_err(),
        EngineError::FraudProofInvalid
    );

    // A valid proof finalizes on the spot: no finalize() call, no window to wait out.
    let claim = bridge
        .post_verified(PATHFIND_TASK, input_hash, packed.clone(), [1u8; 32], b"valid", verify)
        .unwrap();
    assert_eq!(bridge.get(claim).unwrap().status, ClaimStatus::Finalized);
    assert_eq!(bridge.consume(claim, input_hash).unwrap(), packed.as_slice());
}
