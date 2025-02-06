use crate::prelude::*;

use std::{
    process::Output,
    thread::{self, JoinHandle},
};

use tokio::time::sleep;

use crate::client::helper::*;
use pueue_lib::settings::Shared;

/// All lines have the following pattern:
/// 01:49:42 - New task 1 with status Queued
///
/// The following code trims all timestamps from the log output.
/// We cannot work with proper timings, as these times are determined by the client.
/// They are unknown to us.
fn clean_wait_output(stdout: Vec<u8>) -> Vec<u8> {
    let log = String::from_utf8_lossy(&stdout);
    let mut log = log
        .lines()
        .map(|line| line.split('-').nth(1).unwrap().trim_start())
        .collect::<Vec<&str>>()
        .join("\n");
    log.push('\n');

    log.as_bytes().to_owned()
}

/// Spawn the `wait` subcommand in a separate thread.
/// We expect it to finish later on its own.
async fn spawn_wait_client(shared: &Shared, args: Vec<&'static str>) -> JoinHandle<Result<Output>> {
    let shared_clone = shared.clone();
    let wait_handle = thread::spawn(move || run_client_command(&shared_clone, &args));
    // Sleep for half a second to give `pueue wait` time to properly start.
    sleep(std::time::Duration::from_millis(500)).await;

    wait_handle
}

/// Test that `wait` will detect new commands and wait until all queued commands are done.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiple_tasks() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Run a command that'll run for a short time after a delay.
    // The `pueue wait` command will be spawne directly afterwards, resulting in the spawned
    // process to wait for this command to finish.
    run_client_command(shared, &["add", "--delay", "2 seconds", "sleep 1"])?;

    let wait_handle = spawn_wait_client(shared, vec!["wait"]).await;

    // We now spawn another task that should be picked up by and waited upon completion
    // by the `wait` process.
    run_client_command(shared, &["add", "--after=0", "sleep 1"])?;

    let output = wait_handle.join().unwrap()?;
    let stdout = clean_wait_output(output.stdout);

    assert_snapshot_matches_output("wait__multiple_tasks", stdout)?;

    Ok(())
}

/// Test that `wait` will correctly wait for the correct status on tasks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn target_status() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Run a command that'll run for a short time after a delay.
    run_client_command(shared, &["add", "--delay", "4 seconds", "sleep 20"])?;

    // wait for all tasks to be queued.
    // task0 will go from `Stashed` to `Queued` to `Running`.
    // task1 will go from `Stashed` to `Queued`.
    //
    // `Running` fulfills the `Queued` condition, which is why the `wait` command should
    // exit as soon as the second task is enqueued.
    let wait_handle = spawn_wait_client(shared, vec!["wait", "--status", "queued"]).await;

    // We now spawn another task.
    run_client_command(shared, &["add", "--delay", "1 seconds", "sleep 5"])?;

    let output = wait_handle.join().unwrap()?;
    let stdout = clean_wait_output(output.stdout);

    assert_snapshot_matches_output("wait__target_status", stdout)?;

    Ok(())
}

/// Test that `wait` will correctly wait for the correct status on tasks on a single task.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn single_task_target_status() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Run a command that'll run for a short time after a short delay.
    run_client_command(shared, &["add", "--delay", "2 seconds", "sleep 20"])?;

    // We now spawn another task that should be queued after a long time.
    // The wait command shouldn't wait for this one
    run_client_command(shared, &["add", "--delay", "20 seconds", "sleep 5"])?;

    // The wait should exit as soon as task0 changes to `Queued`.
    let wait_handle = spawn_wait_client(shared, vec!["wait", "0", "--status", "queued"]).await;

    let output = wait_handle.join().unwrap()?;
    let stdout = clean_wait_output(output.stdout);

    assert_snapshot_matches_output("wait__single_task_target_status", stdout)?;

    Ok(())
}

/// Test that `wait success` will correctly wait for successful tasks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn success_success() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Run a command that'll run for a short time after a delay.
    run_client_command(shared, &["add", "--delay", "1 seconds", "sleep 2"])?;

    let wait_handle = spawn_wait_client(shared, vec!["wait", "--status", "success"]).await;

    let output = wait_handle.join().unwrap()?;
    assert!(output.status.success(), "Got non-zero exit code on wait.");

    let stdout = clean_wait_output(output.stdout);
    assert_snapshot_matches_output("wait__success_success", stdout)?;

    Ok(())
}

/// Test that `wait success` will fail with exitcode 1 if a single task fails.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn success_failure() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Run two command the first will immediately fail, the second should (theoretically) succeed.
    run_client_command(
        shared,
        &["add", "--delay", "2 seconds", "sleep 2 && failing_command"],
    )?;
    run_client_command(shared, &["add", "--delay", "2 seconds", "sleep 2"])?;

    let wait_handle = spawn_wait_client(shared, vec!["wait", "--status", "success"]).await;

    let output = wait_handle.join().unwrap()?;
    assert!(
        !output.status.success(),
        "Got unexpected zero exit code on wait."
    );

    let stdout = clean_wait_output(output.stdout);
    assert_snapshot_matches_output("wait__success_failure", stdout)?;

    Ok(())
}
