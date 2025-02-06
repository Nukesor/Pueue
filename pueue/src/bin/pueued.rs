use std::process::Command;

use clap::Parser;
use color_eyre::Result;

use pueue::daemon::{cli::CliArguments, run};

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    color_eyre::install()?;

    // Parse commandline options.
    let opt = CliArguments::parse();

    if opt.daemonize {
        // Ordinarily this would be handled in clap, but they don't support conflicting specific args
        // with subcommands. We can't turn this off globally because -c and -p are valid args when using
        // subcommand to install the service
        #[cfg(target_os = "windows")]
        if opt.service.is_some() {
            eprintln!("daemonize flag cannot be used with service subcommand");
            return Ok(());
        }

        return fork_daemon(&opt);
    }

    // Set the verbosity level of the logger.
    pueue::tracing::install_tracing(opt.verbose)?;

    #[cfg(target_os = "windows")]
    {
        use pueue::daemon::cli::{ServiceSubcommand, ServiceSubcommandEntry};
        use pueue::daemon::service;

        if let Some(ServiceSubcommandEntry::Service(service)) = opt.service {
            match service {
                ServiceSubcommand::Run => {
                    // start service
                    service::run_service(opt.config.clone(), opt.profile.clone())?;
                    return Ok(());
                }

                ServiceSubcommand::Install => {
                    service::install_service(opt.config.clone(), opt.profile.clone())?;
                    println!("Successfully installed `pueued` Windows service");
                    return Ok(());
                }

                ServiceSubcommand::Uninstall => {
                    service::uninstall_service()?;
                    println!("Successfully uninstalled `pueued` Windows service");
                    return Ok(());
                }

                ServiceSubcommand::Start => {
                    service::start_service()?;
                    return Ok(());
                }

                ServiceSubcommand::Stop => {
                    service::stop_service()?;
                    return Ok(());
                }
            }
        }
    }

    run(opt.config, opt.profile, false).await
}

/// This is a simple and cheap custom fork method.
/// Simply spawn a new child with identical arguments and exit right away.
fn fork_daemon(opt: &CliArguments) -> Result<()> {
    let mut arguments = Vec::<String>::new();

    if let Some(config) = &opt.config {
        arguments.push("--config".to_string());
        arguments.push(config.to_string_lossy().into_owned());
    }

    if let Some(profile) = &opt.profile {
        arguments.push("--profile".to_string());
        arguments.push(profile.clone());
    }

    if opt.verbose > 0 {
        arguments.push("-".to_string() + &"v".repeat(opt.verbose as usize));
    }

    // Try to get the path to the current binary, since it may not be in the $PATH.
    // If we cannot detect it (for some unknown reason), fallback to the raw `pueued` binary name.
    let current_exe = if let Ok(path) = std::env::current_exe() {
        path.to_string_lossy().clone().to_string()
    } else {
        eprintln!("Couldn't detect path of current binary. Falling back to 'pueue' in $PATH");
        "pueued".to_string()
    };

    let mut command = Command::new(current_exe);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // CREATE_NO_WINDOW causes all children to not show a visible console window,
        // but it also apparently has the effect of starting a new process group.
        //
        // https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags#flags
        // https://stackoverflow.com/a/71364777/9423933
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[allow(clippy::zombie_processes)]
    command
        .args(&arguments)
        .spawn()
        .expect("Failed to fork new process.");

    println!("Pueued is now running in the background");
    Ok(())
}
