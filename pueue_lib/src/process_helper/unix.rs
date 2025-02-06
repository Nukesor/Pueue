use crate::prelude::*;

// We allow anyhow in here, as this is a module that'll be strictly used internally.
// As soon as it's obvious that this is code is intended to be exposed to library users, we have to
// go ahead and replace any `anyhow` usage by proper error handling via our own Error type.
use color_eyre::Result;
use command_group::{GroupChild, Signal, UnixChildExt};

use crate::settings::Settings;

pub fn get_shell_command(settings: &Settings) -> Vec<String> {
    let Some(ref shell_command) = settings.daemon.shell_command else {
        return vec![
            "sh".into(),
            "-c".into(),
            "{{ pueue_command_string }}".into(),
        ];
    };

    shell_command.clone()
}

/// Send a signal to one of Pueue's child process group handle.
pub fn send_signal_to_child<T>(child: &mut GroupChild, signal: T) -> Result<()>
where
    T: Into<Signal>,
{
    child.signal(signal.into())?;
    Ok(())
}

/// This is a helper function to safely kill a child process group.
/// Its purpose is to properly kill all processes and prevent any dangling processes.
pub fn kill_child(task_id: usize, child: &mut GroupChild) -> std::io::Result<()> {
    match child.kill() {
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
    use crate::prelude::*;

    use std::process::Command;
    use std::thread::sleep;
    use std::time::Duration;

    use color_eyre::Result;
    use command_group::CommandGroup;
    use libproc::processes::{pids_by_type, ProcFilter};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::process_helper::{compile_shell_command, process_exists};

    /// List all PIDs that are part of the process group
    pub fn get_process_group_pids(pgrp: u32) -> Vec<u32> {
        match pids_by_type(ProcFilter::ByProgramGroup { pgrpid: pgrp }) {
            Err(error) => {
                warn!("Failed to get list of processes in process group {pgrp}: {error}");
                Vec::new()
            }
            Ok(mut processes) => {
                // MacOS doesn't list the main process in this group
                if !processes.iter().any(|pid| pid == &pgrp) && !process_is_gone(pgrp) {
                    processes.push(pgrp)
                }
                processes
            }
        }
    }

    /// Assert that certain process id no longer exists
    fn process_is_gone(pid: u32) -> bool {
        !process_exists(pid)
    }

    #[test]
    fn test_spawn_command() {
        let settings = Settings::default();
        let mut child = compile_shell_command(&settings, "sleep 0.1")
            .group_spawn()
            .expect("Failed to spawn echo");

        let ecode = child.wait().expect("failed to wait on echo");

        assert!(ecode.success());
    }

    #[test]
    /// Ensure a `sh -c` command will be properly killed without detached processes.
    fn test_shell_command_is_killed() -> Result<()> {
        let settings = Settings::default();
        let mut child =
            compile_shell_command(&settings, "sleep 60 & sleep 60 && echo 'this is a test'")
                .group_spawn()
                .expect("Failed to spawn echo");
        let pid = child.id();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // Get all child processes, so we can make sure they no longer exist afterwards.
        // The process group id is the same as the parent process id.
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 3);

        // Kill the process and make sure it'll be killed.
        assert!(kill_child(0, &mut child).is_ok());

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        // Assert that the direct child (sh -c) has been killed.
        assert!(process_is_gone(pid));

        // Assert that all child processes have been killed.
        assert_eq!(get_process_group_pids(pid).len(), 0);

        Ok(())
    }

    #[test]
    /// Ensure a `sh -c` command will be properly killed without detached processes when using unix
    /// signals directly.
    fn test_shell_command_is_killed_with_signal() -> Result<()> {
        let settings = Settings::default();
        let mut child =
            compile_shell_command(&settings, "sleep 60 & sleep 60 && echo 'this is a test'")
                .group_spawn()
                .expect("Failed to spawn echo");
        let pid = child.id();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // Get all child processes, so we can make sure they no longer exist afterwards.
        // The process group id is the same as the parent process id.
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 3);

        // Kill the process and make sure it'll be killed.
        send_signal_to_child(&mut child, Signal::SIGKILL).unwrap();

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        // Assert that the direct child (sh -c) has been killed.
        assert!(process_is_gone(pid));

        // Assert that all child processes have been killed.
        assert_eq!(get_process_group_pids(pid).len(), 0);

        Ok(())
    }

    #[test]
    /// Ensure that a `sh -c` process with a child process that has children of its own
    /// will properly kill all processes and their children's children without detached processes.
    fn test_shell_command_children_are_killed() -> Result<()> {
        let settings = Settings::default();
        let mut child =
            compile_shell_command(&settings, "bash -c 'sleep 60 && sleep 60' && sleep 60")
                .group_spawn()
                .expect("Failed to spawn echo");
        let pid = child.id();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // Get all child processes, so we can make sure they no longer exist afterwards.
        // The process group id is the same as the parent process id.
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 3);

        // Kill the process and make sure its children will be killed.
        assert!(kill_child(0, &mut child).is_ok());

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        // Assert that the direct child (sh -c) has been killed.
        assert!(process_is_gone(pid));

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
        let pid = child.id();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // No further processes exist in the group
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 1);

        // Kill the process and make sure it'll be killed.
        assert!(kill_child(0, &mut child).is_ok());

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        assert!(process_is_gone(pid));

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
        let pid = child.id();
        // Sleep a little to give everything a chance to spawn.
        sleep(Duration::from_millis(500));

        // Get all child processes, so we can make sure they no longer exist afterwards.
        // The process group id is the same as the parent process id.
        let group_pids = get_process_group_pids(pid);
        assert_eq!(group_pids.len(), 3);

        // Kill the process and make sure it'll be killed.
        assert!(kill_child(0, &mut child).is_ok());

        // Sleep a little to give all processes time to shutdown.
        sleep(Duration::from_millis(500));
        // collect the exit status; otherwise the child process hangs around as a zombie.
        child.try_wait().unwrap_or_default();

        // Assert that the direct child (sh -c) has been killed.
        assert!(process_is_gone(pid));

        // Assert that all child processes have been killed.
        assert_eq!(get_process_group_pids(pid).len(), 0);

        Ok(())
    }
}
