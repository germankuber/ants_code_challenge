use crate::ant::Ant;
use crate::direction::Direction;
use crate::utils::INVALID_NODE;
use crate::world::node::Node;

/// Final world: names + nodes (no hashmaps kept at runtime)
#[derive(Clone, Debug)]
pub struct World {
    pub names: Vec<String>,
    pub nodes: Vec<Node>,
}

impl World {
    /// Create a new world from names and nodes
    pub fn new(names: Vec<String>, nodes: Vec<Node>) -> Self {
        Self { names, nodes }
    }

    /// Get a node by id (unsafe for performance)
    /// 
    /// # Safety
    /// The caller must ensure that `idx` is a valid node index
    #[inline(always)]
    pub unsafe fn node_unchecked(&self, idx: u32) -> &Node {
        self.nodes.get_unchecked(idx as usize)
    }

    /// Get a mutable node by id
    #[inline]
    pub fn node_mut(&mut self, idx: u32) -> Option<&mut Node> {
        self.nodes.get_mut(idx as usize)
    }

    /// Get a node by id
    #[inline]
    pub fn node(&self, idx: u32) -> Option<&Node> {
        self.nodes.get(idx as usize)
    }

    /// Place ants uniformly at alive nodes
    pub fn create_ants(&self, count: usize, rng: &mut fastrand::Rng) -> Vec<Ant> {
        let alive_nodes: Vec<u32> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, nd)| nd.is_alive().then_some(i as u32))
            .collect();

        (0..count)
            .map(|i| {
                let pos = alive_nodes[rng.usize(..alive_nodes.len())];
                Ant::new(i as u32, pos)
            })
            .collect()
    }

    /// Choose next position for an ant, returns (new_position, is_trapped)
    /// 
    /// # Safety invariants:
    /// - `ant_pos` is always a valid node index (< world.nodes.len())
    /// - `ant_pos` points to an alive colony (callers guarantee this)
    /// - All neighbor indices in nodes[ant_pos].neighbors are either INVALID_NODE or valid node indices
    #[inline(always)]
    pub fn choose_next_position(&self, ant_pos: u32, rng: &mut fastrand::Rng) -> (u32, bool) {
        let node = unsafe { self.node_unchecked(ant_pos) };
        debug_assert!(node.is_alive());

        // Manual unroll like the original - this is the performance critical path
        let mut opts = [INVALID_NODE; 4];
        let mut k = 0usize;

        let n0 = node.neighbors[0];
        let n1 = node.neighbors[1];
        let n2 = node.neighbors[2];
        let n3 = node.neighbors[3];

        // Manually unrolled for maximum performance
        if n0 != INVALID_NODE {
            let alive = unsafe { self.node_unchecked(n0) }.is_alive();
            opts[k] = n0;
            k += alive as usize;
        }
        if n1 != INVALID_NODE {
            let alive = unsafe { self.node_unchecked(n1) }.is_alive();
            opts[k] = n1;
            k += alive as usize;
        }
        if n2 != INVALID_NODE {
            let alive = unsafe { self.node_unchecked(n2) }.is_alive();
            opts[k] = n2;
            k += alive as usize;
        }
        if n3 != INVALID_NODE {
            let alive = unsafe { self.node_unchecked(n3) }.is_alive();
            opts[k] = n3;
            k += alive as usize;
        }

        if k == 0 {
            (ant_pos, true) // trapped
        } else {
            (opts[rng.usize(..k)], false)
        }
    }

    /// Print the remaining world in the same input format
    pub fn print_world(&self) {
        let mut line = String::with_capacity(128);
        for node in &self.nodes {
            if !node.is_alive() {
                continue;
            }
            line.clear();
            line.push_str(&self.names[node.name_idx as usize]);
            
            for &direction in &Direction::ALL {
                let neighbor_id = node.neighbors[direction.index()];
                if neighbor_id != INVALID_NODE && self.nodes[neighbor_id as usize].is_alive() {
                    line.push(' ');
                    line.push_str(direction.as_str());
                    line.push('=');
                    line.push_str(&self.names[self.nodes[neighbor_id as usize].name_idx as usize]);
                }
            }
            // Commented out to match original behavior
            // println!("{}", line);
        }
    }

    /// Count surviving colonies
    pub fn count_survivors(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_alive()).count()
    }

    /// Get the name of a colony by node id
    pub fn get_colony_name(&self, node_id: u32) -> &str {
        &self.names[self.nodes[node_id as usize].name_idx as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::parser::parse_world_from_str;

    /// Helper function to find a node id by name
    fn id_of(world: &World, name: &str) -> u32 {
        world
            .names
            .iter()
            .position(|n| n == name)
            .expect("name not found") as u32
    }

    #[test]
    fn test_world_creation() {
        let (names, nodes) = parse_world_from_str("A north=B\nB south=A\n");
        let world = World::new(names, nodes);
        
        assert_eq!(world.names.len(), 2);
        assert_eq!(world.nodes.len(), 2);
    }

    #[test]
    fn test_choose_next_position_trapped() {
        let (names, nodes) = parse_world_from_str("Isolated\n");
        let world = World::new(names, nodes);
        let isolated_id = id_of(&world, "Isolated");
        
        let mut rng = fastrand::Rng::with_seed(123);
        let (next_pos, is_trapped) = world.choose_next_position(isolated_id, &mut rng);
        
        assert_eq!(next_pos, isolated_id);
        assert!(is_trapped);
    }

    #[test]
    fn test_choose_next_position_single_exit() {
        let (names, nodes) = parse_world_from_str("A east=B\n");
        let world = World::new(names, nodes);
        let a_id = id_of(&world, "A");
        let b_id = id_of(&world, "B");
        
        let mut rng = fastrand::Rng::with_seed(42);
        let (next_pos, is_trapped) = world.choose_next_position(a_id, &mut rng);
        
        assert_eq!(next_pos, b_id);
        assert!(!is_trapped);
    }

    #[test]
    fn test_create_ants() {
        let (names, nodes) = parse_world_from_str("A north=B\nB south=A\n");
        let world = World::new(names, nodes);
        
        let mut rng = fastrand::Rng::with_seed(123);
        let ants = world.create_ants(5, &mut rng);
        
        assert_eq!(ants.len(), 5);
        for (i, ant) in ants.iter().enumerate() {
            assert_eq!(ant.id, i as u32);
            assert!(ant.is_alive());
            assert!(!ant.is_trapped());
            assert_eq!(ant.moves, 0);
        }
    }

    #[test]
    fn test_count_survivors() {
        let (names, mut nodes) = parse_world_from_str("A north=B\nB south=A\nC\n");
        let world = World::new(names, nodes);
        
        assert_eq!(world.count_survivors(), 3);
        
        // Destroy one colony
        let mut world = world;
        let c_id = id_of(&world, "C");
        world.nodes[c_id as usize].destroy();
        
        assert_eq!(world.count_survivors(), 2);
    }

    #[test]
    fn test_get_colony_name() {
        let (names, nodes) = parse_world_from_str("Colony1 north=Colony2\n");
        let world = World::new(names, nodes);
        let colony1_id = id_of(&world, "Colony1");
        
        assert_eq!(world.get_colony_name(colony1_id), "Colony1");
    }

    #[test]
    fn test_node_access() {
        let (names, nodes) = parse_world_from_str("A\n");
        let world = World::new(names, nodes);
        let a_id = id_of(&world, "A");
        
        let node = world.node(a_id).unwrap();
        assert!(node.is_alive());
        
        let mut world = world;
        let node_mut = world.node_mut(a_id);
        assert!(node_mut.is_some());
    }
}
