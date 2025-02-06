use crate::prelude::*;

use std::collections::HashMap;

use pueue_lib::{settings::EditMode, task::TaskStatus};

use crate::client::helper::*;

/// Test that editing a task without any flags only updates the command.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_task_directory() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a stashed message which we'll edit later on.
    let mut message = create_add_message(shared, "this is a test");
    message.stashed = true;
    send_message(shared, message)
        .await
        .context("Failed to to add stashed task.")?;

    // Update the task's command by piping a string to the temporary file.
    let mut envs = HashMap::new();
    envs.insert(
        "EDITOR",
        "echo 'expected command string' > ${PUEUE_EDIT_PATH}/0/command ||",
    );
    run_client_command_with_env(shared, &["edit", "0"], envs)?;

    // Make sure that both the command has been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.command, "expected command string");

    // All other properties should be unchanged.
    assert_eq!(task.path, daemon.tempdir.path());
    assert_eq!(task.label, None);
    assert_eq!(task.priority, 0);

    Ok(())
}

/// Test that editing a multiple task properties works as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_all_task_properties() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a stashed message which we'll edit later on.
    let mut message = create_add_message(shared, "this is a test");
    message.stashed = true;
    send_message(shared, message)
        .await
        .context("Failed to to add stashed task.")?;

    // Update all task properties by piping a string to the respective temporary file.
    let mut envs = HashMap::new();
    envs.insert(
        "EDITOR",
        "echo 'command' > ${PUEUE_EDIT_PATH}/0/command && \
echo '/tmp' > ${PUEUE_EDIT_PATH}/0/path && \
echo 'label' > ${PUEUE_EDIT_PATH}/0/label && \
echo '5' > ${PUEUE_EDIT_PATH}/0/priority || ",
    );
    run_client_command_with_env(shared, &["edit", "0"], envs)?;

    // Make sure that all properties have been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.command, "command");
    assert_eq!(task.path.to_string_lossy(), "/tmp");
    assert_eq!(task.label, Some("label".to_string()));
    assert_eq!(task.priority, 5);

    Ok(())
}

/// Ensure that deleting the label in the editor result in the deletion of the task's label.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_delete_label() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a stashed message which we'll edit later on.
    let mut message = create_add_message(shared, "this is a test");
    message.stashed = true;
    message.label = Some("Testlabel".to_owned());
    send_message(shared, message)
        .await
        .context("Failed to to add stashed task.")?;

    // Echo an empty string into the file.
    let mut envs = HashMap::new();
    envs.insert("EDITOR", "echo '' > ${PUEUE_EDIT_PATH}/0/label ||");
    run_client_command_with_env(shared, &["edit", "0"], envs)?;

    // Make sure that the label has indeed be deleted
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.label, None);

    Ok(())
}

/// Ensure that updating the priority in the editor results in the modification of the task's priority.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_change_priority() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a stashed message which we'll edit later on.
    let mut message = create_add_message(shared, "this is a test");
    message.stashed = true;
    message.priority = Some(0);
    send_message(shared, message)
        .await
        .context("Failed to to add stashed task.")?;

    // Echo a new priority into the file.
    let mut envs = HashMap::new();
    envs.insert("EDITOR", "echo '99' > ${PUEUE_EDIT_PATH}/0/priority ||");
    run_client_command_with_env(shared, &["edit", "0"], envs)?;

    // Make sure that the priority has indeed been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.priority, 99);

    Ok(())
}

/// Test that automatic restoration of a task's state works, if the edit command fails for some
/// reason.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fail_to_edit_task() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Create a stashed message which we'll edit later on.
    let mut message = create_add_message(shared, "this is a test");
    message.stashed = true;
    send_message(shared, message)
        .await
        .context("Failed to to add stashed task.")?;

    // Run a editor command that crashes.
    let mut envs = HashMap::new();
    envs.insert("EDITOR", "non_existing_test_binary");
    let output = run_client_command_with_env(shared, &["edit", "0"], envs)?;
    assert!(
        !output.status.success(),
        "The command should fail, as the command isn't valid"
    );

    // Make sure that nothing has changed and the task is `Stashed` again.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.command, "this is a test");
    assert_eq!(task.status, TaskStatus::Stashed { enqueue_at: None });

    Ok(())
}

/// Test that editing a task without any flags only updates the command.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_task_toml() -> Result<()> {
    // Overwrite the edit mode to toml.
    let (mut settings, tempdir) = daemon_base_setup()?;
    settings.client.edit_mode = EditMode::Toml;
    settings.save(&Some(tempdir.path().join("pueue.yml")))?;
    let daemon = daemon_with_settings(settings, tempdir).await?;
    let shared = &daemon.settings.shared;

    // Create a stashed message which we'll edit later on.
    let mut message = create_add_message(shared, "this is a test");
    message.stashed = true;
    send_message(shared, message)
        .await
        .context("Failed to to add stashed task.")?;

    // Update the task's command by piping a string to the temporary file.
    let mut envs = HashMap::new();
    envs.insert(
        "EDITOR",
        "echo '[0]\nid = 0\ncommand = \"expected command string\"\npath = \"/tmp\"\npriority = 0' > ${PUEUE_EDIT_PATH} ||",
    );
    run_client_command_with_env(shared, &["edit", "0"], envs)?;

    // Make sure that both the command has been updated.
    let state = get_state(shared).await?;
    let task = state.tasks.get(&0).unwrap();
    assert_eq!(task.command, "expected command string");
    assert_eq!(task.path.to_string_lossy(), "/tmp");

    // All other properties should be unchanged.
    assert_eq!(task.label, None);
    assert_eq!(task.priority, 0);

    Ok(())
}
