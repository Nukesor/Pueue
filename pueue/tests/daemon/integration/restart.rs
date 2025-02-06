use crate::prelude::*;

use std::path::PathBuf;

use pueue_lib::{network::message::*, task::Task};

use crate::helper::*;

/// Ensure that restarting a task in-place, resets it's state and possibly updates the command and
/// path to the new values.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_restart_in_place() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a single task that instantly finishes.
    assert_success(add_and_start_task(shared, "ls").await?);

    // Wait for task 0 to finish.
    let original_task = wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Restart task 0 with an sleep command and with a different path.
    let restart_message = RestartMessage {
        tasks: vec![TaskToRestart {
            task_id: 0,
            command: "sleep 60".to_string(),
            path: PathBuf::from("/tmp"),
            label: Some("test".to_string()),
            priority: 0,
        }],
        start_immediately: false,
        stashed: false,
    };
    assert_success(send_message(shared, restart_message).await?);

    let state = get_state(shared).await?;
    assert_eq!(state.tasks.len(), 1, "No new task should be created");

    // Task 0 should soon be started again
    let task = wait_for_task_condition(shared, 0, Task::is_running).await?;

    // The created_at time should be the same, as we updated in place
    assert_eq!(
        original_task.created_at, task.created_at,
        "created_at shouldn't change on 'restart -i'"
    );

    // Make sure both command and path were changed
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.command, "sleep 60");
    assert_eq!(task.path, PathBuf::from("/tmp"));
    assert_eq!(task.label, Some("test".to_owned()));

    Ok(())
}

/// Ensure that running task cannot be restarted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cannot_restart_running() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a long running task that starts immediately.
    assert_success(add_and_start_task(shared, "sleep 60").await?);
    let task = get_task(shared, 1).await?;

    // Try to restart task 0
    let restart_message = RestartMessage {
        tasks: vec![TaskToRestart {
            task_id: 0,
            command: task.command,
            path: task.path,
            label: task.label,
            priority: task.priority,
        }],
        start_immediately: false,
        stashed: false,
    };
    assert_failure(send_message(shared, restart_message).await?);

    Ok(())
}
