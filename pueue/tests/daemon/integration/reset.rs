use crate::prelude::*;

use pueue_lib::{network::message::*, state::GroupStatus, task::Task};

use crate::helper::*;

/// A reset command kills all tasks and forces a clean state accross groups.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_reset() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Start a long running task and make sure it's started
    add_task(shared, "ls").await?;
    add_task(shared, "failed").await?;
    add_task_to_group(shared, "sleep 60", "test_2").await?;
    add_task(shared, "ls").await?;
    wait_for_task_condition(shared, 2, Task::is_running).await?;

    // Reset all groups of the daemon
    send_message(
        shared,
        ResetMessage {
            target: ResetTarget::All,
        },
    )
    .await
    .context("Failed to send Start tasks message")?;

    // Resetting is asynchronous, wait for all task to disappear.
    wait_for_task_absence(shared, 0).await?;
    wait_for_task_absence(shared, 1).await?;
    wait_for_task_absence(shared, 2).await?;
    wait_for_task_absence(shared, 3).await?;

    // All tasks should have been removed.
    let state = get_state(shared).await?;
    assert!(state.tasks.is_empty(),);

    // Both groups should be running.
    assert_eq!(
        state.groups.get("default").unwrap().status,
        GroupStatus::Running
    );
    assert_eq!(
        state.groups.get("test_2").unwrap().status,
        GroupStatus::Running
    );

    Ok(())
}

/// A reset command kills all tasks and forces a clean state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_reset_single_group() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Start a long running task and make sure it's started
    add_task(shared, "ls").await?;
    add_task(shared, "failed").await?;
    add_task_to_group(shared, "sleep 60", "test_2").await?;
    add_task_to_group(shared, "sleep 60", "test_3").await?;
    wait_for_task_condition(shared, 2, Task::is_running).await?;

    // Reset only the test_2 of the daemon.
    send_message(
        shared,
        ResetMessage {
            target: ResetTarget::Groups(vec!["test_2".to_string()]),
        },
    )
    .await
    .context("Failed to send Start tasks message")?;

    // Resetting is asynchronous, wait for the third task to disappear.
    wait_for_task_absence(shared, 2).await?;

    // All tasks should have been removed.
    let state = get_state(shared).await?;
    assert_eq!(
        state.tasks.len(),
        3,
        "Only a single task should have been removed"
    );

    assert_eq!(
        state.groups.get("test_2").unwrap().status,
        GroupStatus::Running
    );

    Ok(())
}

/// A reset command kills all tasks and forces a clean state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_reset_multiple_groups() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Start a long running task and make sure it's started
    add_task(shared, "ls").await?;
    add_task(shared, "failed").await?;
    add_task_to_group(shared, "sleep 60", "test_2").await?;
    add_task_to_group(shared, "sleep 60", "test_3").await?;
    wait_for_task_condition(shared, 2, Task::is_running).await?;

    // Reset only the test_2 of the daemon.
    send_message(
        shared,
        ResetMessage {
            target: ResetTarget::Groups(vec!["test_2".to_string(), "test_3".to_string()]),
        },
    )
    .await
    .context("Failed to send Start tasks message")?;

    // Resetting is asynchronous, wait for the third task to disappear.
    wait_for_task_absence(shared, 2).await?;
    wait_for_task_absence(shared, 3).await?;

    // All tasks should have been removed.
    let state = get_state(shared).await?;
    assert_eq!(
        state.tasks.len(),
        2,
        "Only a two task should have been removed"
    );

    assert_eq!(
        state.groups.get("test_2").unwrap().status,
        GroupStatus::Running
    );
    assert_eq!(
        state.groups.get("test_3").unwrap().status,
        GroupStatus::Running
    );

    Ok(())
}
