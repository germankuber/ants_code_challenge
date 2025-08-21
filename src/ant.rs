/// Ant state packed into a byte (alive/trapped) + aligned fields
#[derive(Clone, Debug)]
pub struct Ant {
    pub pos: u32,
    pub id: u32,
    pub moves: u32,
    state: u8, // bit 0 = alive, bit 1 = trapped
}

impl Ant {
    const ALIVE: u8 = 0b01;
    const TRAPPED: u8 = 0b10;

    /// Create a new ant at the given position
    pub fn new(id: u32, pos: u32) -> Self {
        Self {
            id,
            pos,
            moves: 0,
            state: Self::ALIVE,
        }
    }

    /// Check if ant is alive
    #[inline]
    pub fn is_alive(&self) -> bool {
        self.state & Self::ALIVE != 0
    }

    /// Check if ant is trapped
    #[inline]
    pub fn is_trapped(&self) -> bool {
        self.state & Self::TRAPPED != 0
    }

    /// Set alive state
    #[inline]
    pub fn set_alive(&mut self, alive: bool) {
        if alive {
            self.state |= Self::ALIVE;
        } else {
            self.state &= !Self::ALIVE;
        }
    }

    /// Set trapped state
    #[inline]
    pub fn set_trapped(&mut self, trapped: bool) {
        if trapped {
            self.state |= Self::TRAPPED;
        } else {
            self.state &= !Self::TRAPPED;
        }
    }

    /// Move ant to new position and increment move counter
    pub fn move_to(&mut self, new_pos: u32) {
        self.pos = new_pos;
        self.moves += 1;
    }

    /// Check if ant has reached maximum moves
    pub fn has_max_moves(&self, max_moves: u32) -> bool {
        self.moves >= max_moves
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ant_creation() {
        let ant = Ant::new(42, 100);
        
        assert_eq!(ant.id, 42);
        assert_eq!(ant.pos, 100);
        assert_eq!(ant.moves, 0);
        assert!(ant.is_alive());
        assert!(!ant.is_trapped());
    }

    #[test]
    fn test_ant_state_management() {
        let mut ant = Ant::new(1, 10);
        
        // Initial state
        assert!(ant.is_alive());
        assert!(!ant.is_trapped());
        
        // Set trapped
        ant.set_trapped(true);
        assert!(ant.is_trapped());
        assert!(ant.is_alive()); // Should still be alive
        
        // Set not alive
        ant.set_alive(false);
        assert!(!ant.is_alive());
        assert!(ant.is_trapped()); // Trapped state should be unchanged
        
        // Reset states
        ant.set_alive(true);
        ant.set_trapped(false);
        assert!(ant.is_alive());
        assert!(!ant.is_trapped());
    }

    #[test]
    fn test_ant_movement() {
        let mut ant = Ant::new(1, 10);
        
        assert_eq!(ant.moves, 0);
        assert_eq!(ant.pos, 10);
        
        ant.move_to(20);
        assert_eq!(ant.pos, 20);
        assert_eq!(ant.moves, 1);
        
        ant.move_to(30);
        assert_eq!(ant.pos, 30);
        assert_eq!(ant.moves, 2);
    }

    #[test]
    fn test_ant_max_moves() {
        let mut ant = Ant::new(1, 10);
        
        assert!(!ant.has_max_moves(10));
        
        // Move ant 5 times
        for _ in 0..5 {
            ant.move_to(ant.pos + 1);
        }
        
        assert!(!ant.has_max_moves(10));
        assert!(ant.has_max_moves(5));
        assert!(ant.has_max_moves(3));
    }
}
