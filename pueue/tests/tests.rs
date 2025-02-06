#[cfg(unix)]
mod prelude {
    pub use color_eyre::eyre::{bail, eyre, WrapErr};
    pub use color_eyre::Result;
}

#[cfg(unix)]
mod helper;

#[cfg(unix)]
mod client;

#[cfg(unix)]
mod daemon;
