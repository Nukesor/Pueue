use crate::prelude::*;

use std::time::{Duration, SystemTime};

use clap::crate_version;
use tokio::time::sleep;

use pueue_lib::error::Error;
use pueue_lib::network::message::*;
use pueue_lib::network::protocol::*;
use pueue_lib::network::secret::read_shared_secret;
use pueue_lib::settings::Settings;
use pueue_lib::state::SharedState;

use crate::daemon::network::message_handler::handle_message;
use crate::daemon::process_handler::initiate_shutdown;

use super::message_handler::follow_log;

/// Listen for new connections on the socket.
/// On a new connection, the connected stream will be handled in a separate tokio task.
/// See [handle_incoming] for the actual connection handler function.
pub async fn accept_incoming(settings: Settings, state: SharedState) -> Result<()> {
    let listener = get_listener(&settings.shared).await?;
    // Read secret once to prevent multiple disk reads.
    let secret = read_shared_secret(&settings.shared.shared_secret_path())?;

    loop {
        // Poll incoming connections.
        let stream = match listener.accept().await {
            Ok(stream) => stream,
            Err(err) => {
                warn!("Failed connecting to client: {err:?}");
                continue;
            }
        };

        // Start a new task for the request
        let state_clone = state.clone();
        let secret_clone = secret.clone();
        let settings_clone = settings.clone();
        tokio::spawn(async move {
            let _result = handle_incoming(stream, state_clone, settings_clone, secret_clone).await;
        });
    }
}

/// Handle a new connection from a client.
///
/// Pueue has a very simple protocol that needs to be followed.
/// 1. Client sends secret for authentication
/// 2. If secret is valid, the daemon sends its own version to the client.
/// 3. The Client sends the instruction message.
/// 4. The Daemon reads the instruction and acts upon it.
/// 5. The Daemon sends a response
///
/// There're two edge-cases where this pattern is not valid:
/// 1. Shutdown. In that case the message is sent first and the daemon shuts down afterwards.
/// 2. Streaming of logs. The Daemon will continuously send messages with log chunks until
///    the watched task finished or the client disconnects.
async fn handle_incoming(
    mut stream: GenericStream,
    state: SharedState,
    settings: Settings,
    secret: Vec<u8>,
) -> Result<()> {
    // Receive the secret once and check, whether the client is allowed to connect
    let payload_bytes = receive_bytes(&mut stream).await?;

    // Didn't receive any bytes. The client disconnected.
    if payload_bytes.is_empty() {
        info!("Client went away");
        return Ok(());
    }

    let start = SystemTime::now();

    // Return if we got a wrong secret from the client.
    if payload_bytes != secret {
        let received_secret = String::from_utf8(payload_bytes)?;
        warn!("Received invalid secret: {received_secret}");

        // Wait for 1 second before closing the socket, when getting a invalid secret.
        // This invalidates any timing attacks.
        let remaining_sleep_time = Duration::from_secs(1)
            - SystemTime::now()
                .duration_since(start)
                .context("Couldn't calculate duration. Did the system time change?")?;
        sleep(remaining_sleep_time).await;
        bail!("Received invalid secret");
    }

    // Send confirmation to the client, that the secret was valid.
    // This is also the current version of the daemon, so the client can inform user if the
    // daemon needs a restart in case of a version mismatch.
    send_bytes(crate_version!().as_bytes(), &mut stream).await?;

    loop {
        // Receive the actual instruction from the client
        let message_result = receive_message(&mut stream).await;

        if let Err(Error::EmptyPayload) = message_result {
            debug!("Client went away");
            return Ok(());
        }

        // In case of a deserialization error, respond the error to the client and return early.
        if let Err(Error::MessageDeserialization(err)) = message_result {
            send_message(
                create_failure_message(format!("Failed to deserialize message: {err}")),
                &mut stream,
            )
            .await?;
            return Ok(());
        }

        let message = message_result?;

        let response = match message {
            // The client requested the output of a task.
            // Since this involves streaming content, we have to do some special handling.
            Message::StreamRequest(message) => {
                let pueue_directory = settings.shared.pueue_directory();
                follow_log(&pueue_directory, &mut stream, &state, message).await?
            }
            // To initiated a shutdown, a flag in Pueue's state is set that informs the TaskHandler
            // to perform a graceful shutdown.
            //
            // However, this is an edge-case as we have respond to the client first.
            // Otherwise it might happen, that the daemon shuts down too fast and we aren't
            // capable of actually sending the message back to the client.
            Message::DaemonShutdown(shutdown_type) => {
                let response = create_success_message("Daemon is shutting down");
                send_message(response, &mut stream).await?;

                let mut state = state.lock().unwrap();
                initiate_shutdown(&settings, &mut state, shutdown_type);

                return Ok(());
            }
            _ => {
                // Process a normal message.
                handle_message(message, &state, &settings)
            }
        };

        // Respond to the client.
        send_message(response, &mut stream).await?;
    }
}
