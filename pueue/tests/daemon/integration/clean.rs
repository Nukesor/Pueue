use crate::prelude::*;

use pueue_lib::{network::message::*, task::Task};

use crate::helper::*;

/// Ensure that clean only removes finished tasks
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_normal_clean() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // This should result in one failed, one finished, one running and one queued task.
    for command in &["failing", "ls", "sleep 60", "ls"] {
        assert_success(add_task(shared, command).await?);
    }
    // Wait for task2 to start. This implies that task[0,1] are done.
    wait_for_task_condition(shared, 2, Task::is_running).await?;

    // Send the clean message
    let clean_message = CleanMessage {
        successful_only: false,
        group: None,
    };
    send_message(shared, clean_message).await?;

    // Assert that task 0 and 1 have both been removed
    let state = get_state(shared).await?;
    assert!(!state.tasks.contains_key(&0));
    assert!(!state.tasks.contains_key(&1));

    Ok(())
}

/// Ensure only successful tasks are removed, if the `-s` flag is set.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_successful_only_clean() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // This should result in one failed, one finished, one running and one queued task.
    for command in &["failing", "ls"] {
        assert_success(add_task(shared, command).await?);
    }
    // Wait for task2 to start. This implies task[0,1] being finished.
    wait_for_task_condition(shared, 1, Task::is_done).await?;

    // Send the clean message
    let clean_message = CleanMessage {
        successful_only: true,
        group: None,
    };
    send_message(shared, clean_message).await?;

    // Assert that task 0 is still there, as it failed.
    let state = get_state(shared).await?;
    assert!(state.tasks.contains_key(&0));
    // Task 1 should have been removed.
    assert!(!state.tasks.contains_key(&1));

    Ok(())
}

/// Ensure only tasks of the selected group are cleaned up
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_clean_in_selected_group() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    add_group_with_slots(shared, "other", 1).await?;

    for group in &[PUEUE_DEFAULT_GROUP, "other"] {
        for command in &["failing", "ls", "sleep 60", "ls"] {
            assert_success(add_task_to_group(shared, command, group).await?);
        }
    }

    // Wait for task6 to start. This implies task[4,5] in the 'other' group being finished.
    wait_for_task_condition(shared, 6, Task::is_running).await?;

    // Send the clean message
    let clean_message = CleanMessage {
        successful_only: false,
        group: Some("other".to_string()),
    };
    send_message(shared, clean_message).await?;

    // Assert that task 0 and 1 are still there
    let state = get_state(shared).await?;
    assert!(state.tasks.contains_key(&0));
    assert!(state.tasks.contains_key(&1));
    assert!(state.tasks.contains_key(&2));
    assert!(state.tasks.contains_key(&3));
    // Assert that task 4 and 5 have both been removed
    assert!(!state.tasks.contains_key(&4));
    assert!(!state.tasks.contains_key(&5));
    assert!(state.tasks.contains_key(&6));
    assert!(state.tasks.contains_key(&7));

    Ok(())
}

/// Ensure only successful tasks are removed, if the `-s` flag is set.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_clean_successful_only_in_selected_group() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    add_group_with_slots(shared, "other", 1).await?;
    for group in &[PUEUE_DEFAULT_GROUP, "other"] {
        for command in &["failing", "ls", "sleep 60", "ls"] {
            assert_success(add_task_to_group(shared, command, group).await?);
        }
    }

    // Wait for task6 to start. This implies task[4,5] in the 'other' group being finished.
    wait_for_task_condition(shared, 6, Task::is_running).await?;

    // Send the clean message
    let clean_message = CleanMessage {
        successful_only: true,
        group: Some("other".to_string()),
    };
    send_message(shared, clean_message).await?;

    let state = get_state(shared).await?;
    // group default
    assert!(state.tasks.contains_key(&0));
    assert!(state.tasks.contains_key(&1));
    assert!(state.tasks.contains_key(&2));
    assert!(state.tasks.contains_key(&3));

    // group other
    assert!(state.tasks.contains_key(&4));
    // Task 5 should have been removed.
    assert!(!state.tasks.contains_key(&5));
    assert!(state.tasks.contains_key(&6));
    assert!(state.tasks.contains_key(&7));

    Ok(())
}
