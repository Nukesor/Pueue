use crate::prelude::*;

use chrono::Local;
use pueue_lib::log::clean_log_handles;
use pueue_lib::state::GroupStatus;
use pueue_lib::task::{TaskResult, TaskStatus};

use super::*;

use crate::daemon::callbacks::spawn_callback;
use crate::daemon::state_helper::{pause_on_failure, save_state, LockedState};
use crate::ok_or_shutdown;

/// Check whether there are any finished processes
/// In case there are, handle them and update the shared state
pub fn handle_finished_tasks(settings: &Settings, state: &mut LockedState) {
    // Clone the state ref, so we don't have two mutable borrows later on.
    let finished = get_finished(state);

    // Nothing to do. Early return
    if finished.is_empty() {
        return;
    }

    for ((task_id, group, worker_id), error) in finished.iter() {
        let (enqueued_at, start) = {
            let task = state.tasks.get(task_id).unwrap();
            // Get the enqueued_at/start times from the current state.
            match task.status {
                TaskStatus::Running { enqueued_at, start }
                | TaskStatus::Paused { enqueued_at, start } => (enqueued_at, start),
                _ => {
                    error!("Discovered a finished task in unexpected state! Please report this.");
                    error!("Task {task_id}: {task:#?}");
                    (Local::now(), Local::now())
                }
            }
        };

        // Handle std::io errors on child processes.
        // I have never seen something like this, but it might happen.
        if let Some(error) = error {
            let (_taks_id, _child) = state
                .children
                .0
                .get_mut(group)
                .expect("Worker group must exist when handling finished tasks.")
                .remove(worker_id)
                .expect("Errored child went missing while handling finished task.");

            // Update the tasks's state and return a clone for callback handling.
            let task = {
                let task = state.tasks.get_mut(task_id).unwrap();

                task.status = TaskStatus::Done {
                    enqueued_at,
                    start,
                    end: Local::now(),
                    result: TaskResult::Errored,
                };

                task.clone()
            };

            spawn_callback(settings, state, &task);
            error!("Child {} failed with io::Error: {:?}", task_id, error);

            pause_on_failure(state, settings, &task.group);
            continue;
        }

        // Handle any tasks that exited with some kind of exit code
        let (_task_id, mut child) = state
            .children
            .0
            .get_mut(group)
            .expect("Worker group must exist when handling finished tasks.")
            .remove(worker_id)
            .expect("Child of task {} went away while handling finished task.");

        // Get the exit code of the child.
        // Errors really shouldn't happen in here, since we already checked if it's finished
        // with try_wait() before.
        let exit_code_result = child.wait();
        let exit_code = exit_code_result
            .context(format!(
                "Failed on wait() for finished task {task_id} with error: {error:?}"
            ))
            .unwrap()
            .code();

        // Processes with exit code 0 exited successfully
        // Processes with `None` have been killed by a Signal
        let result = match exit_code {
            Some(0) => TaskResult::Success,
            Some(exit_code) => TaskResult::Failed(exit_code),
            None => TaskResult::Killed,
        };

        info!("Task {task_id} finished with result: {result:?}");

        // Update the tasks's state and return a clone for callback handling.
        let task = {
            let task = state
                .tasks
                .get_mut(task_id)
                .expect("Task was removed before child process has finished!");

            task.status = TaskStatus::Done {
                enqueued_at,
                start,
                end: Local::now(),
                result: result.clone(),
            };

            task.clone()
        };
        info!("WTF");
        spawn_callback(settings, state, &task);

        if let TaskResult::Failed(_) = result {
            pause_on_failure(state, settings, &task.group);
        }

        // Already remove the output files, if this group is being reset.
        if state
            .groups
            .get(&task.group)
            .map(|group| group.status == GroupStatus::Reset)
            .unwrap_or(true)
        {
            clean_log_handles(*task_id, &settings.shared.pueue_directory());
        }
    }

    ok_or_shutdown!(settings, state, save_state(state, settings));
}

/// Gather all finished tasks and sort them by finished and errored.
/// Returns a list of finished task ids and whether they errored or not.
fn get_finished(state: &mut LockedState) -> Vec<((usize, String, usize), Option<std::io::Error>)> {
    let mut finished = Vec::new();
    for (group, children) in state.children.0.iter_mut() {
        for (worker_id, (task_id, child)) in children.iter_mut() {
            match child.try_wait() {
                // Handle a child error.
                Err(error) => {
                    finished.push(((*task_id, group.clone(), *worker_id), Some(error)));
                }
                // Child process did not exit yet
                Ok(None) => continue,
                Ok(_exit_status) => {
                    finished.push(((*task_id, group.clone(), *worker_id), None));
                }
            }
        }
    }

    finished
}
