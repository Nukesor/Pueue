use crate::prelude::*;

use std::fs::read_to_string;
use std::fs::File;
use std::path::Path;

use tempfile::TempDir;

use pueue_lib::{network::message::*, task::Task};

use crate::helper::*;

/// This function creates files `[1-20]` in the specified directory.
/// The return value is the expected output.
///
/// If `partial == true`, the expected output are only the last 5 lines.
fn create_test_files(path: &Path, partial: bool) -> Result<String> {
    // Convert numbers from 1 to 01, so they're correctly ordered when using `ls`.
    let names: Vec<String> = (0..20)
        .map(|number| {
            if number < 10 {
                let mut name = "0".to_string();
                name.push_str(&number.to_string());
                name
            } else {
                number.to_string()
            }
        })
        .collect();

    for name in &names {
        File::create(path.join(name))?;
    }

    // Only return the last 5 lines if partial output is requested.
    if partial {
        return Ok((15..20).fold(String::new(), |mut full, name| {
            full.push_str(&name.to_string());
            full.push('\n');
            full
        }));
    }

    // Create the full expected output.
    let mut expected_output = names.join("\n");
    expected_output.push('\n');
    Ok(expected_output)
}

/// Make sure that receiving partial output from the daemon works.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_full_log() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a temporary directory and put some files into it.
    let tempdir = TempDir::new().unwrap();
    let tempdir_path = tempdir.path();
    let expected_output =
        create_test_files(tempdir_path, false).context("Failed to create test files.")?;

    // Add a task that lists those files and wait for it to finish.
    let command = format!("ls {tempdir_path:?}");
    assert_success(add_task(shared, &command).await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Request all log lines
    let output = get_task_log(shared, 0, None).await?;

    // Make sure it's the same
    assert_eq!(output, expected_output);

    Ok(())
}

/// Make sure that receiving partial output from the daemon works.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_partial_log() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a temporary directory and put some files into it.
    let tempdir = TempDir::new().unwrap();
    let tempdir_path = tempdir.path();
    let expected_output =
        create_test_files(tempdir_path, true).context("Failed to create test files.")?;

    // Add a task that lists those files and wait for it to finish.
    let command = format!("ls {tempdir_path:?}");
    assert_success(add_task(shared, &command).await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Debug output to see what the file actually looks like:
    let real_log_path = shared.pueue_directory().join("task_logs").join("0.log");
    let content = read_to_string(real_log_path).context("Failed to read actual file")?;
    println!("Actual log file contents: \n{content}");

    // Request a partial log for task 0
    let log_message = LogRequestMessage {
        tasks: TaskSelection::TaskIds(vec![0]),
        send_logs: true,
        lines: Some(5),
    };
    let response = send_message(shared, Message::Log(log_message)).await?;
    let logs = match response {
        Message::LogResponse(logs) => logs,
        _ => bail!("Received non LogResponse: {:#?}", response),
    };

    // Get the received output
    let logs = logs.get(&0).unwrap();
    let output = logs
        .output
        .clone()
        .ok_or(eyre!("Didn't find output on TaskLogMessage"))?;
    let output = decompress_log(output)?;

    // Make sure it's the same
    assert_eq!(output, expected_output);

    Ok(())
}

/// Ensure that stdout and stderr are properly ordered in log output.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_correct_log_order() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a task that lists those files and wait for it to finish.
    let command = "echo 'test' && echo 'error' && echo 'test'";
    assert_success(add_task(shared, command).await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    // Request all log lines
    let log_message = LogRequestMessage {
        tasks: TaskSelection::TaskIds(vec![0]),
        send_logs: true,
        lines: None,
    };
    let response = send_message(shared, Message::Log(log_message)).await?;
    let logs = match response {
        Message::LogResponse(logs) => logs,
        _ => bail!("Received non LogResponse: {:#?}", response),
    };

    // Get the received output
    let logs = logs.get(&0).unwrap();
    let output = logs
        .output
        .clone()
        .ok_or(eyre!("Didn't find output on TaskLogMessage"))?;
    let output = decompress_log(output)?;

    // Make sure it's the same
    assert_eq!(output, "test\nerror\ntest\n");

    Ok(())
}

/// Make sure that it's possible to get only logs of a specific group
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logs_of_group() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add one task on the default group.
    let command = "echo 'default group'";
    assert_success(add_task(shared, command).await?);

    // Add one task on the test group.
    let command = "echo 'testgroup'";
    assert_success(add_task_to_group(shared, command, "test_2").await?);

    // Wait for both to finish
    wait_for_task_condition(shared, 1, Task::is_done).await?;

    // Request the task's logs.
    let message = LogRequestMessage {
        tasks: TaskSelection::Group("test_2".to_string()),
        send_logs: true,
        lines: None,
    };
    let response = send_message(shared, message).await?;
    let logs = match response {
        Message::LogResponse(logs) => logs,
        _ => bail!("Didn't get log response response in get_state"),
    };

    assert_eq!(logs.len(), 1, "Sould only receive a single log entry.");

    Ok(())
}

/// Make sure that it's possible to get logs across groups.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logs_for_all() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add one task on the default group.
    let command = "echo 'default group'";
    assert_success(add_task(shared, command).await?);

    // Add one task on the test group.
    let command = "echo 'testgroup'";
    assert_success(add_task_to_group(shared, command, "test_2").await?);

    // Wait for both to finish
    wait_for_task_condition(shared, 1, Task::is_done).await?;

    // Request the task's logs.
    let message = LogRequestMessage {
        tasks: TaskSelection::All,
        send_logs: true,
        lines: None,
    };
    let response = send_message(shared, message).await?;
    let logs = match response {
        Message::LogResponse(logs) => logs,
        _ => bail!("Didn't get log response response in get_state"),
    };

    assert_eq!(logs.len(), 2, "Sould receive all log entries.");

    Ok(())
}
