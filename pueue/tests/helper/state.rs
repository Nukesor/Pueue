use crate::prelude::*;

use pueue_lib::network::message::*;
use pueue_lib::settings::*;
use pueue_lib::state::State;

use super::*;

/// Convenience function for getting the current state from the daemon.
pub async fn get_state(shared: &Shared) -> Result<Box<State>> {
    let response = send_message(shared, Message::Status).await?;
    match response {
        Message::StatusResponse(state) => Ok(state),
        _ => bail!("Didn't get status response in get_state"),
    }
}
