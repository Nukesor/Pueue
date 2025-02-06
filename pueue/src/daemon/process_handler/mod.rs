use crate::prelude::*;

use pueue_lib::network::message::{Shutdown, TaskSelection};
use pueue_lib::process_helper::{send_signal_to_child, ProcessAction};
use pueue_lib::settings::Settings;

use super::state_helper::LockedState;

pub mod finish;
pub mod kill;
pub mod pause;
pub mod spawn;
pub mod start;

/// This is a little helper macro, which looks at a critical result and shuts the
/// TaskHandler down, if an error occurred. This is mostly used if the state cannot
/// be written due to IO errors.
/// Those errors are considered unrecoverable and we should initiate a graceful shutdown
/// immediately.
#[macro_export]
macro_rules! ok_or_shutdown {
    ($settings:expr, $state:expr, $result:expr) => {
        match $result {
            Err(err) => {
                use pueue_lib::network::message::Shutdown;
                use $crate::daemon::process_handler::initiate_shutdown;
                ::tracing::error!(
                    "Initializing graceful shutdown. Encountered error in TaskHandler: {err}"
                );
                initiate_shutdown($settings, $state, Shutdown::Emergency);
                return;
            }
            Ok(inner) => inner,
        }
    };
}

/// Initiate shutdown, which includes killing all children and pausing all groups.
/// We don't have to pause any groups, as no new tasks will be spawned during shutdown anyway.
/// Any groups with queued tasks, will be automatically paused on state-restoration.
pub fn initiate_shutdown(settings: &Settings, state: &mut LockedState, shutdown: Shutdown) {
    // Only start shutdown if we aren't already in one.
    // Otherwise, we might end up with an endless recursion as `kill` might fail and initiate shutdown
    // once again.
    if state.shutdown.is_none() {
        state.shutdown = Some(shutdown);
        self::kill::kill(settings, state, TaskSelection::All, false, None);
    }
}

/// This is a small wrapper around the real platform dependant process handling logic
/// It only ensures, that the process we want to manipulate really does exists.
pub fn perform_action(state: &mut LockedState, id: usize, action: ProcessAction) -> Result<bool> {
    match state.children.get_child_mut(id) {
        Some(child) => {
            debug!("Executing action {action:?} to {id}");
            send_signal_to_child(child, &action)?;

            Ok(true)
        }
        None => {
            error!("Tried to execute action {action:?} to non existing task {id}");
            Ok(false)
        }
    }
}
