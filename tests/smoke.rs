// Integration tests for the binary using assert_cmd.
// These tests shell out the compiled binary and validate observable behavior.

use assert_cmd::prelude::*;
use predicates::str::contains;
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

const BIN: &str = "ants_code_challenge"; // change if your binary name differs

#[test]
fn prints_summary_and_survivors() -> Result<(), Box<dyn std::error::Error>> {
    // Small map with a few links
    let mut f = NamedTempFile::new()?;
    writeln!(
        f,
        "A north=B west=C\nB south=A\nC east=A\nD\n"
    )?;

    let mut cmd = Command::cargo_bin(BIN)?;
    cmd.args([
        "--ants", "200",
        "--map", f.path().to_str().unwrap(),
        "--seed", "42",
        "--suppress-events",
    ]);

    cmd.assert()
        .success()
        .stdout(contains("==="))
        .stdout(contains("Simulation Latency"))
        .stdout(contains("survivors="));

    Ok(())
}

#[test]
fn t0_collision_with_single_colony_leaves_zero_survivors() -> Result<(), Box<dyn std::error::Error>> {
    // Single colony => both ants must start there => t=0 destroy it.
    let mut f = NamedTempFile::new()?;
    writeln!(f, "X")?;

    let mut cmd = Command::cargo_bin(BIN)?;
    cmd.args([
        "-n", "2",
        "-m", f.path().to_str().unwrap(),
        "--seed", "123",
        "--suppress-events",
    ]);

    // There should be no world lines printed (colony destroyed at t=0) and survivors=0
    cmd.assert()
        .success()
        .stdout(contains("survivors=0"));

    Ok(())
}
