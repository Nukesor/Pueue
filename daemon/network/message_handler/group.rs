use pueue_lib::network::message::*;
use pueue_lib::state::SharedState;

use crate::network::response_helper::ensure_group_exists;
use crate::state_helper::{save_settings, save_state};

use super::*;
use crate::ok_or_return_failure_message;

/// Invoked on `pueue groups`.
/// Manage groups.
/// - Show groups
/// - Add group
/// - Remove group
pub fn group(message: GroupMessage, state: &SharedState) -> Message {
    let mut state = state.lock().unwrap();

    // Create a new group.
    if let Some(group) = message.add {
        if state.groups.contains_key(&group) {
            return create_failure_message(format!("Group \"{}\" already exists", group));
        }
        state.create_group(&group);

        // Save the state and the settings file.
        ok_or_return_failure_message!(save_state(&state));
        if let Err(error) = save_settings(&state) {
            return create_failure_message(format!(
                "Failed while saving the config file: {}",
                error
            ));
        }

        return create_success_message(format!("Group \"{}\" created", group));
    }

    // Remove an existing group.
    if let Some(group) = message.remove {
        if let Err(message) = ensure_group_exists(&state, &group) {
            return message;
        }

        if let Err(error) = state.remove_group(&group) {
            return create_failure_message(format!("{}", error));
        }

        // Save the state and the settings file.
        ok_or_return_failure_message!(save_state(&state));
        if let Err(error) = save_settings(&state) {
            return create_failure_message(format!(
                "Failed while saving the config file: {}",
                error
            ));
        }

        return create_success_message(format!("Group \"{}\" removed", group));
    }

    // Return information about all groups to the client.
    Message::GroupResponse(GroupResponseMessage {
        groups: state.groups.clone(),
        settings: state.settings.daemon.groups.clone(),
    })
}