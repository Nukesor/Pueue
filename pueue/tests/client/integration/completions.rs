use crate::prelude::*;

use std::process::{Command, Stdio};

use assert_cmd::prelude::*;
use rstest::rstest;

/// Make sure completion for all shells work as expected.
/// This test tests writing to file.
#[rstest]
#[case("zsh")]
#[case("elvish")]
#[case("bash")]
#[case("fish")]
#[case("power-shell")]
#[case("nushell")]
#[test]
fn autocompletion_generation_to_file(#[case] shell: &'static str) -> Result<()> {
    let output = Command::cargo_bin("pueue")?
        .arg("completions")
        .arg(shell)
        .arg("./")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(env!("CARGO_TARGET_TMPDIR"))
        .output()
        .context(format!("Failed to run completion generation for {shell}:"))?;

    assert!(
        output.status.success(),
        "Completion for {shell} didn't finish successfully."
    );

    Ok(())
}

/// Make sure completion for all shells work as expected.
/// This test tests writing to stdout.
#[rstest]
#[case("zsh")]
#[case("elvish")]
#[case("bash")]
#[case("fish")]
#[case("power-shell")]
#[case("nushell")]
#[test]
fn autocompletion_generation_to_stdout(#[case] shell: &'static str) -> Result<()> {
    let output = Command::cargo_bin("pueue")?
        .arg("completions")
        .arg(shell)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(env!("CARGO_TARGET_TMPDIR"))
        .output()
        .context(format!("Failed to run completion generation for {shell}:"))?;

    assert!(
        output.status.success(),
        "Completion for {shell} didn't finish successfully."
    );

    Ok(())
}
