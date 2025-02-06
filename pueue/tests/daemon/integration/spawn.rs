use crate::prelude::*;

use std::io::Read;

use rstest::rstest;

use pueue_lib::{
    log::get_log_file_handle,
    task::{TaskResult, TaskStatus},
};

use crate::helper::*;

/// Make sure a task that isn't able to spawn, prints out an error message to the task's log file.
#[rstest]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_fail_to_spawn_task() -> Result<()> {
    // Start a custom daemon that uses a shell command that doesn't exist.
    let (mut settings, tempdir) = daemon_base_setup()?;
    settings.daemon.shell_command =
        Some(vec!["thisshellshouldreallynotexist.hopefully".to_string()]);
    let tempdir_path = tempdir.path().to_path_buf();
    settings
        .save(&Some(tempdir_path.join("pueue.yml")))
        .context("Couldn't write pueue config to temporary directory")?;
    let daemon = daemon_with_settings(settings, tempdir).await?;

    let shared = &daemon.settings.shared;

    // Try to start a task. That task should then fail.
    assert_success(add_task(shared, "sleep 60").await?);
    let task = wait_for_task_condition(shared, 0, |task| task.failed()).await?;
    assert!(matches!(
        task.status,
        TaskStatus::Done {
            result: TaskResult::FailedToSpawn(_),
            ..
        }
    ));

    // Get the log output and ensure that there's the expected error log from the daemon.
    let mut log_file = get_log_file_handle(0, &tempdir_path)?;
    let mut output = String::new();
    log_file.read_to_string(&mut output)?;
    assert!(output.starts_with("Pueue error, failed to spawn task. Check your command."));

    Ok(())
}
