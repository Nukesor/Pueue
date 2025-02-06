use crate::prelude::*;

use std::collections::HashMap;

use assert_matches::assert_matches;

use pueue_lib::task::{Task, TaskResult, TaskStatus};

use crate::client::helper::*;

/// Test that restarting a task while editing its command work as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_and_edit_task_command() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a task and wait for it to finish.
    assert_success(add_task(shared, "ls").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Set the editor to a command which replaces the temporary file's content.
    let mut envs = HashMap::new();
    envs.insert(
        "EDITOR",
        "echo 'sleep 60' > ${PUEUE_EDIT_PATH}/0/command ||",
    );

    // Restart the command, edit its command and wait for it to start.
    run_client_command_with_env(shared, &["restart", "--in-place", "--edit", "0"], envs)?;
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    // Make sure that both the command has been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.command, "sleep 60");
    assert_matches!(
        task.status,
        TaskStatus::Running { .. },
        "Task should be running"
    );

    Ok(())
}

/// Test that restarting a task while editing its path work as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_and_edit_task_path() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a task and wait for it to finish.
    assert_success(add_task(shared, "ls").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Set the editor to a command which replaces the temporary file's content.
    let mut envs = HashMap::new();
    envs.insert("EDITOR", "echo '/tmp' > ${PUEUE_EDIT_PATH}/0/path ||");

    // Restart the command, edit its command and wait for it to start.
    run_client_command_with_env(shared, &["restart", "--in-place", "--edit", "0"], envs)?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Make sure that both the path has been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.path.to_string_lossy(), "/tmp");

    Ok(())
}

/// Test that restarting a task while editing both, its command and its path, work as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_and_edit_task_path_and_command() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a task and wait for it to finish.
    assert_success(add_task(shared, "ls").await.unwrap());
    wait_for_task_condition(shared, 0, Task::is_done)
        .await
        .unwrap();

    // Set the editor to a command which replaces the temporary file's content.
    let mut envs = HashMap::new();
    envs.insert(
        "EDITOR",
        "echo 'doesnotexist' > ${PUEUE_EDIT_PATH}/0/command && \
echo '/tmp' > ${PUEUE_EDIT_PATH}/0/path && \
echo 'label' > ${PUEUE_EDIT_PATH}/0/label && \
echo '5' > ${PUEUE_EDIT_PATH}/0/priority || ",
    );

    // Restart the command, edit its command and path and wait for it to start.
    // The task will fail afterwards, but it should still be edited.
    run_client_command_with_env(shared, &["restart", "--in-place", "--edit", "0"], envs)?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Make sure that both the path has been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.command, "doesnotexist");
    assert_eq!(task.path.to_string_lossy(), "/tmp");
    assert_eq!(task.label, Some("label".to_string()));
    assert_eq!(task.priority, 5);

    // Also the task should have been restarted and failed.
    assert_matches!(
        task.status,
        TaskStatus::Done {
            result: TaskResult::Failed(127),
            ..
        },
        "The task should have failed"
    );

    Ok(())
}

/// Test that restarting a task while editing its priority works as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_and_edit_task_priority() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a task and wait for it to finish.
    assert_success(add_task(shared, "ls").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Set the editor to a command which replaces the temporary file's content.
    let mut envs = HashMap::new();
    envs.insert("EDITOR", "echo '99' > ${PUEUE_EDIT_PATH}/0/priority ||");

    // Restart the command, edit its priority and wait for it to start.
    run_client_command_with_env(shared, &["restart", "--in-place", "--edit", "0"], envs)?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Make sure that the priority has been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.priority, 99);

    Ok(())
}

/// Test that restarting a task **not** in place works as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_restart_with_edit() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a task and wait for it to finish.
    assert_success(add_task(shared, "ls").await?);
    let original_task = wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Set the editor to a command which replaces the temporary file's content.
    let mut envs = HashMap::new();
    envs.insert(
        "EDITOR",
        "echo 'sleep 60' > ${PUEUE_EDIT_PATH}/0/command ||",
    );

    // Restart the command, edit its command and wait for it to start.
    run_client_command_with_env(shared, &["restart", "--edit", "0"], envs)?;
    wait_for_task_condition(shared, 1, Task::is_running).await?;

    // Make sure that both the command has been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&1).unwrap();
    assert_eq!(task.command, "sleep 60");
    assert_matches!(
        task.status,
        TaskStatus::Running { .. },
        "Task should be running"
    );

    // Since we created a copy, the new task should be created after the first one.
    assert!(
        original_task.created_at < task.created_at,
        "New task should have a newer created_at."
    );

    Ok(())
}
