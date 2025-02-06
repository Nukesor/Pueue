use std::{fs, path::PathBuf};

use color_eyre::{eyre::WrapErr, Result};

use pueue_lib::state::{GroupStatus, State, PUEUE_DEFAULT_GROUP};

/// We aim to have full backward compatibility for our state deserialization for as long as
/// possible. For this reason, an old v4.0.0 serialized state has been checked in.
///
/// **Warning**: This is only one part of our state tests.
///              There is another full test suite in the `pueue` project, which deals with domain
///              specific state restoration logic. This test only checks, whether we can
///              deserialize old state files.
///
/// We have to be able to restore from that state at all costs.
/// Everything else results in a breaking change and needs a major version change.
/// (For `pueue_lib` as well as `pueue`!)
#[test]
fn test_restore_from_old_state() -> Result<()> {
    better_panic::install();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("v4.0.0_state.json");

    // Try to load the file.
    let data = fs::read_to_string(path).wrap_err("State restore: Failed to read file")?;
    // Try to deserialize the state file.
    let state: State = serde_json::from_str(&data).wrap_err("Failed to deserialize state.")?;

    // Make sure the groups are loaded.
    assert!(
        state.groups.contains_key(PUEUE_DEFAULT_GROUP),
        "Group 'default' should exist."
    );
    assert_eq!(
        state.groups.get(PUEUE_DEFAULT_GROUP).unwrap().status,
        GroupStatus::Running
    );
    assert!(
        state.groups.contains_key("test"),
        "Group 'test' should exist"
    );
    assert_eq!(
        state.groups.get("test").unwrap().status,
        GroupStatus::Running
    );

    assert!(state.tasks.contains_key(&3), "Task 3 should exist");
    assert_eq!(state.tasks.get(&3).unwrap().command, "sleep 9000000");

    Ok(())
}
