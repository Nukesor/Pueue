//! Subprocess handling is platform specific code.
//!
//! The submodules of this module represent the different implementations for
//! each supported platform.
//! Depending on the target, the respective platform is read and loaded into this scope.

// Unix specific process handling
// Shared between Linux and Apple
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use self::unix::*;

// Linux specific process support
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use self::linux::*;

// Apple specific process support
#[cfg(target_vendor = "apple")]
mod apple;
#[cfg(target_vendor = "apple")]
pub use self::apple::*;

// Windows specific process handling
#[cfg(any(target_os = "windows"))]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::*;

/// Pueue directly interacts with processes.
/// Since these interactions can vary depending on the current platform, this enum is introduced.
/// The intend is to keep any platform specific code out of the top level code.
/// Even if that implicates adding some layers of abstraction.
#[derive(Debug)]
pub enum ProcessAction {
    Pause,
    Resume,
}
