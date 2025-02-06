use crate::prelude::*;

use std::{
    collections::HashMap,
    process::{Child, Command, Stdio},
};

use assert_cmd::prelude::CommandCargoExt;
use pueue_lib::{
    settings::{Shared, PUEUE_CONFIG_PATH_ENV},
    state::State,
};

use crate::helper::*;

/// Spawn the daemon by calling the actual pueued binary.
/// This is basically the same as the `standalone_daemon` logic, but it uses the
/// `PUEUE_CONFIG_PATH` environment variable instead of the `--config` flag.
pub async fn standalone_daemon_with_env_config(shared: &Shared) -> Result<Child> {
    // Inject an environment variable into the daemon.
    // This is used to test that the spawned subprocesses won't inherit the daemon's environment.
    let mut envs = HashMap::new();
    envs.insert("PUEUED_TEST_ENV_VARIABLE", "Test".to_owned());
    envs.insert(
        PUEUE_CONFIG_PATH_ENV,
        shared
            .pueue_directory()
            .join("pueue.yml")
            .to_string_lossy()
            .to_string(),
    );

    let child = Command::cargo_bin("pueued")?
        .arg("-vvv")
        .envs(envs)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let tries = 20;
    let mut current_try = 0;

    // Wait up to 1s for the unix socket to pop up.
    let socket_path = shared.unix_socket_path();
    while current_try < tries {
        sleep_ms(50).await;
        if socket_path.exists() {
            return Ok(child);
        }

        current_try += 1;
    }

    bail!("Daemon didn't boot in stand-alone mode after 1sec")
}

/// Test that editing a task without any flags only updates the command.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_with_env_config_path() -> Result<()> {
    let (settings, _tempdir) = daemon_base_setup()?;
    let mut child = standalone_daemon_with_env_config(&settings.shared).await?;
    let shared = &settings.shared;

    // Check if the client can connect to the daemon.
    let mut envs = HashMap::new();
    envs.insert(
        PUEUE_CONFIG_PATH_ENV,
        shared
            .pueue_directory()
            .join("pueue.yml")
            .to_string_lossy()
            .to_string(),
    );
    let output = Command::cargo_bin("pueue")?
        .args(["status", "--json"])
        .envs(envs)
        .current_dir(shared.pueue_directory())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute pueue with env config variable".to_string())?;

    // Deserialize the message and make sure it's a status response.
    let response = String::from_utf8_lossy(&output.stdout);
    let state: State = serde_json::from_str(&response)?;

    assert!(state.tasks.is_empty(), "State must have no tasks");

    child.kill()?;
    Ok(())
}
