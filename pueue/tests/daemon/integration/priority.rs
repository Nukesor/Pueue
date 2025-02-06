use crate::prelude::*;

use rstest::rstest;

use pueue_lib::{network::message::TaskSelection, task::Task};

use crate::helper::*;

/// For tasks with the same priority, lowest ids are started first.
#[rstest]
#[case(0)]
#[case(-1)]
#[case(1)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_default_ordering(#[case] priority: i32) -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Pause the daemon and prevent tasks to be automatically spawned.
    pause_tasks(shared, TaskSelection::All).await?;

    // Add two tasks with default priority.
    assert_success(add_task_with_priority(shared, "sleep 10", priority).await?);
    assert_success(add_task_with_priority(shared, "sleep 10", priority).await?);

    // Resume the daemon.
    start_tasks(shared, TaskSelection::All).await?;

    // Make sure task 0 is being started and task 1 is still waiting.
    wait_for_task_condition(shared, 0, Task::is_running).await?;
    wait_for_task_condition(shared, 1, Task::is_queued).await?;

    Ok(())
}

/// Tasks with a higher priority should be executed before tasks with a lower priority.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_highest_priority_first() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Pause the daemon and prevent tasks to be automatically spawned.
    pause_tasks(shared, TaskSelection::All).await?;

    // Add one normal task and one with the lowest possible priority.
    assert_success(add_task(shared, "sleep 10").await?);
    assert_success(add_task_with_priority(shared, "sleep 10", 1).await?);
    assert_success(add_task_with_priority(shared, "sleep 10", 2).await?);

    // Resume the daemon.
    start_tasks(shared, TaskSelection::All).await?;

    // Make sure task 0 is being started and task 1 is still waiting.
    wait_for_task_condition(shared, 2, Task::is_running).await?;
    wait_for_task_condition(shared, 1, Task::is_queued).await?;
    wait_for_task_condition(shared, 0, Task::is_queued).await?;

    Ok(())
}

/// Tasks with a negative priority should be executed before tasks with default priority.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_default_priority_over_negative_priority() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Pause the daemon and prevent tasks to be automatically spawned.
    pause_tasks(shared, TaskSelection::All).await?;

    // Add one normal task and one with the lowest possible priority.
    assert_success(add_task_with_priority(shared, "sleep 10", -2).await?);
    assert_success(add_task_with_priority(shared, "sleep 10", -1).await?);
    assert_success(add_task(shared, "sleep 10").await?);

    // Resume the daemon.
    start_tasks(shared, TaskSelection::All).await?;

    // Make sure task 0 is being started and task 1 is still waiting.
    wait_for_task_condition(shared, 2, Task::is_running).await?;
    wait_for_task_condition(shared, 0, Task::is_queued).await?;
    wait_for_task_condition(shared, 1, Task::is_queued).await?;

    Ok(())
}
