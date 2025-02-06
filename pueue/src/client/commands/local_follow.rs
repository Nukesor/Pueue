use crate::prelude::*;

use std::path::Path;

use pueue_lib::network::protocol::GenericStream;

use crate::client::commands::get_state;
use crate::client::display::follow_local_task_logs;

/// This function reads a log file from the filesystem and streams it to `stdout`.
/// This is the default behavior of `pueue`'s log reading logic, which is only possible
/// if `pueued` runs on the same environment.
///
/// `pueue follow` can be called without a `task_id`, in which case we check whether there's a
/// single running task. If that's the case, we default to it.
/// If there are multiple tasks, the user has to specify which task they want to follow.
pub async fn local_follow(
    stream: &mut GenericStream,
    pueue_directory: &Path,
    task_id: Option<usize>,
    lines: Option<usize>,
) -> Result<()> {
    let task_id = match task_id {
        Some(task_id) => task_id,
        None => {
            // The user didn't provide a task id.
            // Check whether we can find a single running task to follow.
            let state = get_state(stream).await?;
            let running_ids: Vec<_> = state
                .tasks
                .iter()
                .filter_map(|(&id, t)| if t.is_running() { Some(id) } else { None })
                .collect();

            match running_ids.len() {
                0 => {
                    bail!("There are no running tasks.");
                }
                1 => running_ids[0],
                _ => {
                    let running_ids = running_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    bail!(
                        "Multiple tasks are running, please select one of the following: {running_ids}",
                    );
                }
            }
        }
    };

    follow_local_task_logs(stream, pueue_directory, task_id, lines).await?;

    Ok(())
}
