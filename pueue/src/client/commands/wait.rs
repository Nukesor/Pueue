use crate::prelude::*;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::Local;
use crossterm::style::{Attribute, Color};
use pueue_lib::network::message::TaskSelection;
use pueue_lib::state::State;
use strum::{Display, EnumString};
use tokio::time::sleep;

use pueue_lib::network::protocol::GenericStream;
use pueue_lib::task::{Task, TaskResult, TaskStatus};

use crate::client::{commands::get_state, display::OutputStyle};

/// The `wait` subcommand can wait for these specific stati.
#[derive(Default, Debug, Clone, PartialEq, Display, EnumString)]
pub enum WaitTargetStatus {
    #[default]
    #[strum(serialize = "done", serialize = "Done")]
    Done,
    #[strum(serialize = "success", serialize = "Success")]
    Success,
    #[strum(serialize = "queued", serialize = "Queued")]
    Queued,
    #[strum(serialize = "running", serialize = "Running")]
    Running,
}

/// Wait until tasks are done.
/// Tasks can be specified by:
/// - Default queue (no parameter given)
/// - Group
/// - A list of task ids
/// - All tasks (`all == true`)
///
/// By default, this will output status changes of tasks to `stdout`.
/// Pass `quiet == true` to suppress any logging.
pub async fn wait(
    stream: &mut GenericStream,
    style: &OutputStyle,
    selection: TaskSelection,
    quiet: bool,
    target_status: Option<WaitTargetStatus>,
) -> Result<()> {
    let mut first_run = true;
    // Create a list of tracked tasks.
    // This way we can track any status changes and if any new tasks are added.
    let mut watched_tasks: HashMap<usize, TaskStatus> = HashMap::new();
    // Since tasks can be removed by users, we have to track tasks that actually finished.
    let mut finished_tasks: HashSet<usize> = HashSet::new();

    // Wait for either a provided target status or the default (`Done`).
    let target_status = target_status.clone().unwrap_or_default();
    loop {
        let state = get_state(stream).await?;
        let tasks = get_tasks(&state, &selection);

        if tasks.is_empty() {
            eprintln!("No tasks found for selection {selection:?}");
            return Ok(());
        }

        // Get current time for log output

        // Iterate over all matching tasks
        for task in tasks.iter() {
            // Get the previous status of the task.
            // Add it to the watchlist we we know this task yet.
            let Some(previous_status) = watched_tasks.get(&task.id).cloned() else {
                if finished_tasks.contains(&task.id) {
                    continue;
                }

                // Add new/unknown tasks to our watchlist
                watched_tasks.insert(task.id, task.status.clone());

                if !quiet {
                    log_new_task(task, style, first_run);
                }

                continue;
            };

            // The task's status didn't change, continue as there's nothing to do.
            if previous_status == task.status {
                continue;
            }

            // Update the (previous) task status and log any changes
            watched_tasks.insert(task.id, task.status.clone());
            if !quiet {
                log_status_change(previous_status, task, style);
            }
        }

        // We can stop waiting, if every task reached its the target state.
        // We have to check all watched tasks and handle any tasks that get removed.
        let task_ids: Vec<usize> = watched_tasks.keys().cloned().collect();
        for task_id in task_ids {
            // Get the correct task. If it no longer exists, remove it from the task list.
            let Some(task) = tasks.iter().find(|task| task.id == task_id) else {
                watched_tasks.remove(&task_id);
                continue;
            };

            // Check if the task hit the target status.
            if reached_target_status(task, &target_status) {
                watched_tasks.remove(&task_id);
                finished_tasks.insert(task_id);
            }

            // If we're waiting for `Success`ful tasks, check if any of the tasks failed.
            // If so, exit with a `1`.
            if target_status == WaitTargetStatus::Success && task.failed() {
                std::process::exit(1);
            }
        }

        if watched_tasks.is_empty() {
            break;
        }

        // Sleep for a few seconds. We don't want to hurt the CPU.
        // However, we allow faster polling when in a test environment.
        let mut sleep_time = 2000;
        if std::env::var("PUEUED_TEST_ENV_VARIABLE").is_ok() {
            sleep_time = 250;
        }
        sleep(Duration::from_millis(sleep_time)).await;
        first_run = false;
    }

    Ok(())
}

/// Check if a task reached the target status.
/// Other stati that can only occur after that status will also qualify.
fn reached_target_status(task: &Task, target_status: &WaitTargetStatus) -> bool {
    match target_status {
        WaitTargetStatus::Queued => {
            matches!(
                task.status,
                TaskStatus::Queued { .. } | TaskStatus::Running { .. } | TaskStatus::Done { .. }
            )
        }
        WaitTargetStatus::Running => {
            matches!(
                task.status,
                TaskStatus::Running { .. } | TaskStatus::Done { .. }
            )
        }
        WaitTargetStatus::Done => matches!(task.status, TaskStatus::Done { .. }),
        WaitTargetStatus::Success => {
            matches!(
                task.status,
                TaskStatus::Done {
                    result: TaskResult::Success,
                    ..
                }
            )
        }
    }
}

/// Get the correct tasks depending on a given TaskSelection.
fn get_tasks(state: &State, selection: &TaskSelection) -> Vec<Task> {
    match selection {
        // Get all tasks
        TaskSelection::All => state.tasks.values().cloned().collect(),
        // Get all tasks of a specific group
        TaskSelection::TaskIds(task_ids) => state
            .tasks
            .iter()
            .filter(|(id, _)| task_ids.contains(id))
            .map(|(_, task)| task.clone())
            .collect(),
        // Get all tasks of a specific group
        TaskSelection::Group(group) => state
            .tasks
            .iter()
            .filter(|(_, task)| task.group.eq(group))
            .map(|(_, task)| task.clone())
            .collect::<Vec<Task>>(),
    }
}

/// Write a log line about a newly discovered task.
fn log_new_task(task: &Task, style: &OutputStyle, first_run: bool) {
    let current_time = Local::now().format("%H:%M:%S").to_string();
    let color = get_color_for_status(&task.status);
    let task_id = style.style_text(task.id, None, Some(Attribute::Bold));
    let status = style.style_text(&task.status, Some(color), None);

    if !first_run {
        // Don't log non-active tasks in the initial loop.
        println!("{current_time} - New task {task_id} with status {status}");
        return;
    }

    if task.is_running() {
        // Show currently running tasks for better user feedback.
        println!("{current_time} - Found active Task {task_id} with status {status}",);
    }
}

/// Write a log line about a status changes of a task.
fn log_status_change(previous_status: TaskStatus, task: &Task, style: &OutputStyle) {
    let current_time = Local::now().format("%H:%M:%S").to_string();
    let task_id = style.style_text(task.id, None, Some(Attribute::Bold));

    // Check if the task has finished.
    // In case it has, show the task's result in human-readable form.
    // Color some parts of the output depending on the task's outcome.
    if let TaskStatus::Done { result, .. } = &task.status {
        let text = match result {
            TaskResult::Success => {
                let status = style.style_text("0", Some(Color::Green), None);
                format!("Task {task_id} succeeded with {status}")
            }
            TaskResult::DependencyFailed => {
                let status = style.style_text("failed dependencies", Some(Color::Red), None);
                format!("Task {task_id} failed due to {status}")
            }

            TaskResult::FailedToSpawn(_) => {
                let status = style.style_text("failed to spawn", Some(Color::Red), None);
                format!("Task {task_id} {status}")
            }
            TaskResult::Failed(exit_code) => {
                let status = style.style_text(exit_code, Some(Color::Red), Some(Attribute::Bold));
                format!("Task {task_id} failed with {status}")
            }
            TaskResult::Errored => {
                let status = style.style_text("IO error", Some(Color::Red), Some(Attribute::Bold));
                format!("Task {task_id} experienced an {status}.")
            }
            TaskResult::Killed => {
                let status = style.style_text("killed", Some(Color::Red), None);
                format!("Task {task_id} has been {status}")
            }
        };
        println!("{current_time} - {text}");

        return;
    }

    // The task didn't finish yet, but changed it's state (e.g. from `Queued` to `Running`).
    // Inform the user about this change.
    let new_status_color = get_color_for_status(&task.status);
    let previous_status_color = get_color_for_status(&previous_status);

    let previous_status = style.style_text(previous_status, Some(previous_status_color), None);
    let new_status = style.style_text(&task.status, Some(new_status_color), None);
    println!("{current_time} - Task {task_id} changed from {previous_status} to {new_status}",);
}

fn get_color_for_status(task_status: &TaskStatus) -> Color {
    match task_status {
        TaskStatus::Paused { .. } | TaskStatus::Locked { .. } => Color::White,
        TaskStatus::Running { .. } => Color::Green,
        TaskStatus::Done { result, .. } => {
            if matches!(result, TaskResult::Success) {
                Color::Green
            } else {
                Color::Red
            }
        }
        _ => Color::White,
    }
}
