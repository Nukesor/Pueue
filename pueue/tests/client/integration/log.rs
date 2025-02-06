use crate::prelude::*;

use std::collections::{BTreeMap, HashMap};

use rstest::rstest;
use serde::Deserialize;

use pueue_lib::task::Task;

use crate::client::helper::*;

/// Test that the `log` command works for both:
/// - The log being streamed by the daemon.
/// - The log being read from the local files.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Force the client to read remote logs via config file.
    daemon.settings.client.read_local_logs = read_local_logs;
    // Persist the change, so it can be seen by the client.
    daemon
        .settings
        .save(&Some(daemon.tempdir.path().join("pueue.yml")))
        .context("Couldn't write pueue config to temporary directory")?;

    // Add a task and wait until it finishes.
    assert_success(add_task(shared, "echo test").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["log"])?;

    let context = get_task_context(&daemon.settings).await?;
    assert_template_matches("log__default", output, context)?;

    Ok(())
}

/// Test that the `log` command properly truncates content and hints this to the user for:
/// - The log being streamed by the daemon.
/// - The log being read from the local files.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_truncated(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Force the client to read remote logs via config file.
    daemon.settings.client.read_local_logs = read_local_logs;
    // Persist the change, so it can be seen by the client.
    daemon
        .settings
        .save(&Some(daemon.tempdir.path().join("pueue.yml")))
        .context("Couldn't write pueue config to temporary directory")?;

    // Add a task and wait until it finishes.
    assert_success(add_task(shared, "echo '1\n2\n3\n4\n5\n6\n7\n8\n9\n10'").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["log", "--lines=5"])?;

    let context = get_task_context(&daemon.settings).await?;
    assert_template_matches("log__last_lines", output, context)?;

    Ok(())
}

/// If a task has a label, it is included in the log output
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_with_label() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a task and wait until it finishes.
    run_client_command(shared, &["add", "--label", "test_label", "echo test"])?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["log"])?;

    let context = get_task_context(&daemon.settings).await?;
    assert_template_matches("log__with_label", output, context)?;

    Ok(())
}

/// Calling `log` with the `--color=always` flag, colors the output as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn colored() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a task and wait until it finishes.
    assert_success(add_task(shared, "echo test").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["--color", "always", "log"])?;

    let context = get_task_context(&daemon.settings).await?;
    assert_template_matches("log__colored", output, context)?;

    Ok(())
}

/// This is the output struct used for task logs.
/// Since the Pueue client isn't exposed as a library, we have to declare our own for testing
/// purposes. The counter part can be found in `client/display/log/json.rs`.
#[derive(Debug, Deserialize)]
pub struct TaskLog {
    pub task: Task,
    pub output: String,
}

/// Calling `pueue log --json` prints the expected json output to stdout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn json() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a task and wait until it finishes.
    assert_success(add_task(shared, "echo test").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["log", "--json"])?;

    // Deserialize the json back to the original task BTreeMap.
    let json = String::from_utf8_lossy(&output.stdout);
    let mut task_logs: BTreeMap<usize, TaskLog> = serde_json::from_str(&json)
        .context(format!("Failed to deserialize json tasks: \n{json}"))?;

    // Get the actual BTreeMap from the daemon
    let mut state = get_state(shared).await?;
    let original_task = state.tasks.get_mut(&0).unwrap();
    // Clean the environment variables, as they aren't transmitted when calling `log`.
    original_task.envs = HashMap::new();

    let task_log = task_logs.get_mut(&0).expect("Expected one task log");
    assert_eq!(
        original_task, &task_log.task,
        "Deserialized task and original task aren't equal"
    );

    // Append a newline to the deserialized task's output, which is automatically done when working
    // with the shell.
    assert_eq!("test", task_log.output);

    Ok(())
}
