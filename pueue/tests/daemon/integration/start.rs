use crate::prelude::*;

use rstest::rstest;

use pueue_lib::{network::message::*, task::*};

use crate::helper::*;

/// Test if explicitly starting tasks and resuming tasks works as intended.
///
/// We test different ways of resumes tasks.
/// - Via the --all flag, which resumes everything.
/// - Via the --group flag, which resumes everything in a specific group (in our case 'default').
/// - Via specific ids.
#[rstest]
#[case(
    StartMessage {
        tasks: TaskSelection::All,
    }
)]
#[case(
    StartMessage {
        tasks: TaskSelection::Group(PUEUE_DEFAULT_GROUP.into()),
    }
)]
#[case(
    StartMessage {
        tasks: TaskSelection::TaskIds(vec![0, 1, 2]),
    }
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_start_tasks(#[case] start_message: StartMessage) -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add multiple tasks only a single one will be started by default
    for _ in 0..3 {
        assert_success(add_task(shared, "sleep 60").await?);
    }
    // Wait for task 0 to start on its own.
    // We have to do this, otherwise we'll start task 1/2 beforehand, which prevents task 0 to be
    // started on its own.
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    // Start tasks 1 and 2 manually
    start_tasks(shared, TaskSelection::TaskIds(vec![1, 2])).await?;

    // Wait until all tasks are running
    for id in 0..3 {
        wait_for_task_condition(shared, id, Task::is_running).await?;
    }

    // Pause the whole daemon and wait until all tasks are paused
    pause_tasks(shared, TaskSelection::All).await?;
    for id in 0..3 {
        wait_for_task_condition(shared, id, |task| {
            matches!(task.status, TaskStatus::Paused { .. })
        })
        .await?;
    }

    // Send the kill message
    send_message(shared, start_message).await?;

    // Ensure all tasks are running
    for id in 0..3 {
        wait_for_task_condition(shared, id, Task::is_running).await?;
    }
    Ok(())
}
