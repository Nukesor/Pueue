use crate::prelude::*;

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Child;

use pueue_lib::network::message::*;
use pueue_lib::settings::*;

use super::*;

/// Send the Shutdown message to the test daemon.
pub async fn shutdown_daemon(shared: &Shared) -> Result<Message> {
    let message = Shutdown::Graceful;

    send_message(shared, message)
        .await
        .context("Failed to send Shutdown message")
}

/// Get a daemon pid from a specific pueue directory.
/// This function gives the daemon a little time to boot up, but ultimately crashes if it takes too
/// long.
pub async fn get_pid(pid_path: &Path) -> Result<i32> {
    // Give the daemon about 1 sec to boot and create the pid file.
    let sleep = 50;
    let tries = TIMEOUT / sleep;
    let mut current_try = 0;

    while current_try < tries {
        // The daemon didn't create the pid file yet. Wait for 100ms and try again.
        if !pid_path.exists() {
            sleep_ms(sleep).await;
            current_try += 1;
            continue;
        }

        let mut file = File::open(pid_path).context("Couldn't open pid file")?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .context("Couldn't write to file")?;

        // The file has been created but not yet been written to.
        if content.is_empty() {
            sleep_ms(50).await;
            current_try += 1;
            continue;
        }

        let pid = content
            .parse::<i32>()
            .map_err(|_| eyre!("Couldn't parse value: {content}"))?;
        return Ok(pid);
    }

    bail!("Couldn't find pid file after about 1 sec.");
}

/// Waits for a daemon to shut down.
pub async fn wait_for_shutdown(child: &mut Child) -> Result<()> {
    // Give the daemon about 1 sec to shutdown.
    let sleep = 50;
    let tries = TIMEOUT / sleep;
    let mut current_try = 0;

    while current_try < tries {
        // Try to read the process exit code. If this succeeds or
        // an error is returned, the process is gone.
        if let Ok(None) = child.try_wait() {
            // Process is still alive, wait a little longer
            sleep_ms(sleep).await;
            current_try += 1;
            continue;
        }
        // Process is gone; either there was a status code
        // or the child is not a child of this process (highly
        // unlikely).
        return Ok(());
    }

    bail!("Pueued daemon didn't shut down after about 2 sec.");
}
