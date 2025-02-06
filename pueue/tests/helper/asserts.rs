use crate::prelude::*;

use assert_matches::assert_matches;

use pueue_lib::settings::Shared;
use pueue_lib::state::State;
use pueue_lib::task::Task;
use pueue_lib::{network::message::*, state::GroupStatus};

use super::{get_state, send_message};

/// Assert that a message is a successful message.
pub fn assert_success(message: Message) {
    assert!(
        message.response_success(),
        "Expected to get successful message, got {message:?}",
    );
}

/// Assert that a message is a failure message.
pub fn assert_failure(message: Message) {
    assert_matches!(
        message,
        Message::Failure(_),
        "Expected to get FailureMessage, got {message:?}",
    );
}

/// A small helper script which pulls the newest state and asserts that a certain condition on a
/// specific task is given.
pub async fn assert_task_condition<F>(
    shared: &Shared,
    task_id: usize,
    condition: F,
    message: &str,
) -> Result<Task>
where
    F: Fn(&Task) -> bool,
{
    let state = get_state(shared).await?;
    match state.tasks.get(&task_id) {
        Some(task) => {
            if !condition(task) {
                bail!("Condition check for task {task_id} failed: {message}");
            }
            Ok(task.clone())
        }
        None => {
            bail!("Couldn't find task {task_id} while checking for condition: {message}")
        }
    }
}

/// Make sure a specific group has the expected status.
pub async fn assert_group_status(
    shared: &Shared,
    group_name: &str,
    expected_status: GroupStatus,
    message: &str,
) -> Result<()> {
    let state = get_state(shared).await?;
    match state.groups.get(group_name) {
        Some(group) => {
            if group.status != expected_status {
                bail!(
                    "Group {group_name} doesn't have expected status {expected_status:?}. Found {:?}: {message}",
                    group.status
                );
            }
            Ok(())
        }
        None => {
            bail!(
                "Couldn't find group {group_name} while asserting status {expected_status:?}: {message}"
            )
        }
    }
}

/// Make sure the expected environment variables are set.
/// This also makes sure, the variables have properly been injected into the processes'
/// environment.
pub async fn assert_worker_envs(
    shared: &Shared,
    state: &State,
    task_id: usize,
    worker: usize,
    group: &str,
) -> Result<()> {
    let task = state.tasks.get(&task_id).unwrap();
    // Make sure the environment variables have been properly set.
    assert_eq!(
        task.envs.get("PUEUE_GROUP"),
        Some(&group.to_string()),
        "Worker group didn't match for task {task_id}",
    );
    assert_eq!(
        task.envs.get("PUEUE_WORKER_ID"),
        Some(&worker.to_string()),
        "Worker id hasn't been correctly set for task {task_id}",
    );

    // Get the log output for the task.
    let response = send_message(
        shared,
        LogRequestMessage {
            tasks: TaskSelection::TaskIds(vec![task_id]),
            send_logs: true,
            lines: None,
        },
    )
    .await?;

    let Message::LogResponse(message) = response else {
        bail!("Expected LogResponse got {response:?}")
    };

    // Make sure the PUEUE_WORKER_ID and PUEUE_GROUP variables are present in the output.
    // They're always printed as to the [add_env_task] function.
    let log = message
        .get(&task_id)
        .expect("Log should contain requested task.");

    let stdout = log.output.clone().unwrap();
    let output = String::from_utf8_lossy(&stdout);
    assert!(
        output.contains(&format!("WORKER_ID: {worker}")),
        "Output should contain worker id {worker} for task {task_id}. Got: {output}",
    );
    assert!(
        output.contains(&format!("GROUP: {group}")),
        "Output should contain worker group {group} for task {task_id}. Got: {output}",
    );

    Ok(())
}
