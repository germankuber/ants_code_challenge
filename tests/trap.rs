use assert_cmd::prelude::*;
use predicates::str::contains;
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

const BIN: &str = "ants_code_challenge"; // change if needed

#[test]
fn single_isolated_node_traps_ant_immediately() -> Result<(), Box<dyn std::error::Error>> {
    let mut f = NamedTempFile::new()?;
    writeln!(f, "Iso")?;

    let mut cmd = Command::cargo_bin(BIN)?;
    cmd.args([
        "--ants", "1",
        "--map", f.path().to_str().unwrap(),
        "--seed", "7",
        "--suppress-events",
    ]);

    // The node remains alive; survivors should be >=1, but exact value not asserted.
    cmd.assert()
        .success()
        .stdout(contains("Simulation Latency"))
        .stdout(contains("survivors="));

    Ok(())
}
