use crate::prelude::*;

use pueue_lib::network::message::*;
use pueue_lib::settings::*;

use crate::helper::*;

/// Create a new group with a specific amount of slots.
pub async fn add_group_with_slots(shared: &Shared, group_name: &str, slots: usize) -> Result<()> {
    let add_message = GroupMessage::Add {
        name: group_name.to_string(),
        parallel_tasks: Some(slots),
    };
    assert_success(send_message(shared, add_message.clone()).await?);
    wait_for_group(shared, group_name).await?;

    Ok(())
}
