#[cfg(not(target_os = "windows"))]
mod helper;

#[cfg(not(target_os = "windows"))]
mod tests {
    use color_eyre::Result;
    use pretty_assertions::assert_eq;
    use serde_cbor::de::from_slice;
    use serde_cbor::ser::to_vec;
    use tokio::task;

    use pueue_lib::network::message::*;
    use pueue_lib::network::protocol::*;

    use super::*;

    /// This tests whether we can create a listener and client, that communicate via unix sockets.
    #[tokio::test]
    async fn test_unix_socket() -> Result<()> {
        better_panic::install();
        let (shared_settings, _tempdir) = helper::get_shared_settings(true);

        let listener = get_listener(&shared_settings).await?;
        let message = create_success_message("This is a test");
        let original_bytes = to_vec(&message).expect("Failed to serialize message.");

        // Spawn a sub thread that:
        // 1. Accepts a new connection
        // 2. Reads a message
        // 3. Sends the same message back
        task::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            let message_bytes = receive_bytes(&mut stream).await.unwrap();

            let message: Message = from_slice(&message_bytes).unwrap();

            send_message(message, &mut stream).await.unwrap();
        });

        let mut client = get_client_stream(&shared_settings).await?;

        // Create a client that sends a message and instantly receives it
        send_message(message, &mut client).await?;
        let response_bytes = receive_bytes(&mut client).await?;
        let _message: Message = from_slice(&response_bytes)?;

        assert_eq!(response_bytes, original_bytes);

        Ok(())
    }
}
