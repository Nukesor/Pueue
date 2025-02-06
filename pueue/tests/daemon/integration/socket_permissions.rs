use crate::prelude::*;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use crate::helper::*;

/// Make sure that the socket permissions are appropriately set.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(not(target_os = "windows"))]
async fn test_socket_permissions_default() -> Result<()> {
    let (settings, _tempdir) = daemon_base_setup()?;
    let shared = &settings.shared;
    let mut child = standalone_daemon(shared).await?;

    assert_eq!(
        fs::metadata(shared.unix_socket_path())?
            .permissions()
            .mode()
            // The permissions are masked with 0o777 to only get the last 3
            // digits.
            & 0o777,
        0o700
    );

    child.kill()?;
    Ok(())
}

/// Make sure that the socket permissions can be changed
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(not(target_os = "windows"))]
async fn test_socket_permissions_modified() -> Result<()> {
    let (mut settings, _tempdir) = daemon_base_setup()?;
    settings.shared.unix_socket_permissions = Some(0o777);
    let shared = &settings.shared;
    settings
        .save(&Some(settings.shared.runtime_directory().join("pueue.yml")))
        .context("Couldn't write pueue config to temporary directory")?;

    let mut child = standalone_daemon(shared).await?;

    assert_eq!(
        fs::metadata(shared.unix_socket_path())?
            .permissions()
            .mode()
            // The permissions are masked with 0o777 to only get the last 3
            // digits.
            & 0o777,
        0o777
    );

    child.kill()?;
    Ok(())
}
