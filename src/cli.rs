use clap::Parser;

/// CLI arguments for the ant simulation
#[derive(Parser, Debug)]
#[command(name = "ant_mania", about = "🐜 Ant invasion simulator on Hiveum")]
pub struct Args {
    /// Number of ants
    #[arg(short = 'n', long = "ants")]
    pub ants: usize,

    /// Path to the map file
    #[arg(short = 'm', long = "map")]
    pub map: String,

    /// Maximum moves per ant
    #[arg(long, default_value_t = 10_000)]
    pub max_moves: u32,

    /// Random seed
    #[arg(long)]
    pub seed: Option<u64>,

    /// Suppress fight logs (for benchmarks)
    #[arg(long, default_value_t = false)]
    pub suppress_events: bool,
}
