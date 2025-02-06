use crate::prelude::*;

use crate::helper::*;

/// Spin up the daemon and send a SIGTERM shortly afterwards.
/// This should trigger the graceful shutdown and kill the process.
#[tokio::test]
async fn test_ctrlc() -> Result<()> {
    let (settings, _tempdir) = daemon_base_setup()?;
    let mut child = standalone_daemon(&settings.shared).await?;

    use command_group::{Signal, UnixChildExt};
    // Send SIGTERM signal to process via nix
    child
        .signal(Signal::SIGTERM)
        .context("Failed to send SIGTERM to daemon")?;

    // Sleep for 500ms and give the daemon time to shut down
    sleep_ms(500).await;

    let result = child.try_wait();
    if !matches!(result, Ok(Some(_))) {
        println!("Got error when sending SIGTERM to daemon. {result:?}");
        bail!("Daemon process crashed after sending SIGTERM.");
    }
    let code = result.unwrap().unwrap();
    assert!(matches!(code.code(), Some(0)));

    Ok(())
}

/// Spin up the daemon and send a graceful shutdown message afterwards.
/// The daemon should shutdown normally and exit with a 0.
#[tokio::test]
async fn test_graceful_shutdown() -> Result<()> {
    let (settings, _tempdir) = daemon_base_setup()?;
    let mut child = standalone_daemon(&settings.shared).await?;

    // Kill the daemon gracefully and wait for it to shut down.
    assert_success(shutdown_daemon(&settings.shared).await?);
    wait_for_shutdown(&mut child).await?;

    // Sleep for 500ms and give the daemon time to shut down
    sleep_ms(500).await;

    let result = child.try_wait();
    assert!(matches!(result, Ok(Some(_))));
    let code = result.unwrap().unwrap();
    assert!(matches!(code.code(), Some(0)));

    Ok(())
}
