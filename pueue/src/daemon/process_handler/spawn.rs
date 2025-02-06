use crate::prelude::*;

use std::io::Write;
use std::process::Stdio;

use chrono::Local;
use command_group::CommandGroup;
use pueue_lib::log::{create_log_file_handles, get_writable_log_file_handle};
use pueue_lib::process_helper::compile_shell_command;
use pueue_lib::settings::Settings;
use pueue_lib::state::GroupStatus;
use pueue_lib::task::{Task, TaskResult, TaskStatus};

use crate::daemon::callbacks::spawn_callback;
use crate::daemon::state_helper::{pause_on_failure, save_state, LockedState};
use crate::ok_or_shutdown;

/// See if we can start a new queued task.
pub fn spawn_new(settings: &Settings, state: &mut LockedState) {
    // Check whether a new task can be started.
    // Spawn tasks until we no longer have free slots available.
    while let Some(id) = get_next_task_id(state) {
        spawn_process(settings, state, id);
    }
}

/// Search and return the next task that can be started.
/// Precondition for a task to be started:
/// - is in Queued state
/// - There are free slots in the task's group
/// - The group is running
/// - has all its dependencies in `Done` state
///
/// Order at which tasks are picked (descending relevancy):
/// - Task with highest priority first
/// - Task with lowest ID first
pub fn get_next_task_id(state: &LockedState) -> Option<usize> {
    // Get all tasks that could theoretically be started right now.
    let mut potential_tasks: Vec<&Task> = state
            .tasks
            .iter()
            .filter(|(_, task)| matches!(task.status, TaskStatus::Queued {..}))
            .filter(|(_, task)| {
                // Make sure the task is assigned to an existing group.
                let group = match state.groups.get(&task.group) {
                    Some(group) => group,
                    None => {
                        error!(
                            "Got task with unknown group {}. Please report this!",
                            &task.group
                        );
                        return false;
                    }
                };

                // Let's check if the group is running. If it isn't, simply return false.
                if group.status != GroupStatus::Running {
                    return false;
                }

                // If parallel tasks are set to `0`, this means an unlimited amount of tasks may
                // run at any given time.
                if group.parallel_tasks == 0 {
                    return true;
                }

                // Get the currently running tasks by looking at the actually running processes.
                // They're sorted by group, which makes this quite convenient.
                let running_tasks = match state.children.0.get(&task.group) {
                    Some(children) => children.len(),
                    None => {
                        error!(
                            "Got valid group {}, but no worker pool has been initialized. This is a bug!",
                            &task.group
                        );
                        return false
                    }
                };

                // Make sure there are free slots in the task's group
                running_tasks < group.parallel_tasks
            })
            .filter(|(_, task)| {
                // Check whether all dependencies for this task are fulfilled.
                task.dependencies
                    .iter()
                    .flat_map(|id| state.tasks.get(id))
                    .all(|task| matches!(task.status, TaskStatus::Done{result: TaskResult::Success, ..}))
            })
            .map(|(_, task)| {task})
            .collect();

    // Order the tasks based on their priortiy and their task id.
    // Tasks with higher priority go first.
    // Tasks with the same priority are ordered by their id in ascending order, meaning that
    // tasks with smaller id will be processed first.
    potential_tasks.sort_by(|a, b| {
        // If they have the same prio, decide the execution order by task_id!
        if a.priority == b.priority {
            return a.id.cmp(&b.id);
        }

        // Otherwise, let the priority decide.
        b.priority.cmp(&a.priority)
    });

    // Return the id of the first task (if one has been found).
    potential_tasks.first().map(|task| task.id)
}

/// Actually spawn a new sub process
/// The output of subprocesses is piped into a separate file for easier access
pub fn spawn_process(settings: &Settings, state: &mut LockedState, task_id: usize) {
    // Check if the task exists and can actually be spawned. Otherwise do an early return.
    let Some(task) = state.tasks.get(&task_id) else {
        warn!("Tried to start non-existing task: {task_id}");
        return;
    };

    // Get the task's enqueue time and make sure we don't have invalid states for spawning.
    let enqueued_at = match &task.status {
        TaskStatus::Stashed { .. }
        | TaskStatus::Paused { .. }
        | TaskStatus::Running { .. }
        | TaskStatus::Done { .. } => {
            warn!("Tried to start task with status: {}", task.status);
            return;
        }
        TaskStatus::Queued { enqueued_at } => *enqueued_at,
        TaskStatus::Locked { .. } => Local::now(),
    };

    let pueue_directory = settings.shared.pueue_directory();

    // Try to get the log file to which the output of the process will be written to.
    // Panic if this doesn't work! This is unrecoverable.
    let (stdout_log, stderr_log) = match create_log_file_handles(task_id, &pueue_directory) {
        Ok((out, err)) => (out, err),
        Err(err) => {
            panic!("Failed to create child log files: {err:?}");
        }
    };

    // Get all necessary info for starting the task
    let (command, path, group, mut envs) = {
        let task = state.tasks.get(&task_id).unwrap();
        (
            task.command.clone(),
            task.path.clone(),
            task.group.clone(),
            task.envs.clone(),
        )
    };

    // Build the shell command that should be executed.
    let mut command = compile_shell_command(settings, &command);

    // Determine the worker's id depending on the current group.
    // Inject that info into the environment.
    let worker_id = state.children.get_next_group_worker(&group);
    envs.insert("PUEUE_GROUP".into(), group.clone());
    envs.insert("PUEUE_WORKER_ID".into(), worker_id.to_string());

    // Spawn the actual subprocess
    let spawned_command = command
        .current_dir(path)
        .stdin(Stdio::piped())
        .env_clear()
        .envs(envs.clone())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log))
        .group_spawn();

    // Check if the task managed to spawn
    let child = match spawned_command {
        Ok(child) => child,
        Err(err) => {
            let error = format!("Failed to spawn child {task_id} with err: {err:?}");
            error!("{}", error);

            // Write some debug log output to the task's log file.
            // This should always work, but print a datailed error if it didn't work.
            if let Ok(mut file) = get_writable_log_file_handle(task_id, &pueue_directory) {
                let log_output =
                    format!("Pueue error, failed to spawn task. Check your command.\n{error}");
                let write_result = file.write_all(log_output.as_bytes());
                if let Err(write_err) = write_result {
                    error!("Failed to write spawn error to task log: {}", write_err);
                }
            }

            // Update all necessary fields on the task.
            let task = {
                let task = state.tasks.get_mut(&task_id).unwrap();
                task.status = TaskStatus::Done {
                    enqueued_at,
                    start: Local::now(),
                    end: Local::now(),
                    result: TaskResult::FailedToSpawn(error),
                };
                task.clone()
            };

            // Spawn any callback if necessary
            spawn_callback(settings, state, &task);

            pause_on_failure(state, settings, &task.group);
            ok_or_shutdown!(settings, state, save_state(state, settings));
            return;
        }
    };

    // Save the process handle in our self.children datastructure.
    state.children.add_child(&group, worker_id, task_id, child);

    let task = state.tasks.get_mut(&task_id).unwrap();
    task.status = TaskStatus::Running {
        enqueued_at,
        start: Local::now(),
    };
    // Overwrite the task's environment variables with the new ones, containing the
    // PUEUE_WORKER_ID and PUEUE_GROUP variables.
    task.envs = envs;

    info!("Started task: {}", task.command);
    ok_or_shutdown!(settings, state, save_state(state, settings));
}
