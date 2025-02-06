//! This module contains helper functions, which are used by both, the client and daemon tests.
use crate::prelude::*;

use tokio::io::{self, AsyncWriteExt};
use tracing::level_filters::LevelFilter;

pub use pueue_lib::state::PUEUE_DEFAULT_GROUP;

mod asserts;
mod daemon;
mod factories;
mod fixtures;
mod lockfile;
mod log;
mod network;
mod state;
mod task;
mod wait;

pub use self::log::*;
pub use asserts::*;
pub use daemon::*;
pub use factories::*;
pub use fixtures::*;
pub use lockfile::*;
pub use network::*;
pub use state::*;
pub use task::*;
pub use wait::*;

// Global acceptable test timeout
const TIMEOUT: u64 = 5000;

/// Use this function to enable log output for in-runtime daemon output.
#[allow(dead_code)]
pub fn enable_logger() {
    let level = LevelFilter::DEBUG;

    // idea: use local timezone?
    // Init a terminal logger
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .init();
}

/// A helper function to sleep for ms time.
/// Only used to avoid the biolerplate of importing the same stuff all over the place.
pub async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

/// A small helper function, which instantly writes the given string to stdout with a newline.
/// Useful for debugging async tests.
#[allow(dead_code)]
pub async fn async_println(out: &str) -> Result<()> {
    let mut stdout = io::stdout();
    stdout
        .write_all(out.as_bytes())
        .await
        .expect("Failed to write to stdout.");

    stdout
        .write_all("\n".as_bytes())
        .await
        .expect("Failed to write to stdout.");
    stdout.flush().await?;

    Ok(())
}
