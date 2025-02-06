use crate::prelude::*;

use pueue_lib::task::Task;

use crate::client::helper::*;

/// Set an environment variable and make sure it's there afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_environment() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a stashed task so we can edit it.
    run_client_command(shared, &["add", "--stashed", "echo $TEST_VARIABLE"])?;

    // Set the environment variable
    run_client_command(shared, &["env", "set", "0", "TEST_VARIABLE", "thisisatest"])?;

    // Now start the command and wait for it to finish
    run_client_command(shared, &["enqueue", "0"])?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let state = get_state(shared).await?;
    println!("{:#?}", state.tasks[&0].envs);

    // Make sure the environment variable has been set.
    let output = run_client_command(shared, &["follow", "0"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!("thisisatest", stdout.trim());

    Ok(())
}

/// Set an environment variable, immediately unset it and make sure it's not there afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unset_environment() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a stashed task so we can edit it.
    run_client_command(shared, &["add", "--stashed", "echo $TEST_VARIABLE"])?;

    // Set the environment variable
    run_client_command(shared, &["env", "set", "0", "TEST_VARIABLE", "thisisatest"])?;

    // Unset the environment variable again.
    run_client_command(shared, &["env", "unset", "0", "TEST_VARIABLE"])?;

    // Now start the command and wait for it to finish
    run_client_command(shared, &["enqueue", "0"])?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let state = get_state(shared).await?;
    println!("{:#?}", state.tasks[&0].envs);

    // Make sure the environment variable has been set.
    let output = run_client_command(shared, &["follow", "0"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!("", stdout.trim());

    Ok(())
}
