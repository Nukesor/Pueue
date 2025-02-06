use crate::prelude::*;

use std::fs;
use std::path::Path;
use std::sync::MutexGuard;

use chrono::prelude::*;

use pueue_lib::settings::Settings;
use pueue_lib::state::{Group, GroupStatus, State, PUEUE_DEFAULT_GROUP};
use pueue_lib::task::{TaskResult, TaskStatus};

pub type LockedState<'a> = MutexGuard<'a, State>;

/// Check if a task can be deleted. \
/// We have to check all dependant tasks, that haven't finished yet.
/// This is necessary to prevent deletion of tasks which are specified as a dependency.
///
/// `to_delete` A list of task ids, which should also be deleted.
///             This allows to remove dependency tasks as well as their dependants.
pub fn is_task_removable(state: &LockedState, task_id: &usize, to_delete: &[usize]) -> bool {
    // Get all task ids of any dependant tasks.
    let dependants: Vec<usize> = state
        .tasks
        .iter()
        .filter(|(_, task)| {
            task.dependencies.contains(task_id) && !matches!(task.status, TaskStatus::Done { .. })
        })
        .map(|(_, task)| task.id)
        .collect();

    if dependants.is_empty() {
        return true;
    }

    // Check if the dependants are supposed to be deleted as well.
    let should_delete_dependants = dependants.iter().all(|task_id| to_delete.contains(task_id));
    if !should_delete_dependants {
        return false;
    }

    // Lastly, do a recursive check if there are any dependants on our dependants
    dependants
        .iter()
        .all(|task_id| is_task_removable(state, task_id, to_delete))
}

/// A small helper for handling task failures. \
/// Users can specify whether they want to pause the task's group or the
/// whole daemon on a failed tasks. This function wraps that logic and decides if anything should be
/// paused depending on the current settings.
///
/// `group` should be the name of the failed task.
pub fn pause_on_failure(state: &mut LockedState, settings: &Settings, group: &str) {
    if settings.daemon.pause_group_on_failure {
        if let Some(group) = state.groups.get_mut(group) {
            group.status = GroupStatus::Paused;
        }
    } else if settings.daemon.pause_all_on_failure {
        state.set_status_for_all_groups(GroupStatus::Paused);
    }
}

/// Save the current state to disk. \
/// We do this to restore in case of a crash. \
/// If log == true, the file will be saved with a time stamp.
///
/// In comparison to the daemon -> client communication, the state is saved
/// as JSON for readability and debugging purposes.
pub fn save_state(state: &State, settings: &Settings) -> Result<()> {
    let serialized = serde_json::to_string(&state).context("Failed to serialize state:");

    let serialized = serialized.unwrap();
    let path = settings.shared.pueue_directory();
    let temp = path.join("state.json.partial");
    let real = path.join("state.json");

    // Write to temporary log file first, to prevent loss due to crashes.
    fs::write(&temp, serialized).context("Failed to write temp file while saving state.")?;

    // Overwrite the original with the temp file, if everything went fine.
    fs::rename(&temp, &real).context("Failed to overwrite old state while saving state")?;

    debug!("State saved at: {real:?}");

    Ok(())
}

/// Restore the last state from a previous session. \
/// The state is stored as json in the `pueue_directory`.
///
/// If the state cannot be deserialized, an empty default state will be used instead. \
/// All groups with queued tasks will be automatically paused to prevent unwanted execution.
pub fn restore_state(pueue_directory: &Path) -> Result<Option<State>> {
    let path = pueue_directory.join("state.json");

    // Ignore if the file doesn't exist. It doesn't have to.
    if !path.exists() {
        info!("Couldn't find state from previous session at location: {path:?}");
        return Ok(None);
    }
    info!("Restoring state");

    // Try to load the file.
    let data = fs::read_to_string(&path).context("State restore: Failed to read file:\n\n{}")?;

    // Try to deserialize the state file.
    let mut state: State = serde_json::from_str(&data).context("Failed to deserialize state.")?;

    // Restore all tasks.
    // While restoring the tasks, check for any invalid/broken stati.
    for (_, task) in state.tasks.iter_mut() {
        // Handle ungraceful shutdowns while executing tasks.
        if let TaskStatus::Running { start, enqueued_at }
        | TaskStatus::Paused { start, enqueued_at } = task.status
        {
            info!(
                "Setting task {} with previous status {:?} to new status {:?}",
                task.id,
                task.status,
                TaskResult::Killed
            );
            task.status = TaskStatus::Done {
                start,
                end: Local::now(),
                enqueued_at,
                result: TaskResult::Killed,
            };
        }

        // Handle crash during editing of the task command.
        if matches!(task.status, TaskStatus::Locked { .. }) {
            task.status = TaskStatus::Stashed { enqueue_at: None };
        }

        // Go trough all tasks and set all groups that are no longer
        // listed in the configuration file to the default.
        let group = match state.groups.get_mut(&task.group) {
            Some(group) => group,
            None => {
                task.group = PUEUE_DEFAULT_GROUP.into();
                state
                    .groups
                    .entry(PUEUE_DEFAULT_GROUP.into())
                    .or_insert(Group {
                        status: GroupStatus::Running,
                        parallel_tasks: 1,
                    })
            }
        };

        // If there are any queued tasks, pause the group.
        // This should prevent any unwanted execution of tasks due to a system crash.
        if let TaskStatus::Queued { .. } = task.status {
            info!(
                "Pausing group {} to prevent unwanted execution of previous tasks",
                &task.group
            );
            group.status = GroupStatus::Paused;
        }
    }

    Ok(Some(state))
}
