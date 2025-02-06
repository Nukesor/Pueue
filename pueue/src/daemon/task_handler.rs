use crate::prelude::*;

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::prelude::*;

use pueue_lib::children::Children;
use pueue_lib::network::message::*;
use pueue_lib::network::protocol::socket_cleanup;
use pueue_lib::settings::Settings;
use pueue_lib::state::{Group, GroupStatus, SharedState};
use pueue_lib::task::{TaskResult, TaskStatus};

use crate::daemon::pid::cleanup_pid_file;
use crate::daemon::state_helper::save_state;
use crate::ok_or_shutdown;

use super::callbacks::{check_callbacks, spawn_callback};
use super::process_handler::finish::handle_finished_tasks;
use super::process_handler::spawn::spawn_new;
use super::state_helper::LockedState;

/// Main task handling loop.
/// In here a few things happen:
///
/// - Handle finished tasks, i.e. cleanup processes, update statuses.
/// - Callback handling logic. This is rather uncritical.
/// - Enqueue any stashed processes which are ready for being queued.
/// - Ensure tasks with dependencies have no failed ancestors
/// - Handle shutdown logic (graceful & not graceful).
/// - If the client requested a reset: reset the state if all children have been killed and handled.
/// - Check whether we can spawn new tasks.
///
/// We also wait for 300ms to prevent this loop from running hot.
pub async fn run(state: SharedState, settings: Settings) -> Result<()> {
    // Initialize the subprocess management structure.
    {
        let mut state = state.lock().unwrap();
        let mut pools = BTreeMap::new();
        for group in state.groups.keys() {
            pools.insert(group.clone(), BTreeMap::new());
        }
        state.children = Children(pools);
    }

    loop {
        'mutex_block: {
            let mut state = state.lock().unwrap();

            check_callbacks(&mut state);
            handle_finished_tasks(&settings, &mut state);

            // Check if we're in shutdown.
            // If all tasks are killed, we do some cleanup and exit.
            if state.shutdown.is_some() {
                handle_shutdown(&settings, &mut state);
                break 'mutex_block;
            }

            // If we aren't in shutdown mode, do the usual stuff
            handle_group_resets(&settings, &mut state);
            enqueue_delayed_tasks(&settings, &mut state);
            check_failed_dependencies(&settings, &mut state);
            spawn_new(&settings, &mut state);
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// Check if all tasks are killed.
/// If they aren't, we'll wait a little longer.
/// Once they're, we do some cleanup and exit.
fn handle_shutdown(settings: &Settings, state: &mut LockedState) {
    // There are still active tasks. Continue waiting until they're killed and cleaned up.
    if state.children.has_active_tasks() {
        return;
    }

    // Remove the unix socket.
    if let Err(error) = socket_cleanup(&settings.shared) {
        eprintln!("Failed to cleanup socket during shutdown.");
        eprintln!("{error}");
    }

    // Cleanup the pid file
    if let Err(error) = cleanup_pid_file(&settings.shared.pid_path()) {
        eprintln!("Failed to cleanup pid during shutdown.");
        eprintln!("{error}");
    }

    // Actually exit the program the way we're supposed to.
    // Depending on the current shutdown type, we exit with different exit codes.
    if matches!(state.shutdown, Some(Shutdown::Emergency)) {
        std::process::exit(1);
    }
    std::process::exit(0);
}

/// Users can issue to reset the daemon.
/// If that's the case, the `self.full_reset` flag is set to true, all children are killed
/// and no new tasks will be spawned.
/// This function checks, if all killed children have been handled.
/// If that's the case, completely reset the state
fn handle_group_resets(_settings: &Settings, state: &mut LockedState) {
    let groups_to_reset: Vec<String> = state
        .groups
        .iter()
        .filter(|(_name, group)| group.status == GroupStatus::Reset)
        .map(|(name, _)| name.to_string())
        .collect();

    for name in groups_to_reset.iter() {
        // Don't do any reset logic, if there're still some children are still up.
        if state.children.has_group_active_tasks(name) {
            continue;
        }

        // Remove all tasks that belong to the group to reset
        state.tasks.retain(|_id, task| &task.group != name);

        // Restart the group, now that it's devoid of tasks.
        if let Some(group) = state.groups.get_mut(name) {
            group.status = GroupStatus::Running;
        }
    }
}

/// As time passes, some delayed tasks may need to be enqueued.
/// Gather all stashed tasks and enqueue them if it is after the task's enqueue_at
fn enqueue_delayed_tasks(settings: &Settings, state: &mut LockedState) {
    let mut changed = false;
    for (_, task) in state.tasks.iter_mut() {
        if let TaskStatus::Stashed {
            enqueue_at: Some(time),
        } = task.status
        {
            if time <= Local::now() {
                info!("Enqueuing delayed task : {}", task.id);

                task.status = TaskStatus::Queued {
                    enqueued_at: Local::now(),
                };
                changed = true;
            }
        }
    }
    // Save the state if a task has been enqueued
    if changed {
        ok_or_shutdown!(settings, state, save_state(state, settings));
    }
}

/// Ensure that no `Queued` tasks have any failed dependencies.
/// Otherwise set their status to `Done` and result to `DependencyFailed`.
fn check_failed_dependencies(settings: &Settings, state: &mut LockedState) {
    // Get id's of all tasks with failed dependencies
    let has_failed_deps: Vec<_> = state
        .tasks
        .iter()
        .filter(|(_, task)| {
            matches!(task.status, TaskStatus::Queued { .. }) && !task.dependencies.is_empty()
        })
        .filter_map(|(id, task)| {
            // At this point we got all queued tasks with dependencies.
            // Go through all dependencies and ensure they didn't fail.
            let failed = task
                .dependencies
                .iter()
                .flat_map(|id| state.tasks.get(id))
                .filter(|task| task.failed())
                .map(|task| task.id)
                .next();

            failed.map(|f| (*id, f))
        })
        .collect();

    // Update the state of all tasks with failed dependencies.
    for (id, _) in has_failed_deps {
        // Get the task's group, since we have to check if it's paused.
        let group = if let Some(task) = state.tasks.get(&id) {
            task.group.clone()
        } else {
            continue;
        };

        // Only update the status, if the group isn't paused.
        // This allows users to fix and restart dependencies in-place without
        // breaking the dependency chain.
        if let Some(&Group {
            status: GroupStatus::Paused,
            ..
        }) = state.groups.get(&group)
        {
            continue;
        }

        // Update the task and return a clone to build the callback.
        let task = {
            let task = state.tasks.get_mut(&id).unwrap();
            // We know that this must be true, but we have to check anyway.
            let TaskStatus::Queued { enqueued_at } = task.status else {
                continue;
            };

            task.status = TaskStatus::Done {
                enqueued_at,
                start: Local::now(),
                end: Local::now(),
                result: TaskResult::DependencyFailed,
            };
            task.clone()
        };

        spawn_callback(settings, state, &task);
    }
}
