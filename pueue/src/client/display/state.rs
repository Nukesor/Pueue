use crate::prelude::*;

use pueue_lib::settings::Settings;
use pueue_lib::state::{State, PUEUE_DEFAULT_GROUP};
use pueue_lib::task::Task;

use super::{helper::*, table_builder::TableBuilder, OutputStyle};
use crate::client::cli::SubCommand;
use crate::client::display::group::get_group_headline;
use crate::client::query::apply_query;

/// Get the output for the state of the daemon in a nicely formatted table.
/// If there are multiple groups, each group with a task will have its own table.
///
/// We pass the tasks as a separate parameter and as a list.
/// This allows us to print the tasks in the order passed to the `format-status` subcommand.
pub fn print_state(
    mut state: State,
    mut tasks: Vec<Task>,
    cli_command: &SubCommand,
    style: &OutputStyle,
    settings: &Settings,
) -> Result<String> {
    let mut output = String::new();

    let (json, group_only, query) = match cli_command {
        SubCommand::Status { json, group, query } => (*json, group.clone(), Some(query)),
        SubCommand::FormatStatus { group } => (false, group.clone(), None),
        _ => panic!("Got wrong Subcommand {cli_command:?} in print_state. This shouldn't happen!"),
    };

    let mut table_builder = TableBuilder::new(settings, style);

    if let Some(query) = query {
        let query_result = apply_query(&query.join(" "), &group_only)?;
        table_builder.set_visibility_by_rules(&query_result.selected_columns);
        tasks = query_result.apply_filters(tasks);
        tasks = query_result.order_tasks(tasks);
        tasks = query_result.limit_tasks(tasks);
    }

    // If the json flag is specified, print the state as json and exit.
    if json {
        if query.is_some() {
            state.tasks = tasks.into_iter().map(|task| (task.id, task)).collect();
        }
        output.push_str(&serde_json::to_string(&state).unwrap());
        return Ok(output);
    }

    if let Some(group) = group_only {
        print_single_group(state, tasks, style, group, table_builder, &mut output);
        return Ok(output);
    }

    print_all_groups(state, tasks, style, table_builder, &mut output);

    Ok(output)
}

/// The user requested only a single group to be displayed.
///
/// Print this group or show an error if this group doesn't exist.
fn print_single_group(
    state: State,
    tasks: Vec<Task>,
    style: &OutputStyle,
    group_name: String,
    table_builder: TableBuilder,
    output: &mut String,
) {
    // Sort all tasks by their respective group;
    let mut sorted_tasks = sort_tasks_by_group(tasks);

    let Some(group) = state.groups.get(&group_name) else {
        eprintln!("There exists no group \"{group_name}\"");
        return;
    };

    // Only a single group is requested. Print that group and return.
    let tasks = sorted_tasks.entry(group_name.clone()).or_default();
    let headline = get_group_headline(&group_name, group, style);
    output.push_str(&headline);

    // Show a message if the requested group doesn't have any tasks.
    if tasks.is_empty() {
        output.push_str(&format!(
            "\nTask list is empty. Add tasks with `pueue add -g {group_name} -- [cmd]`"
        ));
        return;
    }

    let table = table_builder.build(tasks);
    output.push_str(&format!("\n{table}"));
}

/// Print all groups. All tasks will be shown in the table of their assigned group.
///
/// This will create multiple tables, one table for each group.
fn print_all_groups(
    state: State,
    tasks: Vec<Task>,
    style: &OutputStyle,
    table_builder: TableBuilder,
    output: &mut String,
) {
    // Early exit and hint if there are no tasks in the queue
    // Print the state of the default group anyway, since this is information one wants to
    // see most of the time anyway.
    if state.tasks.is_empty() {
        let headline = get_group_headline(
            PUEUE_DEFAULT_GROUP,
            state.groups.get(PUEUE_DEFAULT_GROUP).unwrap(),
            style,
        );
        output.push_str(&format!("{headline}\n"));
        output.push_str("\nTask list is empty. Add tasks with `pueue add -- [cmd]`");
        return;
    }

    // Sort all tasks by their respective group;
    let sorted_tasks = sort_tasks_by_group(tasks);

    // Always print the default queue at the very top, if no specific group is requested.
    if sorted_tasks.contains_key(PUEUE_DEFAULT_GROUP) {
        let tasks = sorted_tasks.get(PUEUE_DEFAULT_GROUP).unwrap();
        let headline = get_group_headline(
            PUEUE_DEFAULT_GROUP,
            state.groups.get(PUEUE_DEFAULT_GROUP).unwrap(),
            style,
        );
        output.push_str(&headline);
        let table = table_builder.clone().build(tasks);
        output.push_str(&format!("\n{table}"));

        // Add a newline if there are further groups to be printed
        if sorted_tasks.len() > 1 {
            output.push('\n');
        }
    }

    // Print a table for every other group that has any tasks
    let mut sorted_iter = sorted_tasks.iter().peekable();
    while let Some((group, tasks)) = sorted_iter.next() {
        // We always want to print the default group at the very top.
        // That's why we print it before this loop and skip it in here.
        if group.eq(PUEUE_DEFAULT_GROUP) {
            continue;
        }

        let headline = get_group_headline(group, state.groups.get(group).unwrap(), style);
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&headline);
        let table = table_builder.clone().build(tasks);
        output.push_str(&format!("\n{table}"));

        // Add a newline between groups
        if sorted_iter.peek().is_some() {
            output.push('\n');
        }
    }
}
