use crate::ant::Ant;
use crate::cli::Args;
use crate::world::World;
use colored::Colorize;

/// Handles collision detection and colony destruction
pub struct CollisionDetector {
    /// Per-node occupancy count for current generation
    occupancy_count: Vec<u32>,
    /// First ant to occupy each node in current generation
    occupancy_first: Vec<u32>,
    /// Second ant to occupy each node in current generation
    occupancy_second: Vec<u32>,
    /// Generation tracker for efficient array reuse
    generation: Vec<u32>,
    /// Current generation counter
    current_generation: u32,
    /// Base occupancy for stationary ants
    base_occupancy: Vec<u32>,
    /// First stationary ant per node
    base_first: Vec<u32>,
    /// Second stationary ant per node
    base_second: Vec<u32>,
    /// Nodes touched in current iteration
    touched_nodes: Vec<usize>,
    /// Nodes with new stationary ants
    base_touched: Vec<usize>,
}

impl CollisionDetector {
    /// Create a new collision detector for a world with the given number of nodes
    pub fn new(node_count: usize) -> Self {
        Self {
            occupancy_count: vec![0u32; node_count],
            occupancy_first: vec![u32::MAX; node_count],
            occupancy_second: vec![u32::MAX; node_count],
            generation: vec![0u32; node_count],
            current_generation: 1,
            base_occupancy: vec![0u32; node_count],
            base_first: vec![u32::MAX; node_count],
            base_second: vec![u32::MAX; node_count],
            touched_nodes: Vec::with_capacity(4096),
            base_touched: Vec::with_capacity(1024),
        }
    }

    /// Handle initial collisions at t=0
    pub fn handle_initial_collisions(&mut self, world: &mut World, ants: &mut [Ant], args: &Args) {
        let node_count = world.nodes.len();
        let mut destroyed = vec![false; node_count];

        // Reset arrays for t=0
        for i in 0..node_count {
            self.occupancy_count[i] = 0;
            self.occupancy_first[i] = u32::MAX;
            self.occupancy_second[i] = u32::MAX;
        }

        // Count ant occupancy
        for ant in ants.iter() {
            if ant.is_alive() {
                let node_id = ant.pos as usize;
                match self.occupancy_count[node_id] {
                    0 => {
                        self.occupancy_first[node_id] = ant.id;
                        self.occupancy_count[node_id] = 1;
                    }
                    1 => {
                        self.occupancy_second[node_id] = ant.id;
                        self.occupancy_count[node_id] = 2;
                    }
                    _ => {
                        self.occupancy_count[node_id] += 1;
                    }
                }
            }
        }

        // Destroy colonies with collisions
        for node_id in 0..node_count {
            if self.occupancy_count[node_id] >= 2 && world.nodes[node_id].is_alive() {
                self.log_destruction(args, world, node_id, self.occupancy_first[node_id], self.occupancy_second[node_id]);
                world.nodes[node_id].destroy();
                destroyed[node_id] = true;
            }
        }

        // Kill ants on destroyed colonies
        for ant in ants.iter_mut() {
            if destroyed[ant.pos as usize] {
                ant.set_alive(false);
                ant.set_trapped(false);
            }
        }
    }

    /// Process collisions for active ants
    pub fn process_collisions(
        &mut self,
        world: &mut World,
        ants: &[Ant],
        active_indices: &[usize],
        next_positions: &[u32],
        args: &Args,
    ) {
        self.current_generation = self.current_generation.wrapping_add(1);
        self.touched_nodes.clear();

        // Build occupancy (initialize from stationary, then add active)
        for &ant_idx in active_indices {
            let ant = &ants[ant_idx];
            if !ant.is_alive() {
                continue;
            }
            let node_id = next_positions[ant_idx] as usize;

            if self.generation[node_id] != self.current_generation {
                self.generation[node_id] = self.current_generation;
                self.occupancy_count[node_id] = self.base_occupancy[node_id];
                self.occupancy_first[node_id] = self.base_first[node_id];
                self.occupancy_second[node_id] = self.base_second[node_id];
                self.touched_nodes.push(node_id);
            }

            match self.occupancy_count[node_id] {
                0 => {
                    self.occupancy_first[node_id] = ant.id;
                    self.occupancy_count[node_id] = 1;
                }
                1 => {
                    if self.occupancy_first[node_id] == u32::MAX {
                        self.occupancy_first[node_id] = ant.id;
                    } else {
                        self.occupancy_second[node_id] = ant.id;
                    }
                    self.occupancy_count[node_id] = 2;
                }
                _ => {
                    self.occupancy_count[node_id] += 1;
                }
            }
        }

        // Destroy collided colonies (only touched)
        for &node_id in &self.touched_nodes {
            if self.occupancy_count[node_id] >= 2 && world.nodes[node_id].is_alive() {
                self.log_destruction(args, world, node_id, self.occupancy_first[node_id], self.occupancy_second[node_id]);
                world.nodes[node_id].destroy();
                // If destroyed, their stationary stock is now irrelevant
                self.base_occupancy[node_id] = 0;
                self.base_first[node_id] = u32::MAX;
                self.base_second[node_id] = u32::MAX;
            }
        }
    }

    /// Add a stationary ant to base occupancy
    pub fn add_stationary_ant(&mut self, node_id: usize, ant_id: u32) {
        self.base_touched.clear(); // Clear at start of each iteration

        match self.base_occupancy[node_id] {
            0 => self.base_first[node_id] = ant_id,
            1 => self.base_second[node_id] = ant_id,
            _ => {}
        }
        if self.base_occupancy[node_id] < 2 {
            self.base_touched.push(node_id);
        }
        self.base_occupancy[node_id] += 1;
    }

    /// Process pure-stationary destructions
    pub fn process_stationary_collisions(&mut self, world: &mut World, args: &Args) {
        for &node_id in &self.base_touched {
            if self.base_occupancy[node_id] >= 2 && world.nodes[node_id].is_alive() {
                self.log_destruction(args, world, node_id, self.base_first[node_id], self.base_second[node_id]);
                world.nodes[node_id].destroy();
                self.base_occupancy[node_id] = 0;
                self.base_first[node_id] = u32::MAX;
                self.base_second[node_id] = u32::MAX;
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
}
