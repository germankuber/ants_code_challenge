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
