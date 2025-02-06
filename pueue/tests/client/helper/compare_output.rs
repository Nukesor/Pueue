use crate::prelude::*;

use std::collections::HashMap;
use std::process::Output;

use chrono::Local;
use handlebars::Handlebars;
use std::fs::read_to_string;
use std::path::PathBuf;

use pueue_lib::settings::*;
use pueue_lib::task::TaskStatus;

use crate::helper::get_state;

/// Read the current state and extract the tasks' info into a context.
pub async fn get_task_context(settings: &Settings) -> Result<HashMap<String, String>> {
    // Get the current state
    let state = get_state(&settings.shared).await?;

    let mut context = HashMap::new();

    // Get the current daemon cwd.
    context.insert(
        "cwd".to_string(),
        settings
            .shared
            .pueue_directory()
            .to_string_lossy()
            .to_string(),
    );

    for (id, task) in state.tasks {
        let task_name = format!("task_{id}");

        let (start, end) = task.start_and_end();

        if let Some(start) = start {
            // Use datetime format for datetimes that aren't today.
            let format = if start.date_naive() == Local::now().date_naive() {
                &settings.client.status_time_format
            } else {
                &settings.client.status_datetime_format
            };

            let formatted = start.format(format).to_string();
            context.insert(format!("{task_name}_start"), formatted);
            context.insert(format!("{task_name}_start_long"), start.to_rfc2822());
        }
        if let Some(end) = end {
            // Use datetime format for datetimes that aren't today.
            let format = if end.date_naive() == Local::now().date_naive() {
                &settings.client.status_time_format
            } else {
                &settings.client.status_datetime_format
            };

            let formatted = end.format(format).to_string();
            context.insert(format!("{task_name}_end"), formatted);
            context.insert(format!("{task_name}_end_long"), end.to_rfc2822());
        }
        if let Some(label) = &task.label {
            context.insert(format!("{task_name}_label"), label.to_string());
        }

        if let TaskStatus::Stashed {
            enqueue_at: Some(enqueue_at),
        } = task.status
        {
            // Use datetime format for datetimes that aren't today.
            let format = if enqueue_at.date_naive() == Local::now().date_naive() {
                &settings.client.status_time_format
            } else {
                &settings.client.status_datetime_format
            };

            let enqueue_at = enqueue_at.format(format);
            context.insert(format!("{task_name}_enqueue_at"), enqueue_at.to_string());
        }
    }

    Ok(context)
}

/// This function takes the name of a snapshot template, applies a given context to the template
/// and compares it with a given process's `stdout`.
pub fn assert_template_matches(
    name: &str,
    output: Output,
    context: HashMap<String, String>,
) -> Result<()> {
    let mut combined_output = output.stderr.clone();
    combined_output.append(&mut output.stdout.clone());

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("client")
        .join("_templates")
        .join(name);

    let actual = String::from_utf8(combined_output).context("Got invalid utf8 as output!")?;

    let Ok(mut expected) = read_to_string(&path) else {
        println!("Actual output:\n{actual}");
        bail!("Failed to read template file {path:?}")
    };

    // Handle the snapshot as a template, if there's some context.
    if !context.is_empty() {
        // Init Handlebars. We set to strict, as we want to show an error on missing variables.
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);

        expected = handlebars
            .render_template(&expected, &context)
            .context(format!(
                "Failed to render template for file: {name} with context {context:?}"
            ))?;
    }

    assert_strings_match(expected, actual)?;

    Ok(())
}

/// Convenience wrapper to compare process stdout with snapshots.
pub fn assert_snapshot_matches_output(name: &str, stdout: Vec<u8>) -> Result<()> {
    let actual = String::from_utf8(stdout).context("Got invalid utf8 as stdout!")?;
    assert_snapshot_matches(name, actual)
}

/// This function takes the name of a snapshot and ensures that it is the same as the actual
/// provided string.
pub fn assert_snapshot_matches(name: &str, actual: String) -> Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("client")
        .join("_snapshots")
        .join(name);

    let Ok(expected) = read_to_string(&path) else {
        println!("Actual output:\n{actual}");
        bail!("Failed to read template file {path:?}")
    };

    assert_strings_match(expected, actual)?;

    Ok(())
}

/// Check whether two outputs are identical.
/// For convenience purposes, we trim trailing whitespaces.
pub fn assert_strings_match(mut expected: String, mut actual: String) -> Result<()> {
    expected = expected
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect::<Vec<String>>()
        .join("\n");

    actual = actual
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect::<Vec<String>>()
        .join("\n");

    if expected != actual {
        println!("Expected output:\n-----\n{expected}\n-----");
        println!("\nGot output:\n-----\n{actual}\n-----");
        println!(
            "\n{}",
            similar_asserts::SimpleDiff::from_str(&expected, &actual, "expected", "actual")
        );
        bail!("The stdout of the command doesn't match the expected string");
    }

    Ok(())
}
