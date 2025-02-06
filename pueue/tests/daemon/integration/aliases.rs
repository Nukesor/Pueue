use crate::prelude::*;

use std::collections::HashMap;

use assert_matches::assert_matches;

use pueue_lib::{network::message::*, task::*};

use crate::helper::*;

/// Test that using aliases when adding task normally works.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_add_with_alias() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    let mut aliases = HashMap::new();
    aliases.insert("non_existing_cmd".into(), "echo".into());
    create_test_alias_file(daemon.tempdir.path(), aliases)?;

    // Add a task whose command should be replaced by an alias
    assert_success(add_task(shared, "non_existing_cmd test").await?);

    // Wait until the task finished and get state
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let task = get_task(shared, 0).await?;

    // The task finished successfully and its command has replaced the alias.
    assert!(
        matches!(
            task.status,
            TaskStatus::Done {
                result: TaskResult::Success,
                ..
            },
        ),
        "Task should have finished successfully"
    );
    assert_eq!(task.command, "echo test");
    assert_eq!(task.original_command, "non_existing_cmd test");

    // Make sure we see an actual "test" in the output.
    // This ensures that we really called "echo".
    let log = get_task_log(shared, 0, None).await?;
    assert_eq!(log, "test\n");

    Ok(())
}

/// Test that aliases are applied when a task's command is changed on restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_restart_with_alias() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a task whose command that should fail and wait for it to finish.
    assert_success(add_task(shared, "non_existing_cmd test").await?);
    let task = wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Ensure the command hasn't been mutated and the task failed.
    assert_eq!(task.command, "non_existing_cmd test");
    assert_matches!(
        task.status,
        TaskStatus::Done {
            result: TaskResult::Failed(127),
            ..
        },
        "Task should have failed to start"
    );

    // Create the alias file which will replace the new command with "echo".
    let mut aliases = HashMap::new();
    aliases.insert("replaced_cmd".into(), "echo".into());
    create_test_alias_file(daemon.tempdir.path(), aliases)?;

    // Restart the task while editing its command.
    let message = RestartMessage {
        tasks: vec![TaskToRestart {
            task_id: 0,
            command: "replaced_cmd test".to_string(),
            path: task.path,
            label: task.label,
            priority: task.priority,
        }],
        start_immediately: true,
        stashed: false,
    };
    send_message(shared, message).await?;
    let task = wait_for_task_condition(shared, 0, Task::is_done).await?;

    // The task finished successfully and its command has replaced the alias.
    assert_eq!(task.original_command, "replaced_cmd test");
    assert_eq!(task.command, "echo test");
    assert_matches!(
        task.status,
        TaskStatus::Done {
            result: TaskResult::Success,
            ..
        },
        "Task should have finished successfully"
    );

    // Make sure we see an actual "test" in the output.
    // This ensures that we really called "echo".
    let log = get_task_log(shared, 0, None).await?;
    assert_eq!(log, "test\n");

    Ok(())
}
