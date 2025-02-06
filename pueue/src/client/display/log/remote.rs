use crate::prelude::*;

use std::io;

use crossterm::style::{Attribute, Color};
use snap::read::FrameDecoder;

use pueue_lib::network::message::TaskLogMessage;

use super::OutputStyle;

/// Prints log output received from the daemon.
/// We can safely call .unwrap() on output in here, since this
/// branch is always called after ensuring that it is `Some`.
pub fn print_remote_log(task_log: &TaskLogMessage, style: &OutputStyle, lines: Option<usize>) {
    if let Some(bytes) = task_log.output.as_ref() {
        if !bytes.is_empty() {
            // Add a hint if we should limit the output to X lines **and** there are actually more
            // lines than that given limit.
            let mut line_info = String::new();
            if !task_log.output_complete {
                line_info = lines.map_or(String::new(), |lines| format!(" (last {lines} lines)"));
            }

            // Print a newline between the task information and the first output.
            let header = style.style_text("output:", Some(Color::Green), Some(Attribute::Bold));
            println!("\n{header}{line_info}");

            if let Err(err) = decompress_and_print_remote_log(bytes) {
                eprintln!("Error while parsing stdout: {err}");
            }
        }
    }
}

/// We cannot easily stream log output from the client to the daemon (yet).
/// Right now, the output is compressed in the daemon and sent as a single payload to the
/// client. In here, we take that payload, decompress it and stream it it directly to stdout.
fn decompress_and_print_remote_log(bytes: &[u8]) -> Result<()> {
    let mut decompressor = FrameDecoder::new(bytes);

    let stdout = io::stdout();
    let mut write = stdout.lock();
    io::copy(&mut decompressor, &mut write)?;

    Ok(())
}
