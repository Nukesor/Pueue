use std::path::PathBuf;

use clap::{CommandFactory, Parser};
use clap_complete::{generate, generate_to, shells};
use color_eyre::eyre::{bail, WrapErr};
use color_eyre::Result;

use pueue_lib::settings::Settings;

use pueue::client::cli::{CliArguments, Shell, SubCommand};
use pueue::client::client::Client;

/// This is the main entry point of the client.
///
/// At first we do some basic setup:
/// - Parse the cli
/// - Initialize logging
/// - Read the config
///
/// Once all this is done, we init the [Client] struct and start the main loop via [Client::start].
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;

    // Parse commandline options.
    let opt = CliArguments::parse();

    // In case the user requested the generation of shell completion file, create it and exit.
    if let Some(SubCommand::Completions {
        shell,
        output_directory,
    }) = &opt.cmd
    {
        return create_shell_completion_file(shell, output_directory);
    }

    // Init the logger and set the verbosity level depending on the `-v` flags.
    pueue::tracing::install_tracing(opt.verbose)?;

    // Try to read settings from the configuration file.
    let (mut settings, config_found) =
        Settings::read(&opt.config).wrap_err("Failed to read configuration.")?;

    // Load any requested profile.
    if let Some(profile) = &opt.profile {
        settings.load_profile(profile)?;
    }

    // Error if no configuration file can be found, as this is an indicator, that the daemon hasn't
    // been started yet.
    if !config_found {
        bail!("Couldn't find a configuration file. Did you start the daemon yet?");
    }

    // Create client to talk with the daemon and connect.
    let mut client = Client::new(settings, opt)
        .await
        .context("Failed to initialize client.")?;
    client.start().await?;

    Ok(())
}

/// [clap] is capable of creating auto-generated shell completion files.
/// This function creates such a file for one of the supported shells and puts it into the
/// specified output directory.
fn create_shell_completion_file(shell: &Shell, output_directory: &Option<PathBuf>) -> Result<()> {
    let mut app = CliArguments::command();
    app.set_bin_name("pueue");

    // Output a completion file to a directory, if one is provided
    if let Some(output_directory) = output_directory {
        let completion_result = match shell {
            Shell::Bash => generate_to(shells::Bash, &mut app, "pueue", output_directory),
            Shell::Elvish => generate_to(shells::Elvish, &mut app, "pueue", output_directory),
            Shell::Fish => generate_to(shells::Fish, &mut app, "pueue", output_directory),
            Shell::PowerShell => {
                generate_to(shells::PowerShell, &mut app, "pueue", output_directory)
            }
            Shell::Zsh => generate_to(shells::Zsh, &mut app, "pueue", output_directory),
            Shell::Nushell => generate_to(
                clap_complete_nushell::Nushell,
                &mut app,
                "pueue",
                output_directory,
            ),
        };
        completion_result.context(format!("Failed to generate completions for {shell:?}"))?;

        return Ok(());
    }

    // Print the completion file to stdout
    let mut stdout = std::io::stdout();
    match shell {
        Shell::Bash => generate(shells::Bash, &mut app, "pueue", &mut stdout),
        Shell::Elvish => generate(shells::Elvish, &mut app, "pueue", &mut stdout),
        Shell::Fish => generate(shells::Fish, &mut app, "pueue", &mut stdout),
        Shell::PowerShell => generate(shells::PowerShell, &mut app, "pueue", &mut stdout),
        Shell::Zsh => generate(shells::Zsh, &mut app, "pueue", &mut stdout),
        Shell::Nushell => generate(
            clap_complete_nushell::Nushell,
            &mut app,
            "pueue",
            &mut stdout,
        ),
    };

    Ok(())
}
