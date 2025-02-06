use crate::prelude::*;

use rstest::rstest;

use pueue_lib::task::Task;

use crate::client::helper::*;

pub fn set_read_local_logs(daemon: &mut PueueDaemon, read_local_logs: bool) -> Result<()> {
    // Force the client to read remote logs via config file.
    daemon.settings.client.read_local_logs = read_local_logs;
    // Persist the change, so it can be seen by the client.
    daemon
        .settings
        .save(&Some(daemon.tempdir.path().join("pueue.yml")))
        .context("Couldn't write pueue config to temporary directory")?;

    Ok(())
}

/// Test that the `follow` command works with the log being streamed locally and by the daemon.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    set_read_local_logs(&mut daemon, read_local_logs)?;
    let shared = &daemon.settings.shared;

    // Add a task and wait until it started.
    assert_success(add_task(shared, "sleep 1 && echo test").await?);
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    // Execute `follow`.
    // This will result in the client receiving the streamed output until the task finished.
    let output = run_client_command(shared, &["follow"])?;

    assert_snapshot_matches_output("follow__default", output.stdout)?;

    Ok(())
}

/// Test that a task is immediately followed when added with `--immediate --follow`.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_on_immediate_add(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    set_read_local_logs(&mut daemon, read_local_logs)?;
    let shared = &daemon.settings.shared;

    let output = run_client_command(
        shared,
        &["add", "--immediate", "--follow", "sleep 1 && echo test"],
    )?;

    assert_snapshot_matches_output("follow__default", output.stdout)?;

    Ok(())
}

/// Test that the remote `follow` command works, if one specifies to only show the last few lines
/// of recent output.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn last_lines(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    set_read_local_logs(&mut daemon, read_local_logs)?;
    let shared = &daemon.settings.shared;

    // Add a task which echos 8 lines of output
    assert_success(add_task(shared, "echo \"1\n2\n3\n4\n5\n6\n7\n8\" && sleep 1").await?);
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    // Follow the task, but only print the last 4 lines of the output.
    let output = run_client_command(shared, &["follow", "--lines=4"])?;

    assert_snapshot_matches_output("follow__last_lines", output.stdout)?;

    Ok(())
}

/// If a task exists but hasn't started yet, wait for it to start.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wait_for_task(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    set_read_local_logs(&mut daemon, read_local_logs)?;
    let shared = &daemon.settings.shared;

    // Add a normal task that will start in 2 seconds.
    run_client_command(shared, &["add", "--delay", "2 seconds", "echo test"])?;

    // Wait for the task to start and follow until it finisheds.
    let output = run_client_command(shared, &["follow", "0"])?;

    assert_snapshot_matches_output("follow__default", output.stdout)?;

    Ok(())
}

/// Fail when following a non-existing task
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fail_on_non_existing(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    set_read_local_logs(&mut daemon, read_local_logs)?;
    let shared = &daemon.settings.shared;

    // Execute `follow` on a non-existing task.
    // The client should exit with exit code `1`.
    let output = run_client_command(shared, &["follow", "0"])?;
    assert!(!output.status.success(), "follow got an unexpected exit 0");
    assert_snapshot_matches_output("follow__fail_on_non_existing", output.stderr)?;

    Ok(())
}

// /// This test is commented for the time being.
// /// There's a race condition that can happen from time to time.
// /// It's especially reliably hit on MacOS for some reason.
// ///
// /// What happens is that the daemon resets in between reading the output of the file
// /// and the check whether the task actually still exists in the daemon.
// /// There's really no way to properly work around this.
// /// So I'll keep this commented for the time being.
// ///
// ///
// /// Fail and print an error message when following a non-existing task disappears
// #[rstest]
// #[case(true)]
// #[case(false)]
// #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// async fn fail_on_disappearing(#[case] read_local_logs: bool) -> Result<()> {
//     let mut daemon = daemon().await?;
//     set_read_local_logs(&mut daemon, read_local_logs)?;
//     let shared = &daemon.settings.shared;
//
//     // Add a task echoes something and waits for a while
//     assert_success(add_task(shared, "echo test && sleep 20").await?);
//     wait_for_task_condition(shared, 0, Task::is_running).await?;
//
//     // Reset the daemon after 2 seconds. At this point, the client will already be following the
//     // output and should notice that the task went away..
//     // This is a bit hacky, but our client test helper always waits for the command to finish
//     // and I'm feeling too lazy to add a new helper function now.
//     let moved_shared = shared.clone();
//     tokio::task::spawn(async move {
//         sleep_ms(2000).await;
//         // Reset the daemon
//         send_message(&moved_shared, ResetMessage {})
//             .await
//             .expect("Failed to send Start tasks message");
//     });
//
//     // Execute `follow` and remove the task
//     // The client should exit with exit code `1`.
//     let output = run_client_command(shared, &["follow", "0"])?;
//
//     assert_snapshot_matches_stdout("follow__fail_on_disappearing", output.stdout)?;
//
//     Ok(())
// }
