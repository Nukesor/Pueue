use crate::prelude::*;

use std::collections::HashMap;

use chrono::{DateTime, Local};
use handlebars::{Handlebars, RenderError};
use pueue_lib::{
    log::{get_log_path, read_last_log_file_lines},
    process_helper::compile_shell_command,
    settings::Settings,
    task::{Task, TaskResult, TaskStatus},
};

use super::state_helper::LockedState;

/// Users can specify a callback that's fired whenever a task finishes.
/// The callback is performed by spawning a new subprocess.
pub fn spawn_callback(settings: &Settings, state: &mut LockedState, task: &Task) {
    // Return early, if there's no callback specified
    let Some(template_string) = &settings.daemon.callback else {
        return;
    };

    // Build the command to be called from the template string in the configuration file.
    let callback_command = match build_callback_command(settings, state, task, template_string) {
        Ok(callback_command) => callback_command,
        Err(err) => {
            error!("Failed to create callback command from template with error: {err}");
            return;
        }
    };

    let mut command = compile_shell_command(settings, &callback_command);

    // Spawn the callback subprocess and log if it fails.
    let spawn_result = command.spawn();
    let child = match spawn_result {
        Err(error) => {
            error!("Failed to spawn callback with error: {error}");
            return;
        }
        Ok(child) => child,
    };

    debug!("Spawned callback for task {}", task.id);
    state.callbacks.push(child);
}

/// Take the callback template string from the configuration and insert all parameters from the
/// finished task.
pub fn build_callback_command(
    settings: &Settings,
    state: &mut LockedState,
    task: &Task,
    template_string: &str,
) -> Result<String, RenderError> {
    // Init Handlebars. We set to strict, as we want to show an error on missing variables.
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);
    handlebars.register_escape_fn(handlebars::no_escape);

    // Add templating variables.
    let mut parameters = HashMap::new();
    parameters.insert("id", task.id.to_string());
    parameters.insert("command", task.command.clone());
    parameters.insert("path", (*task.path.to_string_lossy()).to_owned());

    // Add group information to template
    // This includes how many stashed and queued tasks are left in the group.
    parameters.insert("group", task.group.clone());
    let queued_tasks = state
        .filter_tasks_of_group(Task::is_queued, &task.group)
        .matching_ids
        .len();
    parameters.insert("queued_count", queued_tasks.to_string());
    let stashed_tasks = state
        .filter_tasks_of_group(|task| task.is_stashed(), &task.group)
        .matching_ids
        .len();
    parameters.insert("stashed_count", stashed_tasks.to_string());

    // Result takes the TaskResult Enum strings, unless it didn't finish yet.
    if let TaskStatus::Done { result, .. } = &task.status {
        parameters.insert("result", result.to_string());
    } else {
        parameters.insert("result", "None".into());
    }

    // Format and insert start and end times.
    let print_time = |time: Option<DateTime<Local>>| {
        time.map(|time| time.timestamp().to_string())
            .unwrap_or_default()
    };
    let (start, end) = task.start_and_end();
    parameters.insert("start", print_time(start));
    parameters.insert("end", print_time(end));

    // Read the last lines of the process' output and make it available.
    if let Ok(output) = read_last_log_file_lines(
        task.id,
        &settings.shared.pueue_directory(),
        settings.daemon.callback_log_lines,
    ) {
        parameters.insert("output", output);
    } else {
        parameters.insert("output", "".to_string());
    }

    let out_path = get_log_path(task.id, &settings.shared.pueue_directory());
    // Using Display impl of PathBuf which isn't necessarily a perfect
    // representation of the path but should work for most cases here
    parameters.insert("output_path", out_path.display().to_string());

    // Get the exit code
    if let TaskStatus::Done { result, .. } = &task.status {
        match result {
            TaskResult::Success => parameters.insert("exit_code", "0".into()),
            TaskResult::Failed(code) => parameters.insert("exit_code", code.to_string()),
            _ => parameters.insert("exit_code", "None".into()),
        };
    } else {
        parameters.insert("exit_code", "None".into());
    }

    handlebars.render_template(template_string, &parameters)
}

/// Look at all running callbacks and check if they're still running.
/// Handle finished callbacks and log their outcome.
pub fn check_callbacks(state: &mut LockedState) {
    let mut finished = Vec::new();
    for (id, child) in state.callbacks.iter_mut().enumerate() {
        match child.try_wait() {
            // Handle a child error.
            Err(error) => {
                error!("Callback failed with error {error:?}");
                finished.push(id);
            }
            // Child process did not exit yet.
            Ok(None) => continue,
            Ok(exit_status) => {
                info!("Callback finished with exit code {exit_status:?}");
                finished.push(id);
            }
        }
    }

    finished.reverse();
    for id in finished.iter() {
        // Explicitly allow this lint since we did a try_wait above and know that it finished.
        #[allow(clippy::zombie_processes)]
        state.callbacks.remove(*id);
    }
}
