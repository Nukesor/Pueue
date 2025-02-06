use crate::prelude::*;

use pueue_lib::task::Task;

use crate::helper::*;

/// Make sure that the daemon's environment variables don't bleed into the spawned subprocesses.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_isolated_task_environment() -> Result<()> {
    let (settings, _tempdir) = daemon_base_setup()?;
    let mut child = standalone_daemon(&settings.shared).await?;

    let shared = &settings.shared;

    // Spawn a task which prints a special environment variable.
    // This environment variable is injected into the daemon's environment.
    // It shouldn't show up in the task's environment, as the task should be isolated!
    assert_success(add_and_start_task(shared, "echo $PUEUED_TEST_ENV_VARIABLE").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let log = get_task_log(shared, 0, None).await?;

    // The log output should be empty
    assert_eq!(log, "\n");

    child.kill()?;
    Ok(())
}
