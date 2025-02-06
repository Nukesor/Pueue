use crate::prelude::*;

use pueue_lib::{
    network::message::{KillMessage, TaskSelection},
    task::*,
};

use crate::helper::*;

/// Test if adding a normal task works as intended.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_dependency() -> Result<()> {
    let (daemon, lockfile) = daemon_with_lockfile().await?;
    let shared = &daemon.settings.shared;

    // Add a task that waits until the lockfile is removed. It's added to a non-default group.
    add_group_with_slots(shared, "testgroup_3", 3).await?;
    assert_success(add_task_to_group(shared, lockfile_command(&lockfile), "testgroup_3").await?);

    // This task now has to wait for task 0, even though it's added to the default group and could
    // start right away.
    assert_success(add_task_with_dependencies(shared, "ls", vec![0]).await?);

    // Wait for a bit, the second task should still be queued.
    sleep_ms(500).await;

    // Clear the lock, the first task should now finish.
    clear_lock(&lockfile)?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // The second one should start and finish right away.
    wait_for_task_condition(shared, 1, Task::is_done).await?;

    Ok(())
}

/// Test if adding a normal task works as intended.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_failing_dependency() -> Result<()> {
    let (daemon, lockfile) = daemon_with_lockfile().await?;
    let shared = &daemon.settings.shared;

    // Add a task that waits until the lockfile is removed. It's added to a non-default group.
    add_group_with_slots(shared, "testgroup_3", 3).await?;
    assert_success(add_task_to_group(shared, lockfile_command(&lockfile), "testgroup_3").await?);

    // This task now has to wait for task 0, even though it's added to the default group and could
    // start right away.
    assert_success(add_task_with_dependencies(shared, "ls", vec![0]).await?);
    // Wait for a bit, the second task should still be queued.
    sleep_ms(500).await;

    // Now we kill the first task.
    // This should result in the second task failing.
    send_message(
        shared,
        KillMessage {
            tasks: TaskSelection::TaskIds(vec![0]),
            signal: None,
        },
    )
    .await?;
    wait_for_task_condition(shared, 0, Task::failed).await?;

    // Now wait until the first task finishes and make sure it failed because of a failing
    // dependency.
    let task = wait_for_task_condition(shared, 1, Task::failed).await?;
    assert!(matches!(
        task.status,
        TaskStatus::Done {
            result: TaskResult::DependencyFailed,
            ..
        }
    ));

    Ok(())
}
