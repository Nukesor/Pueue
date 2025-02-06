use crate::prelude::*;

use pueue_lib::{
    network::message::{Signal, TaskSelection},
    process_helper::*,
    settings::Settings,
    state::GroupStatus,
    task::{Task, TaskStatus},
};

use crate::daemon::state_helper::{save_state, LockedState};
use crate::ok_or_shutdown;

/// Kill specific tasks or groups.
///
/// By default, this kills tasks with Rust's subprocess handling "kill" logic.
/// However, the user can decide to send unix signals to the processes as well.
///
/// `issued_by_user` This is `true` when a kill is issued by an actual user.
///   It is `false`, if the daemon resets or during shutdown.
///
///   In case `true` is given and  a `group` or `all` are killed the affected groups should
///   be paused under some circumstances. is mostly to prevent any further task execution
///   during an emergency. These circumstances are:
///   - There're further queued or scheduled tasks in a killed group.
///
/// `signal` Don't kill the task as usual, but rather send a unix process signal.
pub fn kill(
    settings: &Settings,
    state: &mut LockedState,
    tasks: TaskSelection,
    issued_by_user: bool,
    signal: Option<Signal>,
) {
    // Get the keys of all tasks that should be resumed
    let task_ids = match tasks {
        TaskSelection::TaskIds(task_ids) => task_ids,
        TaskSelection::Group(group_name) => {
            // Ensure that a given group exists. (Might not happen due to concurrency)
            if !state.groups.contains_key(&group_name) {
                return;
            };

            // Check whether the group should be paused before killing the tasks.
            if should_pause_group(state, issued_by_user, &group_name) {
                let group = state.groups.get_mut(&group_name).unwrap();
                group.status = GroupStatus::Paused;
            }

            // Determine all running or paused tasks in that group.
            let filtered_tasks = state.filter_tasks_of_group(
                |task| {
                    matches!(
                        task.status,
                        TaskStatus::Running { .. } | TaskStatus::Paused { .. }
                    )
                },
                &group_name,
            );

            info!("Killing tasks of group {group_name}");
            filtered_tasks.matching_ids
        }
        TaskSelection::All => {
            // Pause all groups, if applicable
            let group_names: Vec<String> = state.groups.keys().cloned().collect();
            for group_name in group_names {
                if should_pause_group(state, issued_by_user, &group_name) {
                    state.set_status_for_all_groups(GroupStatus::Paused);
                }
            }

            info!("Killing all running tasks");
            state.children.all_task_ids()
        }
    };

    for task_id in task_ids {
        if let Some(signal) = signal.clone() {
            send_internal_signal(state, task_id, signal);
        } else {
            kill_task(state, task_id);
        }
    }

    ok_or_shutdown!(settings, state, save_state(state, settings));
}

/// Send a signal to a specific child process.
/// This is a wrapper around [send_signal_to_child], which does a little bit of
/// additional error handling.
pub fn send_internal_signal(state: &mut LockedState, task_id: usize, signal: Signal) {
    let child = match state.children.get_child_mut(task_id) {
        Some(child) => child,
        None => {
            warn!("Tried to kill non-existing child: {task_id}");
            return;
        }
    };

    if let Err(err) = send_signal_to_child(child, signal) {
        warn!("Failed to send signal to task {task_id} with error: {err}");
    };
}

/// Kill a specific task and handle it accordingly.
/// Triggered on `reset` and `kill`.
pub fn kill_task(state: &mut LockedState, task_id: usize) {
    if let Some(child) = state.children.get_child_mut(task_id) {
        kill_child(task_id, child).unwrap_or_else(|err| {
            warn!(
                "Failed to send kill to task {task_id} child process {child:?} with error {err:?}"
            );
        })
    } else {
        warn!("Tried to kill non-existing child: {task_id}");
    }
}

/// Determine, whether a group should be paused during a kill command.
/// It should only be paused if:
/// - The kill was issued by the user, i.e. it wasn't issued by a system during shutdown/reset.
/// - The group that's being killed must have queued or stashed-enqueued tasks.
fn should_pause_group(state: &LockedState, issued_by_user: bool, group: &str) -> bool {
    if !issued_by_user {
        return false;
    }

    // Check if there're tasks that're queued or scheduled to be enqueued.
    let filtered_tasks = state.filter_tasks_of_group(Task::is_queued, group);
    !filtered_tasks.matching_ids.is_empty()
}
