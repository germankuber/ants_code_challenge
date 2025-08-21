//! # Ant Mania
//! 
//! A simulation of giant space ants invading the planet Hiveum.
//! 
//! This library provides the core functionality for simulating ant movement,
//! collisions, and colony destruction on a graph-based map.

pub mod ant;
pub mod cli;
pub mod direction;
pub mod error;
pub mod simulation;
pub mod utils;
pub mod world;

pub use ant::Ant;
pub use cli::Args;
pub use direction::Direction;
pub use error::{ParseError, Result};
pub use simulation::SimulationEngine;
pub use world::World;

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{Ant, Args, Direction, ParseError, Result, SimulationEngine, World};
}
