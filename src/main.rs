use clap::Parser;
use colored::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

/// 4 direcciones fijas para loops tiny y branch-predictable
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
        match s.as_bytes() {
            b"north" => Some(Dir::North),
            b"south" => Some(Dir::South),
            b"east" => Some(Dir::East),
            b"west" => Some(Dir::West),
            _ => None,
        }
    }
    
    const ALL: [Dir; 4] = [Dir::North, Dir::South, Dir::East, Dir::West];
    
    #[inline(always)]
    fn idx(self) -> usize {
        self as usize
    }
}

/// Sentinel para "sin túnel"
const INVALID: u32 = u32::MAX;

/// Nodo del grafo (compacto y cache-friendly)
#[derive(Clone, Debug)]
#[repr(C)]
struct Node {
    name_idx: u32,   // índice en names
    neigh: [u32; 4], // vecinos por dirección; INVALID si no hay
    alive: bool,     // colonia viva
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

/// Mundo final sin hashmaps (solo nombres + nodos)
#[derive(Clone, Debug)]
struct World {
    names: Vec<String>,
    nodes: Vec<Node>,
}

/// Estado de hormiga - reorganizado para mejor alineación de memoria
#[derive(Clone, Debug)]
#[repr(C)]
struct Ant {
    pos: u32,
    id: u32,
    moves: u32,
    state: u8, // Combina alive y trapped en un solo byte
}

impl Ant {
    const ALIVE_BIT: u8 = 0b01;
    const TRAPPED_BIT: u8 = 0b10;
    
    #[inline(always)]
    fn is_alive(&self) -> bool {
        self.state & Self::ALIVE_BIT != 0
    }
    
    #[inline(always)]
    fn is_trapped(&self) -> bool {
        self.state & Self::TRAPPED_BIT != 0
    }
    
    #[inline(always)]
    fn set_alive(&mut self, alive: bool) {
        if alive {
            self.state |= Self::ALIVE_BIT;
        } else {
            self.state &= !Self::ALIVE_BIT;
        }
    }
    
    #[inline(always)]
    fn set_trapped(&mut self, trapped: bool) {
        if trapped {
            self.state |= Self::TRAPPED_BIT;
        } else {
            self.state &= !Self::TRAPPED_BIT;
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

/// Parseo optimizado con preallocación
fn parse_world(path: &str) -> World {
    let file = File::open(path).expect("cannot open map file");
    let reader = BufReader::with_capacity(65536, file); // Buffer más grande

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
            if let Some(eq_pos) = kv.find('=') {
                let dir_s = &kv[..eq_pos];
                let dst_s = &kv[eq_pos + 1..];
                if let Some(dir) = Dir::from_str(dir_s) {
                    edges.push((src_id, dir, dst_s.to_string()));
                }
            }
        }
    }

    // Asegurar ids para destinos no vistos como origen
    for (_, _, dst) in &edges {
        name_to_id.entry(dst.clone()).or_insert_with(|| {
            let id = names.len() as u32;
            names.push(dst.clone());
            id
        });
    }

    let mut nodes: Vec<Node> = Vec::with_capacity(names.len());
    for i in 0..names.len() {
        nodes.push(Node::new(i as u32));
    }

    // Usar referencias para evitar copias innecesarias
    for (src, dir, dst_name) in &edges {
        if let Some(&dst) = name_to_id.get(dst_name) {
            nodes[*src as usize].neigh[dir.idx()] = dst;
        }
    }

    World { names, nodes }
}

/// Coloca N hormigas en nodos vivos al azar - optimizada
fn create_ants(world: &World, n: usize, rng: &mut fastrand::Rng) -> Vec<Ant> {
    let alive_nodes: Vec<u32> = world
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, nd)| {
            if nd.alive {
                Some(i as u32)
            } else {
                None
            }
        })
        .collect();

    let mut ants = Vec::with_capacity(n);
    let alive_len = alive_nodes.len();
    
    for i in 0..n {
        let pos = alive_nodes[rng.usize(..alive_len)];
        ants.push(Ant {
            id: i as u32,
            pos,
            moves: 0,
            state: Ant::ALIVE_BIT,
        });
    }
    ants
}

/// Próximo destino uniforme entre salidas vivas - optimizado con menos branches
#[inline(always)]
fn choose_next_pos(world: &World, ant_pos: u32, rng: &mut fastrand::Rng) -> (u32, bool) {
    let node = unsafe { world.nodes.get_unchecked(ant_pos as usize) };
    debug_assert!(node.alive);

    // Unroll manual del loop para mejor predicción de branches
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

/// Imprime el mundo remanente - optimizada con StringBuilder
fn print_world(world: &World) {
    let mut output = String::with_capacity(world.nodes.len() * 80);
    
    for nd in &world.nodes {
        if !nd.alive {
            continue;
        }
        
        output.clear();
        output.push_str(&world.names[nd.name_idx as usize]);
        
        for &d in &Dir::ALL {
            let nid = nd.neigh[d.idx()];
            if nid != INVALID && world.nodes[nid as usize].alive {
                output.push(' ');
                output.push_str(match d {
                    Dir::North => "north=",
                    Dir::South => "south=",
                    Dir::East => "east=",
                    Dir::West => "west=",
                });
                output.push_str(&world.names[world.nodes[nid as usize].name_idx as usize]);
            }
        }
        // println!("{}", output);
    }
}

fn main() {
    let args = Args::parse();
    let mut rng = if let Some(seed) = args.seed {
        fastrand::Rng::with_seed(seed)
    } else {
        fastrand::Rng::new()
    };

    // 1) Map + ants
    let mut world = parse_world(&args.map);
    let mut ants = create_ants(&world, args.ants, &mut rng);

    // PRE-PASS t=0 - optimizado con menos allocaciones
    {
        let n_nodes = world.nodes.len();
        let mut occ: Vec<Vec<usize>> = vec![Vec::with_capacity(4); n_nodes];
        
        for (ai, a) in ants.iter().enumerate() {
            if a.is_alive() {
                occ[a.pos as usize].push(ai);
            }
        }
        
        for nid in 0..n_nodes {
            let occ_len = occ[nid].len();
            if occ_len >= 2 && world.nodes[nid].alive {
                if !args.suppress_events {
                    let a0 = ants[occ[nid][0]].id;
                    let a1 = ants[occ[nid][1]].id;
                    // println!(
                    //     "{} {} {} {}",
                    //     "💥".red(),
                    //     world.names[world.nodes[nid].name_idx as usize].bright_red(),
                    //     "has been destroyed by".red(),
                    //     format!("ant {} and ant {}", a0, a1).yellow()
                    // );
                }
                world.nodes[nid].alive = false;
                
                // Usar slice para evitar bounds checks repetidos
                let occ_slice = &occ[nid];
                for &ai in occ_slice {
                    let aa = &mut ants[ai];
                    aa.set_alive(false);
                    aa.set_trapped(false);
                }
            }
        }
    }

    // Conjunto de hormigas activas - preallocado
    let mut active: Vec<usize> = Vec::with_capacity(args.ants);
    active.extend(
        ants.iter()
            .enumerate()
            .filter_map(|(i, a)| {
                if a.is_alive() && !a.is_trapped() && a.moves < args.max_moves {
                    Some(i)
                } else {
                    None
                }
            })
    );

    // 2) Simulación
    let sim_start = Instant::now();

    let n_nodes = world.nodes.len();
    let mut gen: Vec<u32> = vec![0; n_nodes];
    let mut occ_count: Vec<u32> = vec![0; n_nodes];
    let mut occ_first: Vec<u32> = vec![u32::MAX; n_nodes];
    let mut occ_second: Vec<u32> = vec![u32::MAX; n_nodes];
    let mut cur_gen: u32 = 1;

    let mut base_occ: Vec<u32> = vec![0; n_nodes];
    let mut base_first: Vec<u32> = vec![u32::MAX; n_nodes];
    let mut base_second: Vec<u32> = vec![u32::MAX; n_nodes];
    let mut base_gen: Vec<u32> = vec![0; n_nodes];

    let mut next_pos: Vec<u32> = Vec::with_capacity(ants.len());
    next_pos.extend(ants.iter().map(|a| a.pos));
    let mut trapped_now: Vec<bool> = vec![false; ants.len()];

    while !active.is_empty() {
        cur_gen = cur_gen.wrapping_add(1);

        // 1) Decidir destinos - usar índices para evitar borrowing conflicts
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

        // 2) Ocupación post-movimiento - optimizado con menos branches
        for &ai in &active {
            let a = &ants[ai];
            if !a.is_alive() {
                continue;
            }
            
            let nid = next_pos[ai] as usize;
            
            // Usar referencia mutable una sola vez
            if gen[nid] != cur_gen {
                gen[nid] = cur_gen;
                occ_count[nid] = base_occ[nid];
                occ_first[nid] = base_first[nid];
                occ_second[nid] = base_second[nid];
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

        // 3) Destruir colonias - usar iteradores cuando sea posible
        for nid in 0..n_nodes {
            if gen[nid] == cur_gen && occ_count[nid] >= 2 && world.nodes[nid].alive {
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
                base_occ[nid] = 0;
                base_first[nid] = u32::MAX;
                base_second[nid] = u32::MAX;
            }
        }

        // 4) Commit de hormigas - split borrowing para evitar conflictos
        let mut j = 0;
        while j < active.len() {
            let ai = active[j];
            let nid = next_pos[ai] as usize;
            
            // Primero check del mundo (immutable borrow)
            let node_alive = world.nodes[nid].alive;
            
            // Luego modificar hormiga (mutable borrow separado)
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
                    let nn = a.pos as usize;
                    match base_occ[nn] {
                        0 => base_first[nn] = a.id,
                        1 => base_second[nn] = a.id,
                        _ => {}
                    }
                    base_occ[nn] += 1;
                    base_gen[nn] = cur_gen;
                    active.swap_remove(j);
                    continue;
                }
            } else if trapped_now[ai] && !a.is_trapped() {
                a.set_trapped(true);
                let nn = a.pos as usize;
                match base_occ[nn] {
                    0 => base_first[nn] = a.id,
                    1 => base_second[nn] = a.id,
                    _ => {}
                }
                base_occ[nn] += 1;
                base_gen[nn] = cur_gen;
                active.swap_remove(j);
                continue;
            }

            j += 1;
        }

        // 5) Destrucción por estacionarias
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
                base_occ[nid] = 0;
                base_first[nid] = u32::MAX;
                base_second[nid] = u32::MAX;
            }
        }

        // 6) Early exit optimizado
        let alive_ants = ants.iter().filter(|a| a.is_alive()).count();
        if alive_ants <= 1 {
            break;
        }
    }

    // 3) Salida final
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