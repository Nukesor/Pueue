use crate::prelude::*;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use pueue_lib::failure_msg;
use pueue_lib::log::read_and_compress_log_file;
use pueue_lib::log::*;
use pueue_lib::network::message::*;
use pueue_lib::network::protocol::{send_message, GenericStream};
use pueue_lib::settings::Settings;
use pueue_lib::state::SharedState;

/// Invoked when calling `pueue log`.
/// Return tasks and their output to the client.
pub fn get_log(settings: &Settings, state: &SharedState, message: LogRequestMessage) -> Message {
    let state = { state.lock().unwrap().clone() };

    let task_ids = match message.tasks {
        TaskSelection::All => state.tasks.keys().cloned().collect(),
        TaskSelection::TaskIds(task_ids) => task_ids,
        TaskSelection::Group(group) => state.task_ids_in_group(&group),
    };

    let mut tasks = BTreeMap::new();
    for task_id in task_ids.iter() {
        if let Some(task) = state.tasks.get(task_id) {
            // We send log output and the task at the same time.
            // This isn't as efficient as sending the raw compressed data directly,
            // but it's a lot more convenient for now.
            let (output, output_complete) = if message.send_logs {
                match read_and_compress_log_file(
                    *task_id,
                    &settings.shared.pueue_directory(),
                    message.lines,
                ) {
                    Ok((output, output_complete)) => (Some(output), output_complete),
                    Err(err) => {
                        // Fail early if there's some problem with getting the log output
                        return failure_msg!("Failed reading process output file: {err:?}");
                    }
                }
            } else {
                (None, true)
            };

            let task_log = TaskLogMessage {
                task: task.clone(),
                output,
                output_complete,
            };
            tasks.insert(*task_id, task_log);
        }
    }
    Message::LogResponse(tasks)
}

/// Handle the continuous stream of a some log output.
///
/// It's not actually a stream in the sense of a low-level network stream, but rather a series of
/// `Message::Stream` messages, that each send a portion of new log output.
///
/// It's basically our own chunked stream implementation on top of the protocol we established.
pub async fn follow_log(
    pueue_directory: &Path,
    stream: &mut GenericStream,
    state: &SharedState,
    message: StreamRequestMessage,
) -> Result<Message> {
    // The user can specify the id of the task they want to follow
    // If the id isn't specified and there's only a single running task, this task will be used.
    // However, if there are multiple running tasks, the user will have to specify an id.
    let task_id = if let Some(task_id) = message.task_id {
        task_id
    } else {
        // Get all ids of running tasks
        let state = state.lock().unwrap();
        let running_ids: Vec<_> = state
            .tasks
            .iter()
            .filter_map(|(&id, t)| if t.is_running() { Some(id) } else { None })
            .collect();

        // Return a message on "no" or multiple running tasks.
        match running_ids.len() {
            0 => {
                return Ok(create_failure_message("There are no running tasks."));
            }
            1 => running_ids[0],
            _ => {
                let running_ids = running_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(create_failure_message(format!(
                    "Multiple tasks are running, please select one of the following: {running_ids}"
                )));
            }
        }
    };

    // It might be that the task is not yet running.
    // Ensure that it exists and is started.
    loop {
        {
            let state = state.lock().unwrap();
            let Some(task) = state.tasks.get(&task_id) else {
                return Ok(create_failure_message(
                    "Pueue: The task to be followed doesn't exist.",
                ));
            };
            // The task is running or finished, we can start to follow.
            if task.is_running() || task.is_done() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    let mut handle = match get_log_file_handle(task_id, pueue_directory) {
        Err(_) => {
            return Ok(create_failure_message(
                "Couldn't find output files for task. Maybe it finished? Try `log`",
            ))
        }
        Ok(handle) => handle,
    };

    // Get the output path.
    // We need to check continuously, whether the file still exists,
    // since the file can go away (e.g. due to finishing a task).
    let path = get_log_path(task_id, pueue_directory);

    // If `lines` is passed as an option, we only want to show the last `X` lines.
    // To achieve this, we seek the file handle to the start of the `Xth` line
    // from the end of the file.
    // The loop following this section will then only copy those last lines to stdout.
    if let Some(lines) = message.lines {
        if let Err(err) = seek_to_last_lines(&mut handle, lines) {
            eprintln!("Error seeking to last lines from log: {err}");
        }
    }

    loop {
        // Check whether the file still exists. Exit if it doesn't.
        if !path.exists() {
            return Ok(create_success_message(
                "Pueue: Log file has gone away. Has the task been removed?",
            ));
        }
        // Read the next chunk of text from the last position.
        let mut buffer = Vec::new();

        if let Err(err) = handle.read_to_end(&mut buffer) {
            return Ok(create_failure_message(format!("Pueue Error: {err}")));
        };
        let text = String::from_utf8_lossy(&buffer).to_string();

        // Only send a message, if there's actual new content.
        if !text.is_empty() {
            // Send the next chunk.
            let response = Message::Stream(text);
            send_message(response, stream).await?;
        }

        // Check if the task in question does:
        // 1. Still exist
        // 2. Is still running
        //
        // In case it's not, close the stream.
        {
            let state = state.lock().unwrap();
            let Some(task) = state.tasks.get(&task_id) else {
                return Ok(create_failure_message(
                    "Pueue: The followed task has been removed.",
                ));
            };

            // The task is done, just close the stream.
            if !task.is_running() {
                return Ok(Message::Close);
            }
        }

        // Wait for 1 second before sending the next chunk.
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}
