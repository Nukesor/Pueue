use crate::prelude::*;

use std::fs::File;
use std::io::prelude::*;

use pueue::daemon::state_helper::restore_state;
use tempfile::TempDir;

use pueue_lib::settings::Settings;

/// 4.0.0 introduced numerous breaking changes.
/// From here on, we now aim to once again have full backward compatibility.
/// For this reason, an old v4.0.0 serialized state has been checked in.
///
/// We have to be able to restore from that state at all costs.
/// Everything else results in a breaking change and needs a major version change.
///
/// On top of simply having an old state, I also added a few non-existing fields.
/// This should be handled as well.
#[test]
fn test_restore_from_old_state() -> Result<()> {
    better_panic::install();
    let old_state = include_str!("data/v4.0.0_state.json");

    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();

    // Open new file and write old state to it.
    let temp_state_path = temp_dir.path().join("state.json");
    let mut file = File::create(temp_state_path)?;
    file.write_all(old_state.as_bytes())?;

    let mut settings = Settings::default();
    settings.shared.pueue_directory = Some(temp_path.to_path_buf());

    let state = restore_state(&settings.shared.pueue_directory())
        .context("Failed to restore state in test")?;

    assert!(state.is_some());

    Ok(())
}
