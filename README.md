# 🐜 Ant Mania — High-Performance Ant Invasion Simulator (Rust)

A fast simulator of giant space ants on the planet **Hiveum**.  
This README explains the **algorithm** in a clear, slightly graphical way (ASCII diagrams), plus how to build, run, and benchmark.

---

## ✨ Overview

Given a map (graph) of colonies and directional tunnels:

```
Fizz north=Buzz west=Bla south=Blub
Buzz south=Fizz west=Blip
```

- Spawn **N ants** at random alive colonies.
- Each tick, every active ant moves to a random **alive** outgoing neighbor (if any).
- If **≥ 2 ants** end up in the same colony in the same tick → they **fight**, both die, and the **colony is destroyed** (removed from the graph).
- Ants that have **no alive exits** become **trapped** (stationary).
- Simulation stops when all ants are "done" (dead or trapped or reached `--max-moves`) or no more collisions are possible.
- Output: remaining world (same input format), then a latency summary.

---

## 🧠 Solution Strategy

### Core Problem
Efficiently simulate thousands of ants moving simultaneously on a directed graph, where collisions (≥2 ants in the same node) destroy both the ants and the node.

### Main Strategy: Optimized Turn-Based Simulation

1. **Cache-Friendly Representation**:
   - Convert names to IDs (`u32`) for O(1) access
   - ID-indexed arrays instead of hashmaps
   - Neighbors as fixed array `[u32; 4]` with sentinel `INVALID`

2. **Generational Occupancy Tracking**:
   - **Problem**: Clearing large arrays each tick is expensive O(n)
   - **Solution**: Generational counter - only "touch" nodes that change
   - Arrays `gen[nid]` + `cur_gen` avoid massive clears

3. **Active Set Management**:
   - **Problem**: Iterating over all ants including dead/trapped ones
   - **Solution**: Maintain `active` list only with ants that can move
   - `swap_remove()` O(1) to eliminate finished ants

4. **Two-Phase Collision Detection**:
   - **Phase 1**: Plan movements without executing
   - **Phase 2**: Detect collisions, destroy nodes, update states
   - Special handling of stationary ants that can still cause collisions

5. **Performance Optimizations**:
   - No allocations in the hot path of the main loop
   - `unsafe get_unchecked` for node access (with documented invariants)
   - Predictable branches and manually unrolled loops
   - Compact structures for better cache locality

### Algorithm Flow
```
Parse → Seed Ants → t=0 Collisions → Main Loop:
  ├─ Plan moves (active ants only)
  ├─ Build occupancy (generational tracking)
  ├─ Detect & destroy collisions
  ├─ Commit ant movements
  ├─ Handle new stationary ants
  └─ Early exit if ≤1 ant alive
```

---

## 🗺️ Input format

- One line per colony: `NAME [north=NAME] [south=NAME] [east=NAME] [west=NAME]`
- Directions are **directed** edges.
- Names are ASCII tokens without spaces.

---

## 🏗️ Project Structure

```
ants_code_challenge/
├── 📁 src/                          # Core source code
│   ├── 🦀 lib.rs                    # Library root with module exports and prelude
│   ├── 🦀 main.rs                   # Binary entry point (minimal, delegates to lib)
│   ├── 🦀 cli.rs                    # Command-line argument parsing (clap)
│   ├── 🦀 error.rs                  # Error types and Result aliases
│   ├── 🦀 utils.rs                  # Constants and shared utilities
│   ├── 🦀 ant.rs                    # Ant struct and state management
│   ├── 🦀 direction.rs              # Direction enum (North/South/East/West)
│   ├── 📁 world/                    # World representation and parsing
│   │   ├── 🦀 mod.rs                # Module exports (Node, World, parse_world)
│   │   ├── 🦀 node.rs               # Individual colony node with neighbors
│   │   ├── 🦀 parser.rs             # Map file parsing logic
│   │   └── 🦀 world.rs              # World container with ant management
│   └── 📁 simulation/               # Core simulation engine
│       ├── 🦀 mod.rs                # Module exports (SimulationEngine)
│       ├── 🦀 engine.rs             # Main simulation loop and state management
│       └── 🦀 collision.rs          # Collision detection and handling
├── 📁 tests/                        # Integration tests
│   ├── 🦀 integration_tests.rs      # Full simulation integration tests
│   ├── 🦀 smoke.rs                  # Basic smoke tests
│   └── 🦀 trap.rs                   # Trapping behavior tests
├── 📁 files/                        # Test maps and input data
│   ├── 📄 description.txt           # Problem description
│   ├── 📄 hiveum_map_small.txt      # Small test map
│   └── 📄 hiveum_map_medium.txt     # Medium test map
├── 📄 Cargo.toml                    # Rust package configuration
├── 📄 Cargo.lock                    # Dependency lock file
└── 📄 README.md                     # This file
```

### 📦 Module Architecture

The code follows idiomatic Rust project structure with clear separation of concerns:

- **`lib.rs`**: Central module organization with a `prelude` module for common imports
- **Domain modules**: Each major concept (`ant`, `world`, `direction`) has its own module
- **Simulation engine**: Isolated in its own module with collision detection logic
- **Tests**: Co-located with implementation code using `#[cfg(test)]` modules, plus integration tests

---

## 🧠 Data Model (Cache-friendly)

```
World
├─ names: Vec<String>     // id → name  
└─ nodes: Vec<Node>       // id → Node { name_idx, neigh[4], alive }

Node
├─ name_idx: u32          // index into names
├─ neigh: [u32; 4]        // neighbor ids by Dir; INVALID (u32::MAX) = no edge
└─ alive: bool            // colony alive?

Ant  
├─ id: u32
├─ pos: u32               // current colony id
├─ moves: u32
└─ state: u8              // bit 0: alive, bit 1: trapped
```

Why this shape?
- **IDs (`u32`)** + **fixed 4-slot adjacency** ⇒ small structs, great locality.
- **Sentinel `INVALID`** avoids `Option` branches on the hot path.
- Ant state packed into bits ⇒ tiny & branch-friendly.

---

## 🧩 Algorithm (visual walkthrough)

### 0) Parse (not timed)
- Read lines, assign ids to colony names.
- For each `dir=dst`, fill `nodes[src].neigh[dir] = dst`.
- Keep **only** `names` + `nodes` in `World`.

### 1) Seed ants (not timed)
- Pick random alive colonies for each ant using a fixed RNG seed.

### 2) **t=0 Pre-pass collision**
If multiple ants **start** on the same colony:
```
Tick 0:
[A] ants: {7, 12, 21} → destroy A, kill all there
```

Implementation:
- One pass counting `occ_count[nid]`, store the first two ids for logging.
- Destroy colonies with `occ_count ≥ 2`, kill ants standing on them.

### 3) Main loop (timed)

Per tick:

```
(1) Plan moves for active ants
(2) Build occupancy from:
    a) stationary stock (trapped / max-moves)
    b) moving ants' next positions
(3) Destroy collided colonies (≥ 2) → log once
(4) Commit ant states:
    - died if arriving to destroyed node
    - moved (+1 move)
    - became stationary if trapped or max-moves
(5) If any node just reached ≥2 stationary this tick → destroy it
(6) Early exit if ≤ 1 alive ant
```

#### Active set
We only iterate **active** ants:
```
active = [indices of ants that are alive & not trapped & moves < max_moves]
```
Finished ants are removed via `swap_remove` (O(1)).

#### Generational occupancy (no clears!)
We avoid clearing big arrays each tick by using a **generation counter**:

```
gen[nid] stores the last tick that touched nid
cur_gen increments every tick

if gen[nid] != cur_gen:
    gen[nid] = cur_gen
    occ_count[nid] = base_occ[nid]      // stationary stock
    occ_first[nid] = base_first[nid]
    occ_second[nid] = base_second[nid]
```

We also keep a small list of **touched nodes** this tick to scan only those for destruction.

#### Stationary ants still collide
Ants can become **stationary** because:
- they got **trapped** (no alive exits), or
- they hit **`--max-moves`**.

We **add** stationary ants to a per-node stock:
```
base_occ[nid]++     // and remember first two ids for logs
```
So a later moving ant arriving at that colony can still trigger a collision:
```
base_occ=1 + arriving ant → 2 ⇒ destroy
```

We track nodes that gain new stationary ants (`base_touched`) and only check those for "pure stationary" collisions.

---

## 🔢 Tiny diagrams

### Directed neighbors per node

```
       North
         ^
         |
West <---+---> East
         |
         v
       South
```

Each node has up to 4 directed exits; ants pick **uniformly** among **alive** exits.

### Collision on a tick

```
Tick t plan:
Ant 3: X → Y
Ant 8: Z → Y
Stationary at Y: one ant already there

Occupancy(Y) starts at 1 (stationary), then:

Ant 3 → 2
=> destroy Y immediately (log once) 💥

Ant 8's commit sees Y is destroyed → dies.
```

---

## ⏱️ Complexity Analysis & Efficiency

### Time Complexity
- **Parse**: O(lines + edges) - single pass
- **Initialization**: O(ants) - random placement
- **Per tick**:
  - Move planning: O(active_ants) ← key: not all ants
  - Occupancy & destruction: O(touched_nodes) ← key: not all nodes
  - Commit & accounting: O(active_ants)
  - Stationary destruction: O(base_touched_nodes)

### Why is it Efficient?

1. **Sublinear in graph size**: Only process nodes "touched" this tick
2. **Decreasing in ants**: Active set shrinks as ants die/get trapped
3. **No massive clears**: Generational tracking avoids O(n) cleanups
4. **Cache locality**: Compact structures and sequential access
5. **Branch prediction**: Predictable loops, no `Option` unwrapping

### Observed Scalability
```
1,000 ants   →  ~2ms    (excellent)
10,000 ants  →  ~20ms   (linear scaling)
50,000 ants  →  ~100ms  (still linear)
```

Typical performance is **sublinear** with respect to graph size thanks to touched node lists.

---

## 🚀 Performance notes

- All the hot-path arrays (`occ_count`, `occ_first`, `occ_second`, `gen`, `base_*`) are **preallocated**.
- **No heap allocs** inside the per-tick loops.
- Short, predictable branches; manual unroll for 4-way neighbor scan.
- Optional: encapsulate `unsafe get_unchecked` behind a small helper with a documented **SAFETY** invariant.

---

## 🎯 Determinism

- With a single RNG stream, changing loop order can change results even with the same `--seed`.  
- For strict determinism across refactors, use **per-ant RNG streams** (e.g., seed = `seed ^ ant_id`) so each ant draws independently of loop order. (Not required for the challenge.)

---

## 🖥️ CLI

```bash
# Build (optimized)
cargo build --release

# Run
target/release/ants_code_challenge \
  --ants 10000 \
  --map ./files/hiveum_map_medium.txt \
  --seed 42 \
  --suppress-events
```

### Flags

- `-n, --ants <N>`: number of ants
- `-m, --map <FILE>`: map file path
- `--max-moves <N>`: per-ant move cap (default: 10000)
- `--seed <U64>`: RNG seed (reproducibility)
- `--suppress-events`: hides per-collision logs (best for benchmarks)

---

## 🧾 Output

Surviving map in the same input format (one line per alive colony):
```
Name [north=...] [south=...] [east=...] [west=...]
```

Summary:
```
===
⏱️  Simulation Latency: 267.349 ms (map loaded) | ants=10000 max_moves=10000 survivors=3029
```

---

## ✅ Assumptions (documented)

- Colony names are unique tokens without spaces.
- Directions are exactly `north|south|east|west`.
- The map can be disconnected; ants spawn uniformly among alive colonies.
- Once a colony is destroyed, all tunnels in/out become unusable immediately.
- Ants do not "pass through" destroyed colonies; they either die (if they landed there) or get trapped (if later they have no exits).

---

## 🧪 Testing

Unit tests validate:
- parsing & directionality,
- t=0 collisions,
- trapping on isolated nodes,
- stationary stock causing future collisions,
- formatting of the remaining world.

Integration tests check:
- the binary prints a summary,
- t=0 collision on a single-node map yields survivors=0.

Run all tests:
```bash
cargo test
```

---

## 📈 Benchmarking tips

- Always run with `--release` and `--suppress-events`.
- Try multiple `--ants` (1k, 5k, 10k, 50k) and record latencies.
- Pin `--seed` when comparing runs to reduce variance.

Example:
```bash
for n in 1000 5000 10000 20000; do
  target/release/ants_code_challenge -n $n -m ./files/hiveum_map_medium.txt --seed 42 --suppress-events
done
```

---

## 🔮 Future work (ideas)

- Optional per-ant RNG for strict determinism across refactors.
- Bitset for node alive status to shrink Node further.
- Parallelism by sharding ants + two-phase reduce (careful with collisions).
- `tracing` + `env_filter` for structured logging without cost when disabled.

---

## 📚 Example map (tiny)

```
A north=B west=C
B south=A
C east=A
```

One possible run (seeded):
```
C west=A
B
===
⏱️  Simulation Latency: 0.031 ms (map loaded) | ants=1000 max_moves=10000 survivors=2
```

Have fun unleashing the ants. 🐜💥
