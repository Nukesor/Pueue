use crate::prelude::*;

use chrono::{DateTime, Local, TimeDelta};
use rstest::rstest;

use pueue_lib::{network::message::*, state::GroupStatus, task::*};

use crate::helper::*;

/// Tasks can be stashed and scheduled for being enqueued at a specific point in time.
///
/// Furthermore these stashed tasks can then be manually enqueued again.
#[rstest]
#[case(None)]
#[case(Some(Local::now() + TimeDelta::try_minutes(2).unwrap()))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_enqueued_tasks(#[case] enqueue_at: Option<DateTime<Local>>) -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    assert_success(create_stashed_task(shared, "sleep 10", enqueue_at).await?);

    // The task should be added in stashed state.
    let task = wait_for_task_condition(shared, 0, |task| task.is_stashed()).await?;

    // Assert the correct point in time has been set, in case `enqueue_at` is specific.
    if enqueue_at.is_some() {
        let status = get_task_status(shared, 0).await?;
        assert!(task.is_stashed());

        if let TaskStatus::Stashed { enqueue_at: inner } = status {
            assert_eq!(inner, enqueue_at);
        }
    }

    // Manually enqueue the task
    let enqueue_message = EnqueueMessage {
        tasks: TaskSelection::TaskIds(vec![0]),
        enqueue_at: None,
    };
    send_message(shared, enqueue_message)
        .await
        .context("Failed to to add task message")?;

    // Make sure the task is started after being enqueued
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    Ok(())
}

/// Delayed stashed tasks will be enqueued.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delayed_tasks() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // The task will be stashed and automatically enqueued after about 1 second.
    let response = create_stashed_task(
        shared,
        "sleep 10",
        Some(Local::now() + TimeDelta::try_seconds(1).unwrap()),
    )
    .await?;
    assert_success(response);

    assert_task_condition(
        shared,
        0,
        Task::is_stashed,
        "Task should be stashed for about 1 second.",
    )
    .await?;

    // Make sure the task is started after being automatically enqueued.
    sleep_ms(800).await;
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    Ok(())
}

/// Stash a task that's currently queued for execution.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stash_queued_task() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Pause the daemon
    pause_tasks(shared, TaskSelection::All).await?;
    wait_for_group_status(shared, "default", GroupStatus::Paused).await?;

    // Add a task that's queued for execution.
    add_task(shared, "sleep 10").await?;

    // Stash the task
    send_message(
        shared,
        StashMessage {
            tasks: TaskSelection::TaskIds(vec![0]),
            enqueue_at: None,
        },
    )
    .await
    .context("Failed to send Stash message")?;

    assert_task_condition(shared, 0, Task::is_stashed, "Task has just been stashed.").await?;

    Ok(())
}
