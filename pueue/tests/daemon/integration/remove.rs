use crate::prelude::*;

use pueue_lib::{network::message::*, task::Task};

use crate::helper::*;

/// Ensure that only removable tasks can be removed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_normal_remove() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // We'll add some tasks.
    // Task 0-2 will be immediately handled by the daemon, the other three tasks are queued for
    // now. However, we'll manipulate them in such a way, that we'll end up with this mapping:
    // 0 -> failed
    // 1 -> success
    // 2 -> running
    // 3 -> paused
    // 4 -> queued
    // 5 -> stashed
    for command in &["failing", "ls", "sleep 60", "sleep 60", "ls", "ls"] {
        assert_success(add_task(shared, command).await?);
    }
    // Wait for task2 to start. This implies task[0,1] being finished.
    wait_for_task_condition(shared, 2, Task::is_running).await?;

    // Explicitly start task3, wait for it to start and directly pause it.
    start_tasks(shared, TaskSelection::TaskIds(vec![3])).await?;
    wait_for_task_condition(shared, 3, Task::is_running).await?;

    pause_tasks(shared, TaskSelection::TaskIds(vec![3])).await?;

    // Stash task 5
    send_message(
        shared,
        StashMessage {
            tasks: TaskSelection::TaskIds(vec![5]),
            enqueue_at: None,
        },
    )
    .await?;

    let remove_message = Message::Remove(vec![0, 1, 2, 3, 4, 5]);
    send_message(shared, remove_message).await?;

    // Ensure that every task that isn't currently running can be removed
    let state = get_state(shared).await?;
    assert!(!state.tasks.contains_key(&0));
    assert!(!state.tasks.contains_key(&1));
    assert!(state.tasks.contains_key(&2));
    assert!(state.tasks.contains_key(&3));
    assert!(!state.tasks.contains_key(&4));
    assert!(!state.tasks.contains_key(&5));

    Ok(())
}
