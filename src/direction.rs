use crate::error::ParseError;
use std::str::FromStr;

/// 4 fixed directions for tiny, predictable loops
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Direction {
    North = 0,
    South = 1,
    East = 2,
    West = 3,
}

impl FromStr for Direction {
    type Err = ParseError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // byte match is faster than string match
        match s.as_bytes() {
            b"north" => Ok(Direction::North),
            b"south" => Ok(Direction::South),
            b"east" => Ok(Direction::East),
            b"west" => Ok(Direction::West),
            _ => Err(ParseError::InvalidDirection(s.to_string())),
        }
    }
}

impl Direction {
    /// All possible directions
    pub const ALL: [Direction; 4] = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
    ];

    /// Get direction index for array indexing
    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Get direction name as string
    pub const fn as_str(self) -> &'static str {
        match self {
            Direction::North => "north",
            Direction::South => "south", 
            Direction::East => "east",
            Direction::West => "west",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_direction_parsing() {
        assert_eq!(Direction::from_str("north").unwrap(), Direction::North);
        assert_eq!(Direction::from_str("south").unwrap(), Direction::South);
        assert_eq!(Direction::from_str("east").unwrap(), Direction::East);
        assert_eq!(Direction::from_str("west").unwrap(), Direction::West);
        
        assert!(Direction::from_str("invalid").is_err());
        assert!(Direction::from_str("North").is_err()); // Case sensitive
    }

    #[test]
    fn test_direction_index() {
        assert_eq!(Direction::North.index(), 0);
        assert_eq!(Direction::South.index(), 1);
        assert_eq!(Direction::East.index(), 2);
        assert_eq!(Direction::West.index(), 3);
    }

    #[test]
    fn test_direction_as_str() {
        assert_eq!(Direction::North.as_str(), "north");
        assert_eq!(Direction::South.as_str(), "south");
        assert_eq!(Direction::East.as_str(), "east");
        assert_eq!(Direction::West.as_str(), "west");
    }

    #[test]
    fn test_all_directions() {
        assert_eq!(Direction::ALL.len(), 4);
        assert!(Direction::ALL.contains(&Direction::North));
        assert!(Direction::ALL.contains(&Direction::South));
        assert!(Direction::ALL.contains(&Direction::East));
        assert!(Direction::ALL.contains(&Direction::West));
    }
}
