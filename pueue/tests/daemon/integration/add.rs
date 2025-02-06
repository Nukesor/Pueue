use crate::prelude::*;

use assert_matches::assert_matches;
use chrono::Local;

use pueue_lib::{network::message::TaskSelection, task::*};

use crate::helper::*;

/// Test if adding a normal task works as intended.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_normal_add() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    let pre_addition_time = Local::now();

    // Add a task that instantly finishes
    assert_success(add_task(shared, "sleep 0.01").await?);

    // Wait until the task finished and get state
    let task = wait_for_task_condition(shared, 0, Task::is_done).await?;

    let post_addition_time = Local::now();

    // Make sure the task's created_at and enqueue_at times are viable.
    assert!(
        task.created_at > pre_addition_time && task.created_at < post_addition_time,
        "Make sure the created_at time is set correctly"
    );

    assert_matches!(
        get_task_status(shared, 0).await?,
        TaskStatus::Done {
            result: TaskResult::Success,
            ..
        },
        "Task should finish successfully",
    );

    Ok(())
}

/// Test if adding a task in stashed state work.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stashed_add() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Tell the daemon to add a task in stashed state.
    let mut message = create_add_message(shared, "sleep 60");
    message.stashed = true;
    assert_success(send_message(shared, message).await?);

    // Make sure the task is actually stashed.
    assert_task_condition(shared, 0, Task::is_stashed, "The task should be stashed.").await?;

    Ok(())
}

/// Pause the default group and make sure that immediately spawning a task still works.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_add_with_immediate_start() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Pause the daemon and prevent tasks to be automatically spawned.
    pause_tasks(shared, TaskSelection::All).await?;

    // Tell the daemon to add a task that must be immediately started.
    assert_success(add_and_start_task(shared, "sleep 60").await?);

    // Make sure the task is actually being started.
    assert_task_condition(
        shared,
        0,
        Task::is_running,
        "Tasks should start immediately",
    )
    .await?;

    Ok(())
}
