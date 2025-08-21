use clap::Parser;
use colored::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

/// 4 direcciones fijas para loops tiny y branch-predictable
#[derive(Clone, Copy)]
enum Dir { North = 0, South = 1, East = 2, West = 3 }
impl Dir {
    fn from_str(s: &str) -> Option<Dir> {
        match s {
            "north" => Some(Dir::North),
            "south" => Some(Dir::South),
            "east"  => Some(Dir::East),
            "west"  => Some(Dir::West),
            _ => None,
        }
    }
    const ALL: [Dir; 4] = [Dir::North, Dir::South, Dir::East, Dir::West];
    #[inline] fn idx(self) -> usize { self as usize }
}

/// Sentinel para "sin túnel"
const INVALID: u32 = u32::MAX;

/// Nodo del grafo (compacto y cache-friendly)
#[derive(Clone, Debug)]
struct Node {
    name_idx: u32,       // índice en names
    neigh: [u32; 4],     // vecinos por dirección; INVALID si no hay
    alive: bool,         // colonia viva
}
impl Node {
    fn new(name_idx: u32) -> Self {
        Self { name_idx, neigh: [INVALID; 4], alive: true }
    }
}

/// Mundo final sin hashmaps (solo nombres + nodos)
#[derive(Clone, Debug)]
struct World {
    names: Vec<String>,
    nodes: Vec<Node>,
}

/// Estado de hormiga
#[derive(Clone, Debug)]
struct Ant {
    id: u32,
    pos: u32,
    moves: u32,
    alive: bool,
    trapped: bool,
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

/// Parseo en dos fases; el hashmap existe solo durante el parse y se descarta
fn parse_world(path: &str) -> World {
    let file = File::open(path).expect("cannot open map file");
    let reader = BufReader::new(file);

    let mut names: Vec<String> = Vec::new();
    let mut name_to_id: HashMap<String, u32> = HashMap::new();
    let mut edges: Vec<(u32, Dir, String)> = Vec::new();

    for line in reader.lines() {
        let line = line.expect("failed to read line");
        let line = line.trim();
        if line.is_empty() { continue; }

        let mut parts = line.split_whitespace();
        let colony = parts.next().expect("invalid line: missing colony name");

        let src_id = *name_to_id.entry(colony.to_string()).or_insert_with(|| {
            let id = names.len() as u32;
            names.push(colony.to_string());
            id
        });

        for kv in parts {
            let mut it = kv.split('=');
            let dir_s = it.next().unwrap_or_default();
            let dst_s = it.next().unwrap_or_default();
            let dir = Dir::from_str(dir_s).expect("invalid direction");
            edges.push((src_id, dir, dst_s.to_string()));
        }
    }

    // Asegurar ids para destinos no vistos como origen
    for (_, _, dst) in &edges {
        if !name_to_id.contains_key(dst) {
            let id = names.len() as u32;
            name_to_id.insert(dst.clone(), id);
            names.push(dst.clone());
        }
    }

    let mut nodes: Vec<Node> = (0..names.len())
        .map(|i| Node::new(i as u32))
        .collect();

    for (src, dir, dst_name) in edges {
        let dst = *name_to_id.get(&dst_name).unwrap();
        nodes[src as usize].neigh[dir.idx()] = dst;
    }

    World { names, nodes }
}

/// Coloca N hormigas en nodos vivos al azar
fn create_ants(world: &World, n: usize, rng: &mut fastrand::Rng) -> Vec<Ant> {
    let alive_nodes: Vec<u32> = world.nodes.iter()
        .enumerate()
        .filter(|(_, nd)| nd.alive)
        .map(|(i, _)| i as u32)
        .collect();

    let mut ants = Vec::with_capacity(n);
    for i in 0..n {
        let pos = alive_nodes[rng.usize(..alive_nodes.len())];
        ants.push(Ant { id: i as u32, pos, moves: 0, alive: true, trapped: false });
    }
    ants
}

/// Próximo destino uniforme entre salidas vivas; si no hay → trapped (queda en pos)
#[inline]
fn choose_next_pos(world: &World, ant_pos: u32, rng: &mut fastrand::Rng) -> (u32, bool) {
    let node = &world.nodes[ant_pos as usize];
    debug_assert!(node.alive);

    let neigh = node.neigh;
    let mut opts: [u32; 4] = [INVALID; 4];
    let mut k = 0usize;

    if neigh[0] != INVALID && world.nodes[neigh[0] as usize].alive { opts[k] = neigh[0]; k += 1; }
    if neigh[1] != INVALID && world.nodes[neigh[1] as usize].alive { opts[k] = neigh[1]; k += 1; }
    if neigh[2] != INVALID && world.nodes[neigh[2] as usize].alive { opts[k] = neigh[2]; k += 1; }
    if neigh[3] != INVALID && world.nodes[neigh[3] as usize].alive { opts[k] = neigh[3]; k += 1; }

    if k == 0 { (ant_pos, true) } else { (opts[rng.usize(..k)], false) }
}

/// Imprime el mundo remanente en el mismo formato del input
fn print_world(world: &World) {
    for nd in &world.nodes {
        if !nd.alive { continue; }
        let mut line = String::new();
        line.push_str(&world.names[nd.name_idx as usize]);
        for d in Dir::ALL {
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
        println!("{}", line);
    }
}

fn main() {
    let args = Args::parse();
    let mut rng = if let Some(seed) = args.seed { fastrand::Rng::with_seed(seed) } else { fastrand::Rng::new() };

    // 1) Map + ants (fuera del tiempo de simulación)
    let mut world = parse_world(&args.map);
    let mut ants = create_ants(&world, args.ants, &mut rng);

    // PRE-PASS t=0: si arrancan varias en la misma colonia, la destruyen
    {
        let n_nodes = world.nodes.len();
        let mut occ: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];
        for (ai, a) in ants.iter().enumerate() {
            if a.alive { occ[a.pos as usize].push(ai); }
        }
        for nid in 0..n_nodes {
            if occ[nid].len() >= 2 && world.nodes[nid].alive {
                if !args.suppress_events {
                    let a0 = ants[occ[nid][0]].id;
                    let a1 = ants[occ[nid][1]].id;
                    println!(
                        "{} {} {} {}",
                        "💥".red(),
                        world.names[world.nodes[nid].name_idx as usize].bright_red(),
                        "has been destroyed by".red(),
                        format!("ant {} and ant {}", a0, a1).yellow()
                    );
                }
                world.nodes[nid].alive = false;
                // matar todas las hormigas presentes ahí
                for &ai in &occ[nid] {
                    let aa = &mut ants[ai];
                    aa.alive = false;
                    aa.trapped = false;
                }
            }
        }
    }

    // Conjunto de hormigas activas (las que siguen moviéndose)
    let mut active: Vec<usize> = ants.iter().enumerate()
        .filter(|(_, a)| a.alive && !a.trapped && a.moves < args.max_moves)
        .map(|(i, _)| i)
        .collect();

    // 2) Simulación (tiempo medido)
    let sim_start = Instant::now();

    // Estructuras por nodo con "generations" (evita limpiar)
    let n_nodes = world.nodes.len();
    let mut gen: Vec<u32>        = vec![0; n_nodes];
    let mut occ_count: Vec<u32>  = vec![0; n_nodes];
    let mut occ_first: Vec<u32>  = vec![u32::MAX; n_nodes];
    let mut occ_second: Vec<u32> = vec![u32::MAX; n_nodes];
    let mut cur_gen: u32 = 1;

    // Estacionarias (atrapadas o por max_moves) que cuentan para colisiones futuras
    let mut base_occ:   Vec<u32>  = vec![0; n_nodes];
    let mut base_first: Vec<u32>  = vec![u32::MAX; n_nodes];
    let mut base_second:Vec<u32>  = vec![u32::MAX; n_nodes];
    // Para detectar en qué nodos agregamos estacionarias en el tick actual
    let mut base_gen:   Vec<u32>  = vec![0; n_nodes];

    // Buffers por hormiga (índices absolutos en `ants`)
    let mut next_pos:   Vec<u32>  = ants.iter().map(|a| a.pos).collect();
    let mut trapped_now:Vec<bool>  = vec![false; ants.len()];

    while !active.is_empty() {
        cur_gen = cur_gen.wrapping_add(1);

        // 1) Decidir destinos para activas
        let mut i = 0;
        while i < active.len() {
            let ai = active[i];
            let a = &ants[ai];
            if !a.alive || a.moves >= args.max_moves || a.trapped {
                active.swap_remove(i);
                continue;
            }
            let (np, became_trapped) = choose_next_pos(&world, a.pos, &mut rng);
            next_pos[ai] = np;
            trapped_now[ai] = became_trapped;
            i += 1;
        }
        if active.is_empty() { break; }

        // 2) Ocupación post-movimiento (inicializa con estacionarias)
        for &ai in &active {
            let a = &ants[ai];
            if !a.alive { continue; }
            let nid = next_pos[ai] as usize;

            if gen[nid] != cur_gen {
                gen[nid] = cur_gen;
                occ_count[nid] = base_occ[nid];           // estacionarias
                occ_first[nid] = base_first[nid];
                occ_second[nid] = base_second[nid];
            }

            // sumar la hormiga activa
            if occ_count[nid] == 0 {
                occ_first[nid] = a.id;
                occ_count[nid] = 1;
            } else if occ_count[nid] == 1 {
                if occ_first[nid] == u32::MAX { occ_first[nid] = a.id; }
                else { occ_second[nid] = a.id; }
                occ_count[nid] = 2;
            } else {
                occ_count[nid] += 1;
            }
        }

        // 3) Destruir colonias con colisiones (activas + estacionarias)
        for (nid, g) in gen.iter().enumerate() {
            if *g == cur_gen && occ_count[nid] >= 2 && world.nodes[nid].alive {
                if !args.suppress_events {
                    let a0 = occ_first[nid];
                    let a1 = occ_second[nid];
                    println!(
                        "{} {} {} {}",
                        "💥".red(),
                        world.names[world.nodes[nid].name_idx as usize].bright_red(),
                        "has been destroyed by".red(),
                        format!("ant {} and ant {}", a0, a1).yellow()
                    );
                }
                world.nodes[nid].alive = false;
                // limpiar estacionarias de ese nodo
                base_occ[nid]   = 0;
                base_first[nid] = u32::MAX;
                base_second[nid]= u32::MAX;
            }
        }

        // 4) Commit de hormigas + registrar nuevas estacionarias
        let mut j = 0;
        while j < active.len() {
            let ai = active[j];
            let a = &mut ants[ai];
            if !a.alive { active.swap_remove(j); continue; }

            let nid = next_pos[ai] as usize;

            if !world.nodes[nid].alive {
                a.alive = false;
                a.trapped = false;
                active.swap_remove(j);
                continue;
            }

            if !trapped_now[ai] && nid as u32 != a.pos {
                a.pos = nid as u32;
                a.moves += 1;

                if a.moves >= args.max_moves {
                    // pasa a estacionaria por max_moves
                    let nn = a.pos as usize;
                    if base_occ[nn] == 0       { base_first[nn] = a.id; }
                    else if base_occ[nn] == 1  { base_second[nn] = a.id; }
                    base_occ[nn] += 1;
                    base_gen[nn] = cur_gen;      // tocado este tick
                    active.swap_remove(j);
                    continue;
                }
            } else if trapped_now[ai] && !a.trapped {
                // pasa a estacionaria por atrapada
                a.trapped = true;
                let nn = a.pos as usize;
                if base_occ[nn] == 0       { base_first[nn] = a.id; }
                else if base_occ[nn] == 1  { base_second[nn] = a.id; }
                base_occ[nn] += 1;
                base_gen[nn] = cur_gen;      // tocado este tick
                active.swap_remove(j);
                continue;
            }

            j += 1;
        }

        // 5) Destrucción por solo estacionarias que alcanzan 2 este tick
        for nid in 0..n_nodes {
            if base_gen[nid] == cur_gen && base_occ[nid] >= 2 && world.nodes[nid].alive {
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
                // al destruir el nodo, vaciar su stock estacionario
                base_occ[nid]   = 0;
                base_first[nid] = u32::MAX;
                base_second[nid]= u32::MAX;
            }
        }

        // 6) Early exit: si hay 0 o 1 hormiga viva, ya no habrá más colisiones
        let alive_ants = ants.iter().filter(|a| a.alive).count();
        if alive_ants <= 1 { break; }
    }

    // 3) Salida final (lo que pediste: mapa arriba, resumen debajo)
    println!("{}", "🌍 Surviving Colonies:".blue().bold());
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
