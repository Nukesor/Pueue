use crate::prelude::*;

use assert_matches::assert_matches;

use pueue_lib::{network::message::ParallelMessage, task::*};

use crate::helper::*;

/// Test that multiple groups with multiple slots work.
///
/// For each group, Pueue should start tasks until all slots are filled.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_parallel_tasks() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // ---- First group ----
    // Add a new group with 3 slots
    add_group_with_slots(shared, "testgroup_3", 3).await?;

    // Add 5 tasks to this group, only 3 should be started.
    for _ in 0..5 {
        assert_success(add_task_to_group(shared, "sleep 60", "testgroup_3").await?);
    }

    // Ensure those three tasks are started.
    for task_id in 0..3 {
        wait_for_task_condition(shared, task_id, Task::is_running).await?;
    }

    // Tasks 4-5 should still be queued
    let state = get_state(shared).await?;
    for task_id in 3..5 {
        let task = state.tasks.get(&task_id).unwrap();
        assert_matches!(
            task.status,
            TaskStatus::Queued { .. },
            "Task {task_id} should be queued"
        );
    }

    // ---- Second group ----
    // Add another group with 2 slots
    add_group_with_slots(shared, "testgroup_2", 2).await?;

    // Add another 5 tasks to this group, only 2 should be started.
    for _ in 0..5 {
        assert_success(add_task_to_group(shared, "sleep 60", "testgroup_2").await?);
    }

    // Ensure only two tasks are started.
    for task_id in 5..7 {
        wait_for_task_condition(shared, task_id, Task::is_running).await?;
    }

    // Tasks 8-10 should still be queued
    let state = get_state(shared).await?;
    for task_id in 7..10 {
        let task = state.tasks.get(&task_id).unwrap();
        assert_matches!(
            task.status,
            TaskStatus::Queued { .. },
            "Task {task_id} should be queued in second check"
        );
    }
    Ok(())
}

/// Test that a group with a parallel limit of `0` has an unlimited amount of tasks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unlimited_parallel_tasks() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a new group with 1 slot
    add_group_with_slots(shared, "testgroup", 1).await?;

    // Add 10 long running tasks to this group, only 1 should be immediately started.
    for _ in 0..10 {
        assert_success(add_task_to_group(shared, "sleep 600", "testgroup").await?);
    }
    // Ensure the first tasks is started.
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    // Update the parallel limit of the group to 0
    let message = ParallelMessage {
        group: "testgroup".to_string(),
        parallel_tasks: 0,
    };
    assert_success(send_message(shared, message).await?);

    // Make sure all other tasks are started as well in quick succession.
    for task_id in 1..10 {
        wait_for_task_condition(shared, task_id, Task::is_running).await?;
    }

    Ok(())
}
