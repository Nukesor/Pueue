use crate::prelude::*;

use pueue_lib::network::message::*;
use pueue_lib::network::protocol::{
    get_client_stream, receive_bytes, receive_message, send_bytes,
    send_message as internal_send_message, GenericStream,
};
use pueue_lib::network::secret::read_shared_secret;
use pueue_lib::settings::Shared;

/// This is a small convenience wrapper that sends a message and immediately returns the response.
pub async fn send_message<T>(shared: &Shared, message: T) -> Result<Message>
where
    T: Into<Message>,
{
    let mut stream = get_authenticated_stream(shared).await?;

    // Check if we can receive the response from the daemon
    internal_send_message(message, &mut stream)
        .await
        .map_err(|err| eyre!("Failed to send message: {err}"))?;

    // Check if we can receive the response from the daemon
    receive_message(&mut stream)
        .await
        .map_err(|err| eyre!("Failed to receive message: {err}"))
}

/// Create a new stream that already finished the handshake and secret exchange.
///
/// Pueue creates a new socket stream for each command, which is why we do it the same way.
async fn get_authenticated_stream(shared: &Shared) -> Result<GenericStream> {
    // Connect to daemon and get stream used for communication.
    let mut stream = match get_client_stream(shared).await {
        Ok(stream) => stream,
        Err(err) => {
            panic!("Couldn't get client stream: {err}");
        }
    };

    // Next we do a handshake with the daemon
    // 1. Client sends the secret to the daemon.
    // 2. If successful, the daemon responds with their version.
    let secret =
        read_shared_secret(&shared.shared_secret_path()).context("Couldn't read shared secret.")?;
    send_bytes(&secret, &mut stream)
        .await
        .context("Failed to send bytes.")?;
    let version_bytes = receive_bytes(&mut stream)
        .await
        .context("Failed sending secret during handshake with daemon.")?;

    if version_bytes.is_empty() {
        bail!("Daemon went away after sending secret. Did you use the correct secret?")
    }

    Ok(stream)
}
