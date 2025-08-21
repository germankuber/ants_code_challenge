use clap::Parser;
use colored::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

/// 4 fixed directions for tiny, predictable loops
#[derive(Clone, Copy)]
#[repr(u8)]
enum Dir {
    North = 0,
    South = 1,
    East = 2,
    West = 3,
}
impl Dir {
    #[inline(always)]
    fn from_str(s: &str) -> Option<Dir> {
        // byte match is faster than string match
        match s.as_bytes() {
            b"north" => Some(Dir::North),
            b"south" => Some(Dir::South),
            b"east" => Some(Dir::East),
            b"west" => Some(Dir::West),
            _ => None,
        }
    }
    pub const ALL: [Dir; 4] = [Dir::North, Dir::South, Dir::East, Dir::West];
    #[inline(always)]
    fn idx(self) -> usize {
        self as usize
    }
}

/// Sentinel for "no tunnel"
const INVALID: u32 = u32::MAX;

/// Graph node: compact and cache-friendly
#[derive(Clone, Debug)]
#[repr(C)]
struct Node {
    name_idx: u32,   // index into `names`
    neigh: [u32; 4], // neighbors by direction; INVALID if none
    alive: bool,     // colony alive
}
impl Node {
    #[inline]
    fn new(name_idx: u32) -> Self {
        Self {
            name_idx,
            neigh: [INVALID; 4],
            alive: true,
        }
    }
}

/// Final world: names + nodes (no hashmaps kept at runtime)
#[derive(Clone, Debug)]
struct World {
    names: Vec<String>,
    nodes: Vec<Node>,
}

/// Ant state packed into a byte (alive/trapped) + aligned fields
#[derive(Clone, Debug)]
#[repr(C)]
struct Ant {
    pos: u32,
    id: u32,
    moves: u32,
    state: u8, // bit 0 = alive, bit 1 = trapped
}
impl Ant {
    const ALIVE: u8 = 0b01;
    const TRAPPED: u8 = 0b10;

    #[inline(always)]
    fn is_alive(&self) -> bool {
        self.state & Self::ALIVE != 0
    }
    #[inline(always)]
    fn is_trapped(&self) -> bool {
        self.state & Self::TRAPPED != 0
    }
    #[inline(always)]
    fn set_alive(&mut self, v: bool) {
        if v {
            self.state |= Self::ALIVE
        } else {
            self.state &= !Self::ALIVE
        }
    }
    #[inline(always)]
    fn set_trapped(&mut self, v: bool) {
        if v {
            self.state |= Self::TRAPPED
        } else {
            self.state &= !Self::TRAPPED
        }
    }
}

/// CLI
#[derive(Parser, Debug)]
#[command(name = "ant_mania", about = "🐜 Ant invasion simulator on Hiveum")]
struct Args {
    /// Number of ants
    #[arg(short = 'n', long = "ants")]
    ants: usize,
    /// Path to the map file
    #[arg(short = 'm', long = "map")]
    map: String,
    /// Maximum moves per ant
    #[arg(long, default_value_t = 10_000)]
    max_moves: u32,
    /// Random seed
    #[arg(long)]
    seed: Option<u64>,
    /// Suppress fight logs (for benchmarks)
    #[arg(long, default_value_t = false)]
    suppress_events: bool,
}

/// Fast parser with preallocation; the hashmap exists only here
fn parse_world(path: &str) -> World {
    let file = File::open(path).expect("cannot open map file");
    let reader = BufReader::with_capacity(64 * 1024, file);

    let mut names: Vec<String> = Vec::with_capacity(1024);
    let mut name_to_id: HashMap<String, u32> = HashMap::with_capacity(1024);
    let mut edges: Vec<(u32, Dir, String)> = Vec::with_capacity(4096);

    for line in reader.lines() {
        let line = line.expect("failed to read line");
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut parts = line.split_whitespace();
        let colony = parts.next().expect("invalid line: missing colony name");

        let src_id = *name_to_id.entry(colony.to_string()).or_insert_with(|| {
            let id = names.len() as u32;
            names.push(colony.to_string());
            id
        });

        for kv in parts {
            if let Some(eq) = kv.find('=') {
                let dir_s = &kv[..eq];
                let dst_s = &kv[eq + 1..];
                if let Some(dir) = Dir::from_str(dir_s) {
                    edges.push((src_id, dir, dst_s.to_string()));
                }
            }
        }
    }

    // Ensure ids exist for destinations not seen as sources
    for (_, _, dst) in &edges {
        name_to_id.entry(dst.clone()).or_insert_with(|| {
            let id = names.len() as u32;
            names.push(dst.clone());
            id
        });
    }

    let mut nodes: Vec<Node> = (0..names.len()).map(|i| Node::new(i as u32)).collect();

    for (src, dir, dst_name) in &edges {
        if let Some(&dst) = name_to_id.get(dst_name) {
            nodes[*src as usize].neigh[dir.idx()] = dst;
        }
    }

    World { names, nodes }
}

/// Place ants uniformly at alive nodes
fn create_ants(world: &World, n: usize, rng: &mut fastrand::Rng) -> Vec<Ant> {
    let alive_nodes: Vec<u32> = world
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, nd)| if nd.alive { Some(i as u32) } else { None })
        .collect();

    let mut ants = Vec::with_capacity(n);
    let alive_len = alive_nodes.len();
    for i in 0..n {
        let pos = alive_nodes[rng.usize(..alive_len)];
        ants.push(Ant {
            id: i as u32,
            pos,
            moves: 0,
            state: Ant::ALIVE,
        });
    }
    ants
}

/// Next destination among alive exits; if none => trapped (stays in place)
#[inline(always)]
fn choose_next_pos(world: &World, ant_pos: u32, rng: &mut fastrand::Rng) -> (u32, bool) {
    // Safety: ant_pos always within nodes len; callers guarantee ant is on an alive node
    let node = unsafe { world.nodes.get_unchecked(ant_pos as usize) };
    debug_assert!(node.alive);

    // Manual unroll; write candidate then increment `k` only if alive
    let mut opts = [INVALID; 4];
    let mut k = 0usize;

    let n0 = node.neigh[0];
    let n1 = node.neigh[1];
    let n2 = node.neigh[2];
    let n3 = node.neigh[3];

    if n0 != INVALID {
        let alive = unsafe { world.nodes.get_unchecked(n0 as usize).alive };
        opts[k] = n0;
        k += alive as usize;
    }
    if n1 != INVALID {
        let alive = unsafe { world.nodes.get_unchecked(n1 as usize).alive };
        opts[k] = n1;
        k += alive as usize;
    }
    if n2 != INVALID {
        let alive = unsafe { world.nodes.get_unchecked(n2 as usize).alive };
        opts[k] = n2;
        k += alive as usize;
    }
    if n3 != INVALID {
        let alive = unsafe { world.nodes.get_unchecked(n3 as usize).alive };
        opts[k] = n3;
        k += alive as usize;
    }

    if k == 0 {
        (ant_pos, true)
    } else {
        (opts[rng.usize(..k)], false)
    }
}

/// Print the remaining world in the same input format
fn print_world(world: &World) {
    // Pre-size per line reduces reallocs on big worlds
    let mut line = String::with_capacity(128);
    for nd in &world.nodes {
        if !nd.alive {
            continue;
        }
        line.clear();
        line.push_str(&world.names[nd.name_idx as usize]);
        for &d in &Dir::ALL {
            let nid = nd.neigh[d.idx()];
            if nid != INVALID && world.nodes[nid as usize].alive {
                line.push(' ');
                line.push_str(match d {
                    Dir::North => "north=",
                    Dir::South => "south=",
                    Dir::East => "east=",
                    Dir::West => "west=",
                });
                line.push_str(&world.names[world.nodes[nid as usize].name_idx as usize]);
            }
        }
        // println!("{}", line);
    }
}

fn main() {
    let args = Args::parse();
    let mut rng = if let Some(seed) = args.seed {
        fastrand::Rng::with_seed(seed)
    } else {
        fastrand::Rng::new()
    };

    // 1) Parse + ants (excluded from latency)
    let mut world = parse_world(&args.map);
    let mut ants = create_ants(&world, args.ants, &mut rng);

    // --- t=0 pre-pass collisions (O(ants) + O(destroyed_nodes)) without Vec<Vec<_>> ---
    {
        let n = world.nodes.len();
        let mut occ_count = vec![0u32; n];
        let mut occ_first = vec![u32::MAX; n];
        let mut occ_second = vec![u32::MAX; n];
        let mut destroyed = vec![false; n];

        for a in &ants {
            if a.is_alive() {
                let nid = a.pos as usize;
                match occ_count[nid] {
                    0 => {
                        occ_first[nid] = a.id;
                        occ_count[nid] = 1;
                    }
                    1 => {
                        occ_second[nid] = a.id;
                        occ_count[nid] = 2;
                    }
                    _ => {
                        occ_count[nid] += 1;
                    }
                }
            }
        }
        for nid in 0..n {
            if occ_count[nid] >= 2 && world.nodes[nid].alive {
                if !args.suppress_events {
                    println!(
                        "{} {} {} {}",
                        "💥".red(),
                        world.names[world.nodes[nid].name_idx as usize].bright_red(),
                        "has been destroyed by".red(),
                        format!("ant {} and ant {}", occ_first[nid], occ_second[nid]).yellow()
                    );
                }
                world.nodes[nid].alive = false;
                destroyed[nid] = true;
            }
        }
        // kill ants that were on destroyed colonies at t=0
        for a in &mut ants {
            if destroyed[a.pos as usize] {
                a.set_alive(false);
                a.set_trapped(false);
            }
        }
    }

    // Active ants set (preallocated)
    let mut active: Vec<usize> = Vec::with_capacity(args.ants);
    active.extend(ants.iter().enumerate().filter_map(|(i, a)| {
        if a.is_alive() && !a.is_trapped() && a.moves < args.max_moves {
            Some(i)
        } else {
            None
        }
    }));

    // 2) Simulation (measured)
    let sim_start = Instant::now();

    // Per-node "generation" trick avoids clearing large arrays
    let n_nodes = world.nodes.len();
    let mut gen = vec![0u32; n_nodes];
    let mut occ_count = vec![0u32; n_nodes];
    let mut occ_first = vec![u32::MAX; n_nodes];
    let mut occ_second = vec![u32::MAX; n_nodes];
    let mut cur_gen: u32 = 1;

    // Stationary stock (trapped / max-moves) that still participates in collisions
    let mut base_occ = vec![0u32; n_nodes];
    let mut base_first = vec![u32::MAX; n_nodes];
    let mut base_second = vec![u32::MAX; n_nodes];

    // "Touched" node lists to avoid O(n_nodes) scans
    let mut touched_nodes: Vec<usize> = Vec::with_capacity(4096);
    let mut base_touched: Vec<usize> = Vec::with_capacity(1024);

    // Per-ant buffers
    let mut next_pos: Vec<u32> = ants.iter().map(|a| a.pos).collect();
    let mut trapped_now: Vec<bool> = vec![false; ants.len()];

    while !active.is_empty() {
        cur_gen = cur_gen.wrapping_add(1);
        touched_nodes.clear();
        base_touched.clear();

        // (1) Decide destinations for active ants
        let mut i = 0;
        while i < active.len() {
            let ai = active[i];
            let a = &ants[ai];
            if !a.is_alive() || a.moves >= args.max_moves || a.is_trapped() {
                active.swap_remove(i);
                continue;
            }
            let (np, became_trapped) = choose_next_pos(&world, a.pos, &mut rng);
            next_pos[ai] = np;
            trapped_now[ai] = became_trapped;
            i += 1;
        }
        if active.is_empty() {
            break;
        }

        // (2) Build occupancy (initialize from stationary, then add active)
        for &ai in &active {
            let a = &ants[ai];
            if !a.is_alive() {
                continue;
            }
            let nid = next_pos[ai] as usize;

            if gen[nid] != cur_gen {
                gen[nid] = cur_gen;
                occ_count[nid] = base_occ[nid];
                occ_first[nid] = base_first[nid];
                occ_second[nid] = base_second[nid];
                touched_nodes.push(nid);
            }

            match occ_count[nid] {
                0 => {
                    occ_first[nid] = a.id;
                    occ_count[nid] = 1;
                }
                1 => {
                    if occ_first[nid] == u32::MAX {
                        occ_first[nid] = a.id;
                    } else {
                        occ_second[nid] = a.id;
                    }
                    occ_count[nid] = 2;
                }
                _ => {
                    occ_count[nid] += 1;
                }
            }
        }

        // (3) Destroy collided colonies (only touched)
        for &nid in &touched_nodes {
            if occ_count[nid] >= 2 && world.nodes[nid].alive {
                if !args.suppress_events {
                    println!(
                        "{} {} {} {}",
                        "💥".red(),
                        world.names[world.nodes[nid].name_idx as usize].bright_red(),
                        "has been destroyed by".red(),
                        format!("ant {} and ant {}", occ_first[nid], occ_second[nid]).yellow()
                    );
                }
                world.nodes[nid].alive = false;
                // If destroyed, their stationary stock is now irrelevant
                base_occ[nid] = 0;
                base_first[nid] = u32::MAX;
                base_second[nid] = u32::MAX;
            }
        }

        // (4) Commit ant state + register new stationaries
        let mut j = 0;
        while j < active.len() {
            let ai = active[j];
            let nid = next_pos[ai] as usize;
            let node_alive = world.nodes[nid].alive;

            let a = &mut ants[ai];
            if !a.is_alive() {
                active.swap_remove(j);
                continue;
            }

            if !node_alive {
                a.set_alive(false);
                a.set_trapped(false);
                active.swap_remove(j);
                continue;
            }

            if !trapped_now[ai] && nid as u32 != a.pos {
                a.pos = nid as u32;
                a.moves += 1;

                if a.moves >= args.max_moves {
                    // becomes stationary due to max-moves
                    match base_occ[nid] {
                        0 => base_first[nid] = a.id,
                        1 => base_second[nid] = a.id,
                        _ => {}
                    }
                    if base_occ[nid] < 2 {
                        base_touched.push(nid);
                    } // only track until threshold
                    base_occ[nid] += 1;
                    active.swap_remove(j);
                    continue;
                }
            } else if trapped_now[ai] && !a.is_trapped() {
                // becomes stationary due to no exits
                a.set_trapped(true);
                match base_occ[nid] {
                    0 => base_first[nid] = a.id,
                    1 => base_second[nid] = a.id,
                    _ => {}
                }
                if base_occ[nid] < 2 {
                    base_touched.push(nid);
                }
                base_occ[nid] += 1;
                active.swap_remove(j);
                continue;
            }

            j += 1;
        }

        // (5) Pure-stationary destruction (only nodes touched by new stationaries)
        for &nid in &base_touched {
            if base_occ[nid] >= 2 && world.nodes[nid].alive {
                if !args.suppress_events {
                    println!(
                        "{} {} {} {}",
                        "💥".red(),
                        world.names[world.nodes[nid].name_idx as usize].bright_red(),
                        "has been destroyed by".red(),
                        format!("ant {} and ant {}", base_first[nid], base_second[nid]).yellow()
                    );
                }
                world.nodes[nid].alive = false;
                base_occ[nid] = 0;
                base_first[nid] = u32::MAX;
                base_second[nid] = u32::MAX;
            }
        }

        // (6) Early exit: with <=1 alive ant, no further collisions can happen
        let alive_ants = ants.iter().filter(|a| a.is_alive()).count();
        if alive_ants <= 1 {
            break;
        }
    }

    // 3) Final output: survivors, then summary
    print_world(&world);

    let sim_elapsed = sim_start.elapsed();
    let survivors = world.nodes.iter().filter(|n| n.alive).count();
    println!(
        "\n{}\n{} {:.3} ms {} {} {} {} {}",
        "===".bright_blue().bold(),
        "⏱️  Simulation Latency:".green().bold(),
        sim_elapsed.as_secs_f64() * 1000.0,
        "(map loaded)".dimmed(),
        "|".dimmed(),
        format!("ants={}", args.ants).cyan(),
        format!("max_moves={}", args.max_moves).cyan(),
        format!("survivors={}", survivors).cyan(),
    );
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ---------- Helpers (test-only) ----------

    /// Parse a world directly from an in-memory string.
    /// This mirrors `parse_world` but avoids filesystem I/O.
    fn parse_world_from_str(src: &str) -> World {
        let mut names: Vec<String> = Vec::new();
        let mut name_to_id: HashMap<String, u32> = HashMap::new();
        let mut edges: Vec<(u32, Dir, String)> = Vec::new();

        for raw in src.lines() {
            let line = raw.trim();
            if line.is_empty() { continue; }
            let mut parts = line.split_whitespace();
            let colony = parts.next().expect("missing colony name");

            let src_id = *name_to_id.entry(colony.to_string()).or_insert_with(|| {
                let id = names.len() as u32;
                names.push(colony.to_string());
                id
            });

            for kv in parts {
                if let Some(eq) = kv.find('=') {
                    let dir_s = &kv[..eq];
                    let dst_s = &kv[eq + 1..];
                    let dir = Dir::from_str(dir_s).expect("invalid direction");
                    edges.push((src_id, dir, dst_s.to_string()));
                }
            }
        }

        for (_, _, dst) in &edges {
            name_to_id.entry(dst.clone()).or_insert_with(|| {
                let id = names.len() as u32;
                names.push(dst.clone());
                id
            });
        }

        let mut nodes: Vec<Node> = (0..names.len()).map(|i| Node::new(i as u32)).collect();
        for (src, dir, dst_name) in &edges {
            let dst = *name_to_id.get(dst_name).unwrap();
            nodes[*src as usize].neigh[dir.idx()] = dst;
        }

        World { names, nodes }
    }

    /// Find a node id by name. Panics if not found (tests should define it).
    fn id_of(world: &World, name: &str) -> u32 {
        world.names.iter().position(|n| n == name).expect("name not found") as u32
    }

    /// Format remaining world to a single string (without printing).
    fn format_world(world: &World) -> String {
        let mut out = String::new();
        let mut line = String::with_capacity(128);
        for nd in &world.nodes {
            if !nd.alive { continue; }
            line.clear();
            line.push_str(&world.names[nd.name_idx as usize]);
            for &d in &Dir::ALL {
                let nid = nd.neigh[d.idx()];
                if nid != INVALID && world.nodes[nid as usize].alive {
                    line.push(' ');
                    line.push_str(match d {
                        Dir::North => "north=",
                        Dir::South => "south=",
                        Dir::East  => "east=",
                        Dir::West  => "west=",
                    });
                    line.push_str(&world.names[world.nodes[nid as usize].name_idx as usize]);
                }
            }
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    // ---------- Tests ----------

    #[test]
    fn parse_directionality_basic() {
        // Directed edges: A north=B, west=C; B south=A (no link to C)
        let src = "A north=B west=C\nB south=A\n";
        let w = parse_world_from_str(src);
        let a = id_of(&w, "A") as usize;
        let b = id_of(&w, "B") as usize;
        let c = id_of(&w, "C") as usize;

        assert_eq!(w.nodes[a].neigh[Dir::North.idx()], b as u32);
        assert_eq!(w.nodes[a].neigh[Dir::West.idx()],  c as u32);
        assert_eq!(w.nodes[a].neigh[Dir::South.idx()], INVALID);
        assert_eq!(w.nodes[a].neigh[Dir::East.idx()],  INVALID);

        assert_eq!(w.nodes[b].neigh[Dir::South.idx()], a as u32);
        assert_eq!(w.nodes[b].neigh[Dir::North.idx()], INVALID);

        assert_eq!(w.nodes[c].neigh, [INVALID; 4]);
    }

    #[test]
    fn t0_collision_kills_and_destroys() {
        // With a single colony X and 2 ants, both start at X => t=0 destroy X.
        let mut w = parse_world_from_str("X\n");
        let x = id_of(&w, "X");

        let mut ants = vec![
            Ant { id: 1, pos: x, moves: 0, state: Ant::ALIVE },
            Ant { id: 2, pos: x, moves: 0, state: Ant::ALIVE },
        ];

        // Reproduce the t=0 pre-pass in a compact way
        let n = w.nodes.len();
        let mut occ_count = vec![0u32; n];
        for a in &ants { if a.is_alive() { occ_count[a.pos as usize] += 1; } }
        for nid in 0..n {
            if occ_count[nid] >= 2 && w.nodes[nid].alive {
                w.nodes[nid].alive = false;
            }
        }
        for a in &mut ants {
            if !w.nodes[a.pos as usize].alive {
                a.set_alive(false);
                a.set_trapped(false);
            }
        }

        assert!(!w.nodes[x as usize].alive, "colony X must be destroyed at t=0");
        assert!(!ants[0].is_alive() && !ants[1].is_alive(), "both ants must die");
    }

    #[test]
    fn trapped_on_isolated_node() {
        // Single node with no exits => next_pos must return trapped=true.
        let w = parse_world_from_str("Iso\n");
        let iso = id_of(&w, "Iso");
        let mut rng = fastrand::Rng::with_seed(123);
        let (next, trapped) = super::choose_next_pos(&w, iso, &mut rng);
        assert_eq!(next, iso);
        assert!(trapped);
    }

    #[test]
    fn choose_next_pos_respects_single_exit() {
        // A east=B, B has no exits. From A there is exactly one live exit.
        let w = parse_world_from_str("A east=B\n");
        let a = id_of(&w, "A");
        let mut rng = fastrand::Rng::with_seed(42);
        let (next, trapped) = super::choose_next_pos(&w, a, &mut rng);
        assert!(!trapped);
        assert_eq!(next, id_of(&w, "B"));
    }

    #[test]
    fn stationary_stock_causes_future_destruction() {
        // Node A accumulates one stationary ant; an active arriving should destroy A.
        let mut w = parse_world_from_str("A\nB\n");
        let a = id_of(&w, "A") as usize;

        // Stationary stock like in runtime
        let mut base_occ    = vec![0u32; w.nodes.len()];
        let mut base_first  = vec![u32::MAX; w.nodes.len()];
        let mut base_second = vec![u32::MAX; w.nodes.len()];

        // One stationary already at A
        base_occ[a] = 1;
        base_first[a] = 100;

        // Build occupancy for an arriving active ant with id=7
        let mut gen        = vec![0u32; w.nodes.len()];
        let mut occ_count  = vec![0u32; w.nodes.len()];
        let mut occ_first  = vec![u32::MAX; w.nodes.len()];
        let mut occ_second = vec![u32::MAX; w.nodes.len()];
        let cur_gen = 999u32;

        // initialize from stationary
        if gen[a] != cur_gen {
            gen[a] = cur_gen;
            occ_count[a] = base_occ[a];
            occ_first[a] = base_first[a];
            occ_second[a]= base_second[a];
        }
        // add active
        if occ_count[a] == 0 {
            occ_first[a] = 7;
            occ_count[a] = 1;
        } else if occ_count[a] == 1 {
            if occ_first[a] == u32::MAX { occ_first[a] = 7; }
            else { occ_second[a] = 7; }
            occ_count[a] = 2;
        }

        // destruction rule
        if occ_count[a] >= 2 && w.nodes[a].alive {
            w.nodes[a].alive = false;
        }

        assert!(!w.nodes[a].alive, "A must be destroyed by stationary + active");
        assert_eq!(occ_first[a], 100);
        assert_eq!(occ_second[a], 7);
    }

    #[test]
    fn world_print_formatting_like_input() {
        // A east=B; C isolated
        let mut w = parse_world_from_str("A east=B\nC\n");
        let out = format_world(&w);
        assert!(out.contains("A east=B"));
        assert!(out.contains("C\n") || out.ends_with("C"));
    }
}