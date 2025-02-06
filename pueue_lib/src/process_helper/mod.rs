//! Subprocess handling is platform specific code.
//!
//! The submodules of this module represent the different implementations for
//! each supported platform.
//! Depending on the target, the respective platform is read and loaded into this scope.
use crate::prelude::*;

use std::{collections::HashMap, process::Command};

use crate::{network::message::Signal as InternalSignal, settings::Settings};

// Unix specific process handling
// Shared between Linux and Apple
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use self::unix::*;
#[cfg(unix)]
use command_group::Signal;

// Platform specific process support
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_vendor = "apple", path = "apple.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_os = "freebsd", path = "freebsd.rs")]
#[cfg_attr(target_os = "netbsd", path = "netbsd.rs")]
mod platform;
pub use self::platform::*;

/// Pueue directly interacts with processes.
/// Since these interactions can vary depending on the current platform, this enum is introduced.
/// The intend is to keep any platform specific code out of the top level code.
/// Even if that implicates adding some layers of abstraction.
#[derive(Debug)]
pub enum ProcessAction {
    Pause,
    Resume,
}

impl From<&ProcessAction> for Signal {
    fn from(action: &ProcessAction) -> Self {
        match action {
            ProcessAction::Pause => Signal::SIGSTOP,
            ProcessAction::Resume => Signal::SIGCONT,
        }
    }
}

impl From<InternalSignal> for Signal {
    fn from(signal: InternalSignal) -> Self {
        match signal {
            InternalSignal::SigKill => Signal::SIGKILL,
            InternalSignal::SigInt => Signal::SIGINT,
            InternalSignal::SigTerm => Signal::SIGTERM,
            InternalSignal::SigCont => Signal::SIGCONT,
            InternalSignal::SigStop => Signal::SIGSTOP,
        }
    }
}

/// Take a platform specific shell command and insert the actual task command via templating.
pub fn compile_shell_command(settings: &Settings, command: &str) -> Command {
    let shell_command = get_shell_command(settings);

    let mut handlebars = handlebars::Handlebars::new();
    handlebars.set_strict_mode(true);
    handlebars.register_escape_fn(handlebars::no_escape);

    // Make the command available to the template engine.
    let mut parameters = HashMap::new();
    parameters.insert("pueue_command_string", command);

    // We allow users to provide their own shell command.
    // They should use the `{{ pueue_command_string }}` placeholder.
    let mut compiled_command = Vec::new();
    for part in shell_command {
        let compiled_part = handlebars
            .render_template(&part, &parameters)
            .unwrap_or_else(|_| {
                panic!("Failed to render shell command for template: {part} and parameters: {parameters:?}")
            });

        compiled_command.push(compiled_part);
    }

    let executable = compiled_command.remove(0);

    // Chain two `powershell` commands, one that sets the output encoding to utf8 and then the user provided one.
    let mut command = Command::new(executable);
    for arg in compiled_command {
        command.arg(&arg);
    }

    // Inject custom environment variables.
    if !settings.daemon.env_vars.is_empty() {
        info!(
            "Inject environment variables: {:?}",
            &settings.daemon.env_vars
        );
        command.envs(&settings.daemon.env_vars);
    }

    command
}
