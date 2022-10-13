use std::process::Command;

// We allow anyhow in here, as this is a module that'll be strictly used internally.
// As soon as it's obvious that this is code is intended to be exposed to library users, we have to
// go ahead and replace any `anyhow` usage by proper error handling via our own Error type.
use anyhow::Result;
use command_group::{GroupChild, Signal, UnixChildExt};
use log::info;

use super::ProcessAction;
use crate::network::message::Signal as InternalSignal;

pub fn compile_shell_command(command_string: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(command_string);

    command
}

fn map_action_to_signal(action: &ProcessAction) -> Signal {
    match action {
        ProcessAction::Pause => Signal::SIGSTOP,
        ProcessAction::Resume => Signal::SIGCONT,
    }
}

fn map_internal_signal_to_nix_signal(signal: InternalSignal) -> Signal {
    match signal {
        InternalSignal::SigKill => Signal::SIGKILL,
        InternalSignal::SigInt => Signal::SIGINT,
        InternalSignal::SigTerm => Signal::SIGTERM,
        InternalSignal::SigCont => Signal::SIGCONT,
        InternalSignal::SigStop => Signal::SIGSTOP,
    }
}

/// Convenience wrapper around `send_signal_to_child` for raw unix signals.
/// Its purpose is to hide platform specific logic.
pub fn send_internal_signal_to_child(
    child: &mut GroupChild,
    signal: InternalSignal,
    send_to_children: bool,
) -> Result<()> {
    let signal = map_internal_signal_to_nix_signal(signal);
    send_signal_to_child(child, signal, send_to_children)
}

/// Convenience wrapper around `send_signal_to_child` for internal actions on processes.
/// Its purpose is to hide platform specific logic.
pub fn run_action_on_child(
    child: &mut GroupChild,
    action: &ProcessAction,
    send_to_children: bool,
) -> Result<()> {
    let signal = map_action_to_signal(action);
    send_signal_to_child(child, signal, send_to_children)
}

/// Send a signal to one of Pueue's child process or process group handles.
pub fn send_signal_to_child(
    child: &mut GroupChild,
    signal: Signal,
    send_to_children: bool,
) -> Result<()> {
    if send_to_children {
        // Send the signal to the process group
        child.signal(signal)?;
    } else {
        // Send the signal to the process itself
        child.inner().signal(signal)?;
    }
    Ok(())
}

/// This is a helper function to safely kill a child process or process group.
/// Its purpose is to properly kill all processes and prevent any dangling processes.
pub fn kill_child(
    task_id: usize,
    child: &mut GroupChild,
    kill_children: bool,
) -> std::io::Result<()> {
    match if kill_children {
        child.kill()
    } else {
        child.inner().kill()
    } {
        Ok(_) => Ok(()),
        Err(ref e) if e.kind() == std::io::ErrorKind::InvalidData => {
            // Process already exited
            info!("Task {task_id} has already finished by itself.");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;
    use std::time::Duration;

    use anyhow::Result;
    use command_group::CommandGroup;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::process_helper::{get_process_group_pids, process_exists};

    /// Assert that certain process id no longer exists
    fn process_is_gone(pid: u32) -> bool {
        !process_exists(pid)
    }

    #[test]
    fn test_spawn_command() {
        let mut child = compile_shell_command("sleep 0.1")
            .group_spawn()
            .expect("Failed to spawn echo");

        let ecode = child.wait().expect("failed to wait on echo");

        assert!(ecode.success());
    }

    #[test]
    /// Ensure a `sh -c` command will be properly killed without detached processes.
    fn test_shell_command_is_killed() -> Result<()> {
        let mut child =
            compile_shell_command("sleep 60 & bash -c sleep 60 && echo 'this is a test'")
                .group_spawn()
                .expect("Failed to spawn echo");
        let pid: i32 = child.id().try_into().unwrap();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // Get all child processes, so we can make sure they no longer exist afterwards.
        // The process group id is the same as the parent process id.
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 2);

        // Kill the process and make sure it'll be killed.
        assert!(kill_child(0, &mut child, true).is_ok());

        // Assert that the direct child (sh -c) has been killed.
        assert!(process_is_gone(pid as u32));

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        // Assert that all child processes have been killed.
        assert_eq!(get_process_group_pids(pid).len(), 0);

        Ok(())
    }

    #[test]
    /// Ensure a `sh -c` command will be properly killed without detached processes when using unix
    /// signals directly.
    fn test_shell_command_is_killed_with_signal() -> Result<()> {
        let mut child = compile_shell_command("sleep 60 & sleep 60 && echo 'this is a test'")
            .group_spawn()
            .expect("Failed to spawn echo");
        let pid: i32 = child.id().try_into().unwrap();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // Get all child processes, so we can make sure they no longer exist afterwards.
        // The process group id is the same as the parent process id.
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 3);

        // Kill the process and make sure it'll be killed.
        send_signal_to_child(&mut child, Signal::SIGKILL, true).unwrap();

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        // Assert that the direct child (sh -c) has been killed.
        assert!(process_is_gone(pid as u32));

        // Assert that all child processes have been killed.
        assert_eq!(get_process_group_pids(pid).len(), 0);

        Ok(())
    }

    #[test]
    /// Ensure that a `sh -c` process with a child process that has children of its own
    /// will properly kill all processes and their children's children without detached processes.
    fn test_shell_command_children_are_killed() -> Result<()> {
        let mut child = compile_shell_command("bash -c 'sleep 60 && sleep 60' && sleep 60")
            .group_spawn()
            .expect("Failed to spawn echo");
        let pid: i32 = child.id().try_into().unwrap();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // Get all child processes, so we can make sure they no longer exist afterwards.
        // The process group id is the same as the parent process id.
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 3);

        // Kill the process and make sure its childen will be killed.
        assert!(kill_child(0, &mut child, true).is_ok());

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        // Assert that the direct child (sh -c) has been killed.
        assert!(process_is_gone(pid as u32));

        // Assert that all child processes have been killed.
        assert_eq!(get_process_group_pids(pid).len(), 0);

        Ok(())
    }

    #[test]
    /// Ensure a normal command without `sh -c` will be killed.
    fn test_normal_command_is_killed() -> Result<()> {
        let mut child = Command::new("sleep")
            .arg("60")
            .group_spawn()
            .expect("Failed to spawn echo");
        let pid: i32 = child.id().try_into().unwrap();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // No further processes exist in the group
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 1);

        // Kill the process and make sure it'll be killed.
        assert!(kill_child(0, &mut child, false).is_ok());

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        assert!(process_is_gone(pid as u32));

        Ok(())
    }

    #[test]
    /// Ensure a normal command and all its children will be
    /// properly killed without any detached processes.
    fn test_normal_command_children_are_killed() -> Result<()> {
        let mut child = Command::new("bash")
            .arg("-c")
            .arg("sleep 60 & sleep 60 && sleep 60")
            .group_spawn()
            .expect("Failed to spawn echo");
        let pid: i32 = child.id().try_into().unwrap();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // Get all child processes, so we can make sure they no longer exist afterwards.
        // The process group id is the same as the parent process id.
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 3);

        // Kill the process and make sure it'll be killed.
        assert!(kill_child(0, &mut child, true).is_ok());

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        // Assert that the direct child (sh -c) has been killed.
        assert!(process_is_gone(pid as u32));

        // Assert that all child processes have been killed.
        assert_eq!(get_process_group_pids(pid).len(), 0);

        Ok(())
    }
}
