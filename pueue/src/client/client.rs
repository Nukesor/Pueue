use crate::prelude::*;

use std::env::{current_dir, vars};
use std::io::{self, stdout, Write};
use std::{borrow::Cow, collections::HashMap};

use clap::crate_version;
use crossterm::tty::IsTty;

use pueue_lib::format::format_datetime;
use pueue_lib::network::message::*;
use pueue_lib::network::protocol::*;
use pueue_lib::network::secret::read_shared_secret;
use pueue_lib::settings::Settings;
use pueue_lib::state::PUEUE_DEFAULT_GROUP;

use crate::client::cli::{CliArguments, ColorChoice, GroupCommand, SubCommand};
use crate::client::commands::*;
use crate::client::display::*;

use super::cli::EnvCommand;

/// This struct contains the base logic for the client.
/// The client is responsible for connecting to the daemon, sending instructions
/// and interpreting their responses.
///
/// Most commands are a simple ping-pong. However, some commands require a more complex
/// communication pattern, such as the `follow` command, which can read local files,
/// or the `edit` command, which needs to open an editor.
pub struct Client {
    subcommand: SubCommand,
    settings: Settings,
    style: OutputStyle,
    stream: GenericStream,
}

/// This is a small helper which either returns a given group or the default group.
pub fn group_or_default(group: &Option<String>) -> String {
    group
        .clone()
        .unwrap_or_else(|| PUEUE_DEFAULT_GROUP.to_string())
}

/// This is a small helper which determines a task selection depending on
/// given commandline parameters.
/// I.e. whether the default group, a set of tasks or a specific group should be selected.
/// `start`, `pause` and `kill` can target either of these three selections.
///
/// If no parameters are given, it returns to the default group.
pub fn selection_from_params(
    all: bool,
    group: Option<String>,
    task_ids: Vec<usize>,
) -> TaskSelection {
    if all {
        TaskSelection::All
    } else if let Some(group) = group {
        TaskSelection::Group(group)
    } else if !task_ids.is_empty() {
        TaskSelection::TaskIds(task_ids)
    } else {
        TaskSelection::Group(PUEUE_DEFAULT_GROUP.into())
    }
}

impl Client {
    /// Initialize a new client.
    /// This includes establishing a connection to the daemon:
    ///     - Connect to the daemon.
    ///     - Authorize via secret.
    ///     - Check versions incompatibilities.
    pub async fn new(settings: Settings, opt: CliArguments) -> Result<Self> {
        // Connect to daemon and get stream used for communication.
        let mut stream = get_client_stream(&settings.shared)
            .await
            .context("Failed to initialize stream.")?;

        // Next we do a handshake with the daemon
        // 1. Client sends the secret to the daemon.
        // 2. If successful, the daemon responds with their version.
        let secret = read_shared_secret(&settings.shared.shared_secret_path())?;
        send_bytes(&secret, &mut stream)
            .await
            .context("Failed to send secret.")?;

        // Receive and parse the response. We expect the daemon's version as UTF-8.
        let version_bytes = receive_bytes(&mut stream)
            .await
            .context("Failed to receive version during handshake with daemon.")?;
        if version_bytes.is_empty() {
            bail!("Daemon went away after sending secret. Did you use the correct secret?")
        }
        let version = match String::from_utf8(version_bytes) {
            Ok(version) => version,
            Err(_) => {
                bail!("Daemon sent invalid UTF-8. Did you use the correct secret?")
            }
        };

        // Info if the daemon runs a different version.
        // Backward compatibility should work, but some features might not work as expected.
        if version != crate_version!() {
            // Only show warnings if we aren't supposed to output json.
            let show_warning = if let Some(subcommand) = &opt.cmd {
                match subcommand {
                    SubCommand::Status { json, .. } => !json,
                    SubCommand::Log { json, .. } => !json,
                    SubCommand::Group { json, .. } => !json,
                    _ => true,
                }
            } else {
                true
            };

            if show_warning {
                warn!(
                    "Different daemon version detected '{version}'. Consider restarting the daemon."
                );
            }
        }

        // Determine whether we should color/style our output or not.
        // The user can explicitly disable/enable this, otherwise we check whether we are on a TTY.
        let style_enabled = match opt.color {
            ColorChoice::Auto => stdout().is_tty(),
            ColorChoice::Always => true,
            ColorChoice::Never => false,
        };
        let style = OutputStyle::new(&settings, style_enabled);

        // Determine the subcommand that has been called by the user.
        // If no subcommand is given, we default to the `status` subcommand without any arguments.
        let subcommand = opt.cmd.unwrap_or(SubCommand::Status {
            json: false,
            group: None,
            query: Vec::new(),
        });

        Ok(Client {
            settings,
            style,
            stream,
            subcommand,
        })
    }

    /// This is the function where the actual communication and logic starts.
    /// At this point everything is initialized, the connection is up and
    /// we can finally start doing stuff.
    ///
    /// The command handling is split into "simple" and "complex" commands.
    pub async fn start(&mut self) -> Result<()> {
        // Return early, if the command has already been handled.
        if self.handle_complex_command(self.subcommand.clone()).await? {
            return Ok(());
        }

        // The handling of "generic" commands is encapsulated in this function.
        self.handle_simple_command(self.subcommand.clone()).await?;

        Ok(())
    }

    /// Handle all complex client-side functionalities.
    /// Some functionalities need special handling and are contained in their own functions
    /// with their own communication code.
    /// Some examples for special handling includes
    /// - reading local files
    /// - sending multiple messages
    /// - interacting with other programs
    ///
    /// Returns `Ok(true)`, if the current command has been handled by this function.
    /// This indicates that the client can now shut down.
    /// If `Ok(false)` is returned, the client will continue and handle the Subcommand in the
    /// [Client::handle_simple_command] function.
    async fn handle_complex_command(&mut self, subcommand: SubCommand) -> Result<bool> {
        // This match handles all "complex" commands.
        match subcommand {
            SubCommand::Add {
                mut command,
                working_directory,
                escape,
                start_immediately,
                stashed,
                group,
                delay_until,
                dependencies,
                priority,
                label,
                print_task_id,
                follow,
            } => {
                // Either take the user-specified path or default to the current working directory.
                let path = working_directory
                    .as_ref()
                    .map(|path| Ok(path.clone()))
                    .unwrap_or_else(current_dir)?;

                // The user can request to escape any special shell characters in all parameter strings before
                // we concatenated them to a single string.
                if escape {
                    command = command
                        .iter()
                        .map(|parameter| shell_escape::escape(Cow::from(parameter)).into_owned())
                        .collect();
                }

                // Add the message to the daemon.
                let message = Message::Add(AddMessage {
                    command: command.join(" "),
                    path,
                    // Catch the current environment for later injection into the task's process.
                    envs: HashMap::from_iter(vars()),
                    start_immediately,
                    stashed,
                    group: group_or_default(&group),
                    enqueue_at: delay_until,
                    dependencies,
                    priority,
                    label,
                });
                send_message(message, &mut self.stream).await?;

                // Get the response from the daemon.
                let response = receive_message(&mut self.stream).await?;

                // Make sure the task has been added, otherwise handle the response and return.
                let Message::AddedTask(AddedTaskMessage {
                    task_id,
                    enqueue_at,
                    group_is_paused,
                }) = response
                else {
                    self.handle_response(response)?;
                    return Ok(true);
                };

                let mut output = if print_task_id {
                    // Only print the task id if that was requested.
                    format!("{task_id}")
                } else if let Some(enqueue_at) = enqueue_at {
                    let enqueue_at = format_datetime(&self.settings, &enqueue_at);
                    format!("New task added (id {task_id}). It will be enqueued at {enqueue_at}")
                } else {
                    format!("New task added (id {task_id}).")
                };

                // Also notify the user if the task's group is paused
                if !print_task_id && group_is_paused && !follow {
                    output.push_str("\nThe group of this task is currently paused!");
                }

                // If we were to follow the task immediately, print the task info to `stderr` so
                // that the actual log output may be piped into another command.
                if follow {
                    eprintln!("{output}");
                } else {
                    println!("{output}");
                }

                if follow {
                    // If we're supposed to read the log files from the local system, we don't have to
                    // do any communication with the daemon.
                    // Thereby we handle this in a separate function.
                    if self.settings.client.read_local_logs {
                        local_follow(
                            &mut self.stream,
                            &self.settings.shared.pueue_directory(),
                            Some(task_id),
                            None,
                        )
                        .await?;
                        return Ok(true);
                    } else {
                        // In case we need to follow the daemon, go ahead and overwrite
                        // `self.subcommand` with a `Follow` subcommand.
                        // Afterwards, return an `Ok(false)`, which signals the client that it
                        // should continue processing `self.subcommand`.
                        // This is pretty hacky. TODO: Refactor the whole client at some point.
                        self.subcommand = SubCommand::Follow {
                            task_id: Some(task_id),
                            lines: None,
                        };
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            SubCommand::Reset { force, groups } => {
                // Get the current state and check if there're any running tasks.
                // If there are, ask the user if they really want to reset the state.
                let state = get_state(&mut self.stream).await?;

                // Get the groups that should be reset.
                let groups: Vec<String> = if groups.is_empty() {
                    state.groups.keys().cloned().collect()
                } else {
                    groups
                };

                // Check if there're any running tasks for that group
                let running_tasks = state
                    .tasks
                    .iter()
                    .filter(|(_id, task)| groups.contains(&task.group))
                    .filter_map(|(id, task)| if task.is_running() { Some(*id) } else { None })
                    .collect::<Vec<_>>();

                if !running_tasks.is_empty() && !force {
                    self.handle_user_confirmation("remove running tasks", &running_tasks)?;
                }

                // Now that we got the user's consent, we return `false` and let the
                // `handle_simple_command` function process the subcommand as usual to send
                // a `reset` message to the daemon.
                Ok(false)
            }

            SubCommand::Edit { task_ids } => {
                let message = edit(&mut self.stream, &self.settings, task_ids).await?;
                self.handle_response(message)?;
                Ok(true)
            }
            SubCommand::Wait {
                task_ids,
                group,
                all,
                quiet,
                status,
            } => {
                let selection = selection_from_params(all, group, task_ids);
                wait(&mut self.stream, &self.style, selection, quiet, status).await?;
                Ok(true)
            }
            SubCommand::Restart {
                task_ids,
                all_failed,
                failed_in_group,
                start_immediately,
                stashed,
                in_place,
                not_in_place,
                edit,
            } => {
                // `not_in_place` superseeds both other configs
                let in_place = (self.settings.client.restart_in_place || in_place) && !not_in_place;
                restart(
                    &mut self.stream,
                    &self.settings,
                    task_ids,
                    all_failed,
                    failed_in_group,
                    start_immediately,
                    stashed,
                    in_place,
                    edit,
                )
                .await?;
                Ok(true)
            }
            SubCommand::Follow { task_id, lines } => {
                // If we're supposed to read the log files from the local system, we don't have to
                // do any communication with the daemon.
                // Thereby we handle this in a separate function.
                if self.settings.client.read_local_logs {
                    local_follow(
                        &mut self.stream,
                        &self.settings.shared.pueue_directory(),
                        task_id,
                        lines,
                    )
                    .await?;
                    return Ok(true);
                }
                // Otherwise, we forward this to the `handle_simple_command` function.
                Ok(false)
            }
            SubCommand::FormatStatus { .. } => {
                format_state(
                    &mut self.stream,
                    &self.subcommand,
                    &self.style,
                    &self.settings,
                )
                .await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Handle logic that's super generic on the client-side.
    /// This (almost) always follows a singular ping-pong pattern.
    /// One message to the daemon, one response, done.
    ///
    /// The only exception is streaming of log output.
    /// In that case, we send one request and contine receiving until the stream shuts down.
    async fn handle_simple_command(&mut self, subcommand: SubCommand) -> Result<()> {
        // Create the message that should be sent to the daemon
        // depending on the given commandline options.
        let message = self.get_message_from_cmd(subcommand)?;

        // Create the message payload and send it to the daemon.
        send_message(message, &mut self.stream).await?;

        // Check if we can receive the response from the daemon
        let mut response = receive_message(&mut self.stream).await?;

        // Handle the message.
        // In some scenarios, such as log streaming, we should continue receiving messages
        // from the daemon, which is why we have a while loop in place.
        while self.handle_response(response)? {
            response = receive_message(&mut self.stream).await?;
        }

        Ok(())
    }

    /// Most returned messages can be handled in a generic fashion.
    /// However, some commands require to continuously receive messages (streaming).
    ///
    /// If this function returns `Ok(true)`, the parent function will continue to receive
    /// and handle messages from the daemon. Otherwise the client will simply exit.
    fn handle_response(&self, message: Message) -> Result<bool> {
        match message {
            Message::Success(text) => print_success(&self.style, &text),
            Message::Failure(text) => {
                print_error(&self.style, &text);
                std::process::exit(1);
            }
            Message::StatusResponse(state) => {
                let tasks = state.tasks.values().cloned().collect();
                let output =
                    print_state(*state, tasks, &self.subcommand, &self.style, &self.settings)?;
                println!("{output}");
            }
            Message::LogResponse(task_logs) => {
                print_logs(task_logs, &self.subcommand, &self.style, &self.settings)
            }
            Message::GroupResponse(groups) => {
                let group_text = format_groups(groups, &self.subcommand, &self.style);
                println!("{group_text}");
            }
            Message::Stream(text) => {
                print!("{text}");
                io::stdout().flush().unwrap();
                return Ok(true);
            }
            Message::Close => return Ok(false),
            _ => error!("Received unhandled response message"),
        };

        Ok(false)
    }

    /// Prints a warning and prompt for a given action and tasks.
    /// Returns `Ok(())` if the action was confirmed.
    fn handle_user_confirmation(&self, action: &str, task_ids: &[usize]) -> Result<()> {
        // printing warning and prompt
        let task_ids = task_ids
            .iter()
            .map(|t| format!("task{t}"))
            .collect::<Vec<String>>()
            .join(", ");
        eprintln!("You are trying to {action}: {task_ids}",);

        let mut input = String::new();

        loop {
            print!("Do you want to continue [Y/n]: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;

            match input.chars().next().unwrap() {
                'N' | 'n' => {
                    eprintln!("Aborted!");
                    std::process::exit(1);
                }
                '\n' | 'Y' | 'y' => {
                    break;
                }
                _ => {
                    continue;
                }
            }
        }

        Ok(())
    }

    /// Convert the cli command into the message that's being sent to the server,
    /// so it can be understood by the daemon.
    ///
    /// This function is pretty large, but it consists mostly of simple conversions
    /// of [SubCommand] variant to a [Message] variant.
    fn get_message_from_cmd(&self, subcommand: SubCommand) -> Result<Message> {
        Ok(match subcommand {
            SubCommand::Remove { task_ids } => {
                if self.settings.client.show_confirmation_questions {
                    self.handle_user_confirmation("remove", &task_ids)?;
                }
                Message::Remove(task_ids)
            }
            SubCommand::Stash {
                task_ids,
                group,
                all,
                delay_until,
            } => {
                let selection = selection_from_params(all, group, task_ids);
                StashMessage {
                    tasks: selection,
                    enqueue_at: delay_until,
                }
                .into()
            }
            SubCommand::Switch {
                task_id_1,
                task_id_2,
            } => SwitchMessage {
                task_id_1,
                task_id_2,
            }
            .into(),
            SubCommand::Enqueue {
                task_ids,
                group,
                all,
                delay_until,
            } => {
                let selection = selection_from_params(all, group, task_ids);
                EnqueueMessage {
                    tasks: selection,
                    enqueue_at: delay_until,
                }
            }
            .into(),
            SubCommand::Start {
                task_ids,
                group,
                all,
                ..
            } => StartMessage {
                tasks: selection_from_params(all, group, task_ids),
            }
            .into(),
            SubCommand::Pause {
                task_ids,
                group,
                wait,
                all,
                ..
            } => PauseMessage {
                tasks: selection_from_params(all, group, task_ids),
                wait,
            }
            .into(),
            SubCommand::Kill {
                task_ids,
                group,
                all,
                signal,
                ..
            } => {
                if self.settings.client.show_confirmation_questions {
                    self.handle_user_confirmation("kill", &task_ids)?;
                }
                KillMessage {
                    tasks: selection_from_params(all, group, task_ids),
                    signal,
                }
                .into()
            }
            SubCommand::Send { task_id, input } => SendMessage { task_id, input }.into(),
            SubCommand::Env { cmd } => Message::from(match cmd {
                EnvCommand::Set {
                    task_id,
                    key,
                    value,
                } => EnvMessage::Set {
                    task_id,
                    key,
                    value,
                },
                EnvCommand::Unset { task_id, key } => EnvMessage::Unset { task_id, key },
            }),

            SubCommand::Group { cmd, .. } => match cmd {
                Some(GroupCommand::Add { name, parallel }) => GroupMessage::Add {
                    name: name.to_owned(),
                    parallel_tasks: parallel.to_owned(),
                },
                Some(GroupCommand::Remove { name }) => GroupMessage::Remove(name.to_owned()),
                None => GroupMessage::List,
            }
            .into(),
            SubCommand::Status { .. } => Message::Status,
            SubCommand::Log {
                task_ids,
                lines,
                group,
                full,
                all,
                ..
            } => {
                let lines = determine_log_line_amount(full, &lines);
                let selection = selection_from_params(all, group, task_ids);

                let message = LogRequestMessage {
                    tasks: selection,
                    send_logs: !self.settings.client.read_local_logs,
                    lines,
                };
                Message::Log(message)
            }
            SubCommand::Follow { task_id, lines } => StreamRequestMessage { task_id, lines }.into(),
            SubCommand::Clean {
                successful_only,
                group,
            } => CleanMessage {
                successful_only,
                group,
            }
            .into(),
            SubCommand::Reset { force, groups, .. } => {
                if self.settings.client.show_confirmation_questions && !force {
                    self.handle_user_confirmation("reset", &Vec::new())?;
                }

                let target = if groups.is_empty() {
                    ResetTarget::All
                } else {
                    ResetTarget::Groups(groups.clone())
                };

                ResetMessage { target }.into()
            }
            SubCommand::Shutdown => Shutdown::Graceful.into(),
            SubCommand::Parallel {
                parallel_tasks,
                group,
            } => match parallel_tasks {
                Some(parallel_tasks) => {
                    let group = group_or_default(&group);
                    ParallelMessage {
                        parallel_tasks,
                        group,
                    }
                    .into()
                }
                None => GroupMessage::List.into(),
            },
            SubCommand::Add { .. } => bail!("Add has to be handled earlier"),
            SubCommand::FormatStatus { .. } => bail!("FormatStatus has to be handled earlier"),
            SubCommand::Completions { .. } => bail!("Completions have to be handled earlier"),
            SubCommand::Restart { .. } => bail!("Restarts have to be handled earlier"),
            SubCommand::Edit { .. } => bail!("Edits have to be handled earlier"),
            SubCommand::Wait { .. } => bail!("Wait has to be handled earlier"),
        })
    }
}
