use crate::prelude::*;

use std::{
    fs::{remove_file, File},
    path::{Path, PathBuf},
};

use super::{daemon_base_setup, daemon_with_settings, PueueDaemon};

/// A helper wrapper around [`daemon`] that also creates a lockfile that can be listened to from a
/// task via `inotifywait -e delete_self "$FILE"`.
/// This is super useful as it allows us to have a proper lock that we may release at any point in
/// time when working with timing specific issues in tests.
///
/// E.g. task1 depends on task0 and we want to make sure that task1 isn't started before task0
/// ends. This mechanism can be ensured that task0 only finishes when we allow it to do so.
pub async fn daemon_with_lockfile() -> Result<(PueueDaemon, PathBuf)> {
    let (settings, tempdir) = daemon_base_setup()?;
    let tempdir_path = tempdir.path().to_owned();

    let daemon = daemon_with_settings(settings, tempdir).await?;

    let lockfile = tempdir_path.join("file.lock");
    File::create(&lockfile)?;

    Ok((daemon, lockfile))
}

pub fn lockfile_command(path: &Path) -> String {
    format!("inotifywait -e delete_self \"{}\"", path.to_string_lossy())
}

pub fn clear_lock(path: &Path) -> Result<()> {
    remove_file(path).context("Failed to clear lock file")?;

    Ok(())
}
