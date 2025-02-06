use crate::prelude::*;

use pretty_assertions::assert_eq;
use rstest::rstest;

use pueue_lib::network::message::*;
use pueue_lib::state::GroupStatus;
use pueue_lib::task::*;

use crate::helper::*;

/// Test if killing running tasks works as intended.
///
/// We test different ways of killing those tasks.
/// - Via the --all flag, which just kills everything.
/// - Via the --group flag, which just kills everything in the default group.
/// - Via specific ids.
///
/// If a whole group or everything is killed, the respective groups should also be paused,
/// as long as there's no further queued task.
/// This is security measure to prevent unwanted task execution in an emergency.
#[rstest]
#[case(
    KillMessage {
        tasks: TaskSelection::All,
        signal: None,
    }, true
)]
#[case(
    KillMessage {
        tasks: TaskSelection::Group(PUEUE_DEFAULT_GROUP.into()),
        signal: None,
    }, true
)]
#[case(
    KillMessage {
        tasks: TaskSelection::TaskIds(vec![0, 1, 2]),
        signal: None,
    }, false
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_kill_tasks_with_pause(
    #[case] kill_message: KillMessage,
    #[case] group_should_pause: bool,
) -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add multiple tasks and start them immediately
    for _ in 0..3 {
        assert_success(add_and_start_task(shared, "sleep 60").await?);
    }
    // Wait until all tasks are running, they should be start `immediately`.
    for id in 0..3 {
        assert_task_condition(
            shared,
            id,
            Task::is_running,
            "Tasks should start immediately.",
        )
        .await?;
    }

    // Add another task that will be normally enqueued.
    for _ in 0..3 {
        assert_success(add_task(shared, "sleep 60").await?);
    }

    // Send the kill message
    send_message(shared, kill_message).await?;

    // Make sure all tasks get killed
    for id in 0..3 {
        wait_for_task_condition(shared, id, |task| {
            matches!(
                task.status,
                TaskStatus::Done {
                    result: TaskResult::Killed,
                    ..
                }
            )
        })
        .await?;
    }

    // Groups should be paused in specific modes.
    if group_should_pause {
        let state = get_state(shared).await?;
        assert_eq!(
            state.groups.get(PUEUE_DEFAULT_GROUP).unwrap().status,
            GroupStatus::Paused
        );
    }

    Ok(())
}

/// This test ensures the following rule:
/// If a whole group or everything is killed, the respective groups should not be paused, as long
/// as there's no further queued task in that group.
///
/// We test different ways of killing those tasks.
/// - Via the --all flag, which just kills everything.
/// - Via the --group flag, which just kills everything in the default group.
/// - Via specific ids.
#[rstest]
#[case(
    KillMessage {
        tasks: TaskSelection::All,
        signal: None,
    }
)]
#[case(
    KillMessage {
        tasks: TaskSelection::Group(PUEUE_DEFAULT_GROUP.into()),
        signal: None,
    }
)]
#[case(
    KillMessage {
        tasks: TaskSelection::TaskIds(vec![0, 1, 2]),
        signal: None,
    }
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_kill_tasks_without_pause(#[case] kill_message: KillMessage) -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add multiple tasks and start them immediately
    for _ in 0..3 {
        assert_success(add_and_start_task(shared, "sleep 60").await?);
    }
    // Wait until all tasks are running, they should be start `immediately`.
    for id in 0..3 {
        assert_task_condition(
            shared,
            id,
            Task::is_running,
            "Tasks should start immediately",
        )
        .await?;
    }

    // Add a dummy group that also shouldn't be paused.
    add_group_with_slots(shared, "testgroup", 1).await?;

    // Send the kill message
    send_message(shared, kill_message).await?;

    // Make sure all tasks get killed
    for id in 0..3 {
        wait_for_task_condition(shared, id, |task| {
            matches!(
                task.status,
                TaskStatus::Done {
                    result: TaskResult::Killed,
                    ..
                }
            )
        })
        .await?;
    }

    // Groups should not be paused, since no other queued tasks exist at this point in time.
    let state = get_state(shared).await?;
    assert_eq!(
        state.groups.get(PUEUE_DEFAULT_GROUP).unwrap().status,
        GroupStatus::Running
    );
    assert_eq!(
        state.groups.get("testgroup").unwrap().status,
        GroupStatus::Running
    );

    Ok(())
}
