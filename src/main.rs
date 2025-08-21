use ants_code_challenge::prelude::*;
use ants_code_challenge::world::parse_world;
use clap::Parser;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut rng = if let Some(seed) = args.seed {
        fastrand::Rng::with_seed(seed)
    } else {
        fastrand::Rng::new()
    };

    // Parse world and create ants
    let (names, nodes) = parse_world(&args.map)?;
    let mut world = World::new(names, nodes);
    let mut ants = world.create_ants(args.ants, &mut rng);

    // Run simulation
    let mut engine = SimulationEngine::new(&world, args.ants);
    let simulation_time = engine.run_simulation(&mut world, &mut ants, &args, &mut rng);

    // Print results
    engine.print_summary(&world, &args, simulation_time);

    Ok(())
}


