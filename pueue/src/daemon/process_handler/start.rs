use crate::prelude::*;

use pueue_lib::{
    network::message::TaskSelection, process_helper::ProcessAction, settings::Settings,
    state::GroupStatus, task::TaskStatus,
};

use crate::daemon::process_handler::spawn::spawn_process;
use crate::daemon::state_helper::{save_state, LockedState};
use crate::ok_or_shutdown;

use super::perform_action;

/// Start specific tasks or groups.
///
/// By default, this command only resumes tasks.
/// However, if specific task_ids are provided, tasks can actually be force-started.
/// Of course, they can only be started if they're in a valid status, i.e. Queued/Stashed.
pub fn start(settings: &Settings, state: &mut LockedState, tasks: TaskSelection) {
    let task_ids = match tasks {
        TaskSelection::TaskIds(task_ids) => {
            // Start specific tasks.
            // This is handled differently and results in an early return, as this branch is
            // capable of force-spawning processes, instead of simply resuming tasks.
            for task_id in task_ids {
                // Continue all children that are simply paused
                if state.children.has_child(task_id) {
                    continue_task(state, task_id);
                } else {
                    // Start processes for all tasks that haven't been started yet
                    spawn_process(settings, state, task_id);
                }
            }
            ok_or_shutdown!(settings, state, save_state(state, settings));
            return;
        }
        TaskSelection::Group(group_name) => {
            // Ensure that a given group exists. (Might not happen due to concurrency)
            let group = match state.groups.get_mut(&group_name) {
                Some(group) => group,
                None => return,
            };

            // Set the group to running.
            group.status = GroupStatus::Running;
            info!("Resuming group {}", &group_name);

            let filtered_tasks = state.filter_tasks_of_group(
                |task| matches!(task.status, TaskStatus::Paused { .. }),
                &group_name,
            );

            filtered_tasks.matching_ids
        }
        TaskSelection::All => {
            // Resume all groups and the default queue
            info!("Resuming everything");
            state.set_status_for_all_groups(GroupStatus::Running);

            state.children.all_task_ids()
        }
    };

    // Resume all specified paused tasks
    for task_id in task_ids {
        continue_task(state, task_id);
    }

    ok_or_shutdown!(settings, state, save_state(state, settings));
}

/// Send a start signal to a paused task to continue execution.
fn continue_task(state: &mut LockedState, task_id: usize) {
    // Task doesn't exist
    if !state.children.has_child(task_id) {
        return;
    }

    // Encapsulate to prevent a duplicate borrow on `state`.
    let (enqueued_at, start) = {
        // Task is already done
        let Some(task) = state.tasks.get_mut(&task_id) else {
            return;
        };

        // Return early if the task is somehow already done and not cleaned up yet.
        if task.is_done() {
            warn!("Tried to resume finished task: {:?}", task);
            return;
        }

        // Don't send a resume signal if the task isn't paused.
        let TaskStatus::Paused { enqueued_at, start } = task.status else {
            warn!("Tried to resume unpaused task: {:?}", task);
            return;
        };
        (enqueued_at, start)
    };

    let success = match perform_action(state, task_id, ProcessAction::Resume) {
        Err(err) => {
            warn!("Failed to resume task {}: {:?}", task_id, err);
            false
        }
        Ok(success) => success,
    };

    if success {
        state.change_status(task_id, TaskStatus::Running { enqueued_at, start });
    }
}
