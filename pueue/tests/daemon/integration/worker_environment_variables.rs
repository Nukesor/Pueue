use crate::prelude::*;

use pueue_lib::{network::message::TaskSelection, state::PUEUE_DEFAULT_GROUP, task::Task};

use crate::helper::*;

/// Make sure that the expected worker variables are injected into the tasks' environment variables
/// for a single task on the default queue.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_single_worker() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add some tasks that finish instantly.
    for _ in 0..3 {
        assert_success(add_env_task(shared, "sleep 0.1").await?);
    }
    // Wait a second. Since the tasks are run sequentially, the timings are sometimes a bit tight.
    sleep_ms(1000).await;

    // Wait for the last task to finish.
    wait_for_task_condition(shared, 2, Task::is_done).await?;

    // All tasks should have the worker id 0, as the tasks are processed sequentially.
    let state = get_state(shared).await?;
    for task_id in 0..3 {
        assert_worker_envs(shared, &state, task_id, 0, PUEUE_DEFAULT_GROUP).await?;
    }

    Ok(())
}

/// Make sure the correct workers are used when having multiple slots.
///
/// Slots should be properly freed and re-used.
/// Add some tasks to a group with three slots:
///
/// Task0-2 are started in parallel.
/// Task3-4 are started in parallel once Task0-2 finished.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multiple_worker() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Pause the group before adding the tasks.
    // Adding tasks takes a while and the first task might already be finished
    // when we add the last one.
    pause_tasks(shared, TaskSelection::Group("test_3".to_string())).await?;

    // Add three tasks. They will be started in the same main loop iteration
    // and run in parallel.
    for _ in 0..3 {
        assert_success(add_env_task_to_group(shared, "sleep 0.1", "test_3").await?);
    }

    // Start and wait for the tasks
    start_tasks(shared, TaskSelection::Group("test_3".to_string())).await?;
    wait_for_task_condition(shared, 2, Task::is_done).await?;

    // The first three tasks should have the same worker id's as the task ids.
    // They ran in parallel and each should have their own worker id assigned.
    let state = get_state(shared).await?;
    for task_id in 0..3 {
        assert_worker_envs(shared, &state, task_id, task_id, "test_3").await?;
    }

    // Spawn two more tasks and wait for them.
    // They should now get worker0 and worker1, as there aren't any other running tasks.
    pause_tasks(shared, TaskSelection::Group("test_3".to_string())).await?;
    for _ in 0..2 {
        assert_success(add_env_task_to_group(shared, "sleep 0.1", "test_3").await?);
    }
    start_tasks(shared, TaskSelection::Group("test_3".to_string())).await?;
    wait_for_task_condition(shared, 4, Task::is_done).await?;

    let state = get_state(shared).await?;
    // Task3 gets worker0
    assert_worker_envs(shared, &state, 3, 0, "test_3").await?;
    // Task4 gets worker1
    assert_worker_envs(shared, &state, 4, 1, "test_3").await?;

    Ok(())
}

/// Make sure the worker pools are properly initialized when manually adding a new group.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_worker_for_new_pool() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a new group
    add_group_with_slots(shared, "testgroup", 1).await?;

    // Add a tasks that finishes instantly.
    assert_success(add_env_task_to_group(shared, "sleep 0.1", "testgroup").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // The task should have the correct worker id + group.
    let state = get_state(shared).await?;
    assert_worker_envs(shared, &state, 0, 0, "testgroup").await?;

    Ok(())
}
