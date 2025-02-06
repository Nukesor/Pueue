use crate::prelude::*;

use std::collections::HashMap;

use assert_cmd::prelude::*;
use std::process::{Command, Output, Stdio};

use pueue_lib::settings::Shared;
use pueue_lib::task::TaskStatus;

use crate::helper::get_state;

/// Spawn a client command that connects to a specific daemon.
pub fn run_client_command(shared: &Shared, args: &[&str]) -> Result<Output> {
    // Inject an environment variable into the pueue command.
    // This is used to ensure that the environment is properly captured and forwarded.
    let mut envs = HashMap::new();
    envs.insert("PUEUED_TEST_ENV_VARIABLE", "Test");

    run_client_command_with_env(shared, args, envs)
}

/// Run the status command without the path being included in the output.
pub async fn run_status_without_path(shared: &Shared, args: &[&str]) -> Result<Output> {
    // Inject an environment variable into the pueue command.
    // This is used to ensure that the environment is properly captured and forwarded.
    let mut envs = HashMap::new();
    envs.insert("PUEUED_TEST_ENV_VARIABLE", "Test");

    let state = get_state(shared).await?;
    println!("{state:?}");
    let mut base_args = vec!["status"];

    // Since we want to exclude the path, we have to manually assemble the
    // list of columns that should be displayed.
    // We start with the base columns, check which optional columns should be
    // included based on the current task list and add any of those columns at
    // the correct position.
    let mut columns = vec!["id,status"];

    // Add the enqueue_at column if necessary.
    if state.tasks.iter().any(|(_, task)| {
        if let TaskStatus::Stashed { enqueue_at } = task.status {
            return enqueue_at.is_some();
        }
        false
    }) {
        columns.push("enqueue_at");
    }

    // Add the `deps` column if necessary.
    if state
        .tasks
        .iter()
        .any(|(_, task)| !task.dependencies.is_empty())
    {
        columns.push("dependencies");
    }

    // Add the `label` column if necessary.
    if state.tasks.iter().any(|(_, task)| task.label.is_some()) {
        columns.push("label");
    }

    // Add the remaining base columns.
    columns.extend_from_slice(&["command", "start", "end"]);

    let column_filter = format!("columns={}", columns.join(","));
    base_args.push(&column_filter);

    base_args.extend_from_slice(args);
    run_client_command_with_env(shared, &base_args, envs)
}

/// Spawn a client command that connects to a specific daemon.
/// Accepts a list of environment variables that'll be injected into the client's env.
pub fn run_client_command_with_env(
    shared: &Shared,
    args: &[&str],
    envs: HashMap<&str, &str>,
) -> Result<Output> {
    let output = Command::cargo_bin("pueue")?
        .arg("--config")
        .arg(shared.pueue_directory().join("pueue.yml").to_str().unwrap())
        .args(args)
        .envs(envs)
        .current_dir(shared.pueue_directory())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context(format!("Failed to execute pueue with {args:?}"))?;

    Ok(output)
}
