// Clippy generates a false-positive for an empty generated docstring in the query parser code.
#![allow(clippy::empty_docs)]
use crate::prelude::*;

use chrono::prelude::*;
use pest::Parser;
use pest_derive::Parser;

use pueue_lib::task::{Task, TaskResult, TaskStatus};

mod column_selection;
mod filters;
mod limit;
mod order_by;

use limit::Limit;
use order_by::Direction;

/// See the pest docs on how this derive macro works and how to use pest:
/// https://docs.rs/pest/latest/pest/
#[derive(Parser)]
#[grammar = "./src/client/query/syntax.pest"]
struct QueryParser;

type FilterFunction = dyn Fn(&Task) -> bool;

/// All applicable information that has been extracted from the query.
#[derive(Default)]
pub struct QueryResult {
    /// Filter results for a single group.
    group: Option<String>,

    /// The list of selected columns based.
    pub selected_columns: Vec<Rule>,

    /// A list of filter functions that should be applied to the list of tasks.
    filters: Vec<Box<FilterFunction>>,

    /// A list of filter functions that should be applied to the list of tasks.
    order_by: Option<(Rule, Direction)>,

    /// Limit
    limit: Option<(Limit, usize)>,
}

impl QueryResult {
    /// Take a list of tasks and apply all filters to it.
    pub fn apply_filters(&self, tasks: Vec<Task>) -> Vec<Task> {
        let mut iter = tasks.into_iter();

        // If requested, only look at tasks of a specific group.
        if let Some(group) = &self.group {
            iter = iter
                .filter(|task| task.group == *group)
                .collect::<Vec<Task>>()
                .into_iter();
        }

        for filter in self.filters.iter() {
            iter = iter.filter(filter).collect::<Vec<Task>>().into_iter();
        }
        iter.collect()
    }

    /// Take a list of tasks and apply all filters to it.
    pub fn order_tasks(&self, mut tasks: Vec<Task>) -> Vec<Task> {
        // Only apply ordering if it was requested.
        let Some((column, direction)) = &self.order_by else {
            return tasks;
        };

        // Sort the tasks by the specified column.
        tasks.sort_by(|task1, task2| match column {
            Rule::column_id => task1.id.cmp(&task2.id),
            Rule::column_status => {
                /// Rank a task status to allow ordering by status.
                /// Returns a u8 based on the expected
                fn rank_status(task: &Task) -> u8 {
                    match &task.status {
                        TaskStatus::Stashed { .. } => 0,
                        TaskStatus::Locked { .. } => 1,
                        TaskStatus::Queued { .. } => 2,
                        TaskStatus::Paused { .. } => 3,
                        TaskStatus::Running { .. } => 4,
                        TaskStatus::Done { result, .. } => match result {
                            TaskResult::Success => 6,
                            _ => 5,
                        },
                    }
                }

                rank_status(task1).cmp(&rank_status(task2))
            }
            Rule::column_label => task1.label.cmp(&task2.label),
            Rule::column_command => task1.command.cmp(&task2.command),
            Rule::column_path => task1.path.cmp(&task2.path),
            Rule::column_enqueue_at => {
                fn enqueue_date(task: &Task) -> DateTime<Local> {
                    match &task.status {
                        TaskStatus::Queued { enqueued_at, .. }
                        | TaskStatus::Running { enqueued_at, .. }
                        | TaskStatus::Paused { enqueued_at, .. }
                        | TaskStatus::Done { enqueued_at, .. }
                        | TaskStatus::Stashed {
                            enqueue_at: Some(enqueued_at),
                            ..
                        } => *enqueued_at,
                        // considered far in the future when no explicit date:
                        _ => DateTime::<Utc>::MAX_UTC.into(),
                    }
                }

                enqueue_date(task1).cmp(&enqueue_date(task2))
            }
            Rule::column_start => {
                let (start1, _) = task1.start_and_end();
                let (start2, _) = task2.start_and_end();
                start1.cmp(&start2)
            }
            Rule::column_end => {
                let (_, end1) = task1.start_and_end();
                let (_, end2) = task2.start_and_end();
                end1.cmp(&end2)
            }
            _ => std::cmp::Ordering::Less,
        });

        // Reverse the order, if we're in ordering by descending order.
        if let Direction::Descending = direction {
            tasks.reverse();
        }

        tasks
    }

    /// Take a list of tasks and apply all filters to it.
    pub fn limit_tasks(&self, tasks: Vec<Task>) -> Vec<Task> {
        // Only apply limits if it was requested.
        let Some((direction, count)) = &self.limit else {
            return tasks;
        };

        // Don't do anything if:
        // - we don't have to limit
        // - the limit is invalid
        if tasks.len() <= *count || *count == 0 {
            return tasks;
        }

        match direction {
            Limit::First => tasks[0..*count].to_vec(),
            Limit::Last => tasks[(tasks.len() - count)..].to_vec(),
        }
    }
}

/// Take a given `pueue status QUERY` and apply it to all components that're involved in the
/// `pueue status` process:
///
/// - TableBuilder: The component responsible for building the table and determining which
///         columns should or need to be displayed.
///         A `columns [columns]` statement will define the set of visible columns.
pub fn apply_query(query: &str, group: &Option<String>) -> Result<QueryResult> {
    let mut parsed = QueryParser::parse(Rule::query, query).context("Failed to parse query")?;

    let mut query_result = QueryResult {
        group: group.clone(),
        ..Default::default()
    };

    // Expect there to be exactly one pair for the full query.
    // Return early if we got an empty query.
    let Some(parsed) = parsed.next() else {
        return Ok(query_result);
    };

    // Make sure we really got a query.
    if parsed.as_rule() != Rule::query {
        bail!("Expected a valid query");
    }

    // Get the sections of the query
    let sections = parsed.into_inner();
    // Go through each section and handle it accordingly
    for section in sections {
        // The `columns=[columns]` section
        // E.g. `columns=id,status,start,end`
        match section.as_rule() {
            Rule::column_selection => column_selection::apply(section, &mut query_result)?,
            Rule::datetime_filter => filters::datetime(section, &mut query_result)?,
            Rule::label_filter => filters::label(section, &mut query_result)?,
            Rule::command_filter => filters::command(section, &mut query_result)?,
            Rule::status_filter => filters::status(section, &mut query_result)?,
            Rule::order_by_condition => order_by::order_by(section, &mut query_result)?,
            Rule::limit_condition => limit::limit(section, &mut query_result)?,
            _ => (),
        }
    }

    Ok(query_result)
}
