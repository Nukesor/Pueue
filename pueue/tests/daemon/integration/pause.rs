use crate::prelude::*;

use assert_matches::assert_matches;

use pueue_lib::network::message::*;
use pueue_lib::state::GroupStatus;
use pueue_lib::task::*;

use crate::helper::*;

/// Make sure that no tasks will be started in a paused queue
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pause_daemon() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // This pauses the daemon
    pause_tasks(shared, TaskSelection::All).await?;
    // Make sure the default group get's paused
    wait_for_group_status(shared, PUEUE_DEFAULT_GROUP, GroupStatus::Paused).await?;

    // Add a task and give the taskmanager time to theoretically start the process
    add_task(shared, "ls").await?;
    sleep_ms(500).await;

    // Make sure it's not started
    assert_matches!(
        get_task_status(shared, 0).await?,
        TaskStatus::Queued { .. },
        "Task should not be started yet."
    );

    Ok(())
}

/// Make sure that running tasks will be properly paused
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pause_running_task() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Start a long running task and make sure it's started
    add_task(shared, "sleep 60").await?;
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    // This pauses the daemon
    pause_tasks(shared, TaskSelection::All).await?;

    // Make sure the task as well as the default group get paused
    assert_group_status(
        shared,
        PUEUE_DEFAULT_GROUP,
        GroupStatus::Paused,
        "Default group should be paused.",
    )
    .await?;
    assert_task_condition(
        shared,
        0,
        Task::is_paused,
        "All default groups should be paused.",
    )
    .await?;

    Ok(())
}

/// A queue can get paused, while the tasks may finish on their own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pause_with_wait() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Start a long running task and make sure it's started
    add_task(shared, "sleep 60").await?;
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    // Pauses the default queue while waiting for tasks
    let message = PauseMessage {
        tasks: TaskSelection::Group(PUEUE_DEFAULT_GROUP.into()),
        wait: true,
    };
    send_message(shared, message)
        .await
        .context("Failed to send message")?;

    // Make sure the default group gets paused, but the task is still running
    assert_group_status(
        shared,
        PUEUE_DEFAULT_GROUP,
        GroupStatus::Paused,
        "Default group should be paused.",
    )
    .await?;
    assert_task_condition(
        shared,
        0,
        Task::is_running,
        "Task should continue running after group is paused.",
    )
    .await?;

    Ok(())
}
