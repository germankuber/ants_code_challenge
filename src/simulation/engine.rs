use crate::ant::Ant;
use crate::cli::Args;
use crate::world::World;
use colored::Colorize;
use std::time::Instant;

/// Main simulation engine that coordinates the ant simulation
/// Optimized to match original performance while maintaining modularity
pub struct SimulationEngine;

impl SimulationEngine {
    /// Create a new simulation engine
    pub fn new(_world: &World, _ant_count: usize) -> Self {
        Self
    }

    /// Run the complete simulation - optimized version that closely matches original
    pub fn run_simulation(
        &mut self,
        world: &mut World,
        ants: &mut Vec<Ant>,
        args: &Args,
        rng: &mut fastrand::Rng,
    ) -> std::time::Duration {
        // Handle initial collisions at t=0 (same as original)
        self.handle_initial_collisions(world, ants, args);

        // Initialize active ants list
        let mut active: Vec<usize> = Vec::with_capacity(args.ants);
        active.extend(ants.iter().enumerate().filter_map(|(i, a)| {
            if a.is_alive() && !a.is_trapped() && a.moves < args.max_moves {
                Some(i)
            } else {
                None
            }
        }));

        let sim_start = Instant::now();

        // Per-node "generation" trick avoids clearing large arrays (same as original)
        let n_nodes = world.nodes.len();
        let mut gen = vec![0u32; n_nodes];
        let mut occ_count = vec![0u32; n_nodes];
        let mut occ_first = vec![u32::MAX; n_nodes];
        let mut occ_second = vec![u32::MAX; n_nodes];
        let mut cur_gen: u32 = 1;

        // Stationary stock (same as original)
        let mut base_occ = vec![0u32; n_nodes];
        let mut base_first = vec![u32::MAX; n_nodes];
        let mut base_second = vec![u32::MAX; n_nodes];

        // "Touched" node lists (same as original)
        let mut touched_nodes: Vec<usize> = Vec::with_capacity(4096);
        let mut base_touched: Vec<usize> = Vec::with_capacity(1024);

        // Per-ant buffers (same as original)
        let mut next_pos: Vec<u32> = ants.iter().map(|a| a.pos).collect();
        let mut trapped_now: Vec<bool> = vec![false; ants.len()];

        // Main simulation loop (identical to original)
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
                let (np, became_trapped) = world.choose_next_position(a.pos, rng);
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

            // (3) Destroy collided colonies
            for &nid in &touched_nodes {
                if occ_count[nid] >= 2 && world.nodes[nid].is_alive() {
                    self.log_destruction(args, world, nid, occ_first[nid], occ_second[nid]);
                    world.nodes[nid].destroy();
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
                let node_alive = world.nodes[nid].is_alive();

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
                    a.move_to(nid as u32);

                    if a.has_max_moves(args.max_moves) {
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
                } else if trapped_now[ai] && !a.is_trapped() {
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

            // (5) Pure-stationary destruction
            for &nid in &base_touched {
                if base_occ[nid] >= 2 && world.nodes[nid].is_alive() {
                    self.log_destruction(args, world, nid, base_first[nid], base_second[nid]);
                    world.nodes[nid].destroy();
                    base_occ[nid] = 0;
                    base_first[nid] = u32::MAX;
                    base_second[nid] = u32::MAX;
                }
            }

            // (6) Early exit
            let alive_ants = ants.iter().filter(|a| a.is_alive()).count();
            if alive_ants <= 1 {
                break;
            }
        }

        sim_start.elapsed()
    }

    /// Handle initial collisions at t=0
    fn handle_initial_collisions(&self, world: &mut World, ants: &mut [Ant], args: &Args) {
        let n = world.nodes.len();
        let mut occ_count = vec![0u32; n];
        let mut occ_first = vec![u32::MAX; n];
        let mut occ_second = vec![u32::MAX; n];
        let mut destroyed = vec![false; n];

        for a in ants.iter() {
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
            if occ_count[nid] >= 2 && world.nodes[nid].is_alive() {
                self.log_destruction(args, world, nid, occ_first[nid], occ_second[nid]);
                world.nodes[nid].destroy();
                destroyed[nid] = true;
            }
        }

        for a in ants.iter_mut() {
            if destroyed[a.pos as usize] {
                a.set_alive(false);
                a.set_trapped(false);
            }
        }
    }

    /// Log colony destruction event
    #[inline]
    fn log_destruction(&self, args: &Args, world: &World, node_id: usize, ant1: u32, ant2: u32) {
        if args.suppress_events {
            return;
        }
        println!(
            "{} {} {} {}",
            "💥".red(),
            world.get_colony_name(node_id as u32).bright_red(),
            "has been destroyed by".red(),
            format!("ant {} and ant {}", ant1, ant2).yellow()
        );
    }

    /// Print simulation summary
    pub fn print_summary(
        &self,
        world: &World,
        args: &Args,
        simulation_time: std::time::Duration,
    ) {
        world.print_world();

        let survivors = world.count_survivors();
        println!(
            "\n{}\n{} {:.3} ms {} {} {} {} {}",
            "===".bright_blue().bold(),
            "⏱️  Simulation Latency:".green().bold(),
            simulation_time.as_secs_f64() * 1000.0,
            "(map loaded)".dimmed(),
            "|".dimmed(),
            format!("ants={}", args.ants).cyan(),
            format!("max_moves={}", args.max_moves).cyan(),
            format!("survivors={}", survivors).cyan(),
        );
    }
}
