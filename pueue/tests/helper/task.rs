use crate::prelude::*;

use std::collections::HashMap;
use std::env::vars;

use chrono::{DateTime, Local};
use pueue_lib::network::message::*;
use pueue_lib::settings::*;
use pueue_lib::task::{Task, TaskStatus};

use crate::helper::*;

/// Create a bare AddMessage for testing.
/// This is just here to minimize boilerplate code.
pub fn create_add_message<C: ToString>(shared: &Shared, command: C) -> AddMessage {
    AddMessage {
        command: command.to_string(),
        path: shared.pueue_directory(),
        envs: HashMap::from_iter(vars()),
        start_immediately: false,
        stashed: false,
        group: PUEUE_DEFAULT_GROUP.to_string(),
        enqueue_at: None,
        dependencies: Vec::new(),
        priority: None,
        label: None,
    }
}

/// Helper to create a stashed task
pub async fn create_stashed_task(
    shared: &Shared,
    command: &str,
    enqueue_at: Option<DateTime<Local>>,
) -> Result<Message> {
    let mut message = create_add_message(shared, command);
    message.stashed = true;
    message.enqueue_at = enqueue_at;

    send_message(shared, message)
        .await
        .context("Failed to to add task message")
}

/// Helper to either continue the daemon or start specific tasks
pub async fn start_tasks(shared: &Shared, tasks: TaskSelection) -> Result<Message> {
    let message = StartMessage { tasks };

    send_message(shared, message)
        .await
        .context("Failed to send Start tasks message")
}

/// Helper to pause the default group of the daemon
pub async fn pause_tasks(shared: &Shared, tasks: TaskSelection) -> Result<Message> {
    let message = PauseMessage { tasks, wait: false };

    send_message(shared, message)
        .await
        .context("Failed to send Pause message")
}

/// Convenience wrapper around `get_state` if you only need a single task.
pub async fn get_task(shared: &Shared, task_id: usize) -> Result<Task> {
    let state = get_state(shared).await?;
    let task = state
        .tasks
        .get(&0)
        .ok_or_else(|| eyre!("Couldn't find task {task_id}"))?;

    Ok(task.clone())
}

/// Convenience wrapper around `get_task` if you really only need the task's status.
pub async fn get_task_status(shared: &Shared, task_id: usize) -> Result<TaskStatus> {
    let task = get_task(shared, task_id).await?;
    Ok(task.status)
}
