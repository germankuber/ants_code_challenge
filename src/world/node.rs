use crate::utils::INVALID_NODE;

/// Graph node: compact and cache-friendly
#[derive(Clone, Debug)]
pub struct Node {
    pub name_idx: u32,   // index into `names`
    pub neighbors: [u32; 4], // neighbors by direction; INVALID_NODE if none
    pub alive: bool,     // colony alive
}

impl Node {
    /// Create a new node with the given name index
    #[inline]
    pub fn new(name_idx: u32) -> Self {
        Self {
            name_idx,
            neighbors: [INVALID_NODE; 4],
            alive: true,
        }
    }

    /// Set neighbor in a specific direction
    #[inline]
    pub fn set_neighbor(&mut self, direction_idx: usize, neighbor_id: u32) {
        self.neighbors[direction_idx] = neighbor_id;
    }

    /// Get neighbor in a specific direction
    #[inline]
    pub fn get_neighbor(&self, direction_idx: usize) -> Option<u32> {
        let neighbor = self.neighbors[direction_idx];
        if neighbor == INVALID_NODE {
            None
        } else {
            Some(neighbor)
        }
    }

    /// Destroy this colony
    #[inline]
    pub fn destroy(&mut self) {
        self.alive = false;
    }

    /// Check if colony is alive
    #[inline]
    pub fn is_alive(&self) -> bool {
        self.alive
    }
}
