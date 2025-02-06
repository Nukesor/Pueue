#![allow(bindings_with_variant_name)]
use crate::prelude::*;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use pest::iterators::Pair;
use pueue_lib::task::{Task, TaskResult, TaskStatus};

use super::{QueryResult, Rule};

enum DateOrDateTime {
    DateTime(DateTime<Local>),
    Date(NaiveDate),
}

/// Parse a datetime/date/time filter.
/// Such a filter can be applied to either the `start`, `end` or `enqueue_at` field.
///
/// This filter syntax looks like this is expected to be:
/// `[enqueue_at|start|end] [>|<|=|!=] [YYYY-MM-DD HH:mm:SS|HH:mm:SS|YYYY-MM-DD]`
///
/// The data structure looks something like this:
/// Pair {
///     rule: datetime_filter,
///     span: Span {
///         str: "start=2022-09-01",
///         start: 0,
///         end: 16,
///     },
///     inner: [
///         Pair {
///             rule: column_start,
///             span: Span {
///                 str: "start",
///                 start: 0,
///                 end: 5,
///             },
///             inner: [],
///         },
///         Pair {
///             rule: operator,
///             span: Span {
///                 str: "=",
///                 start: 5,
///                 end: 6,
///             },
///             inner: [
///                 Pair {
///                     rule: eq,
///                     span: Span {
///                         str: "=",
///                         start: 5,
///                         end: 6,
///                     },
///                     inner: [],
///                 },
///             ],
///         },
///         Pair {
///             rule: date,
///             span: Span {
///                 str: "2022-09-01",
///                 start: 6,
///                 end: 16,
///             },
///             inner: [],
///         },
///     ],
/// }
pub fn datetime(section: Pair<'_, Rule>, query_result: &mut QueryResult) -> Result<()> {
    let mut filter = section.into_inner();
    // Get the column this filter should be applied to.
    // Either of [Rule::column_enqueue_at | Rule::column_start | Rule::column_end]
    let column = filter.next().unwrap();
    let column = column.as_rule();

    // Get the operator that should be applied in this filter.
    // Either of [Rule::eq | Rule::neq | Rule::lt | Rule::gt]
    let operator = filter.next().unwrap().as_rule();

    // Get the point in time which we should filter for.
    // This can be either a Date or a DateTime.
    let operand = filter.next().unwrap();
    let operand_rule = operand.as_rule();
    let operand = match operand_rule {
        Rule::time => {
            let time = NaiveTime::parse_from_str(operand.as_str(), "%X")
                .context("Expected hh:mm:ss time format")?;
            let today = Local::now().date_naive();
            let datetime = today.and_time(time).and_local_timezone(Local).unwrap();
            DateOrDateTime::DateTime(datetime)
        }
        Rule::datetime => {
            let datetime = NaiveDateTime::parse_from_str(operand.as_str(), "%F %X")
                .context("Expected YYYY-MM-SS hh:mm:ss date time format")?;
            DateOrDateTime::DateTime(datetime.and_local_timezone(Local).unwrap())
        }
        Rule::date => {
            let date = NaiveDate::parse_from_str(operand.as_str(), "%F")
                .context("Expected YYYY-MM-SS date format")?;
            DateOrDateTime::Date(date)
        }
        _ => bail!("Expected either a date, datetime or time expression."),
    };

    let filter_function = Box::new(move |task: &Task| -> bool {
        // Get the field we should apply the filter to.
        let field = match column {
            Rule::column_enqueue_at => {
                let TaskStatus::Stashed {
                    enqueue_at: Some(enqueue_at),
                } = task.status
                else {
                    return false;
                };

                enqueue_at
            }
            Rule::column_start => {
                let (start, _) = task.start_and_end();
                let Some(start) = start else {
                    return false;
                };
                start
            }
            Rule::column_end => {
                let (_, end) = task.start_and_end();
                let Some(end) = end else {
                    return false;
                };
                end
            }
            _ => return true,
        };

        // Apply the operator to the operands.
        // The operator might have a different meaning depending on the type of datetime/date
        // we're dealing with.
        // E.g. when working with dates, `>` should mean bigger than the end of that day.
        // `<` however should mean before that day.
        match operand {
            DateOrDateTime::DateTime(datetime) => match operator {
                Rule::eq => field == datetime,
                Rule::neq => field != datetime,
                Rule::lt => field < datetime,
                Rule::gt => field > datetime,
                _ => true,
            },
            DateOrDateTime::Date(date) => {
                // Get the start of the given day.
                // Use the most inclusive datetime in case of ambiguity
                let start_of_day = date
                    .and_hms_opt(0, 0, 0)
                    .expect("Couldn't determine start of day for given date.")
                    .and_local_timezone(Local);
                let start_of_day = match start_of_day.latest() {
                    None => return false,
                    Some(datetime) => datetime,
                };

                // Get the end of the given day.
                // Use the most inclusive datetime in case of ambiguity
                let end_of_day = (date + TimeDelta::try_days(1).unwrap())
                    .and_hms_opt(0, 0, 0)
                    .expect("Couldn't determine start of day for given date.")
                    .and_local_timezone(Local);
                let end_of_day = match end_of_day.latest() {
                    None => return false,
                    Some(datetime) => datetime,
                };

                match operator {
                    Rule::eq => field > start_of_day && field < end_of_day,
                    Rule::neq => field < start_of_day && field > end_of_day,
                    Rule::lt => field < start_of_day,
                    Rule::gt => field > end_of_day,
                    _ => true,
                }
            }
        }
    });
    query_result.filters.push(filter_function);

    Ok(())
}

/// Parse a filter for the label field.
///
/// This filter syntax looks like this:
/// `label [=|!=] string`
///
/// The data structure looks something like this:
///  Pair {
///     rule: label_filter,
///     span: Span {
///         str: "label=test",
///         start: 0,
///         end: 10,
///     },
///     inner: [
///         Pair {
///             rule: column_label,
///             span: Span {
///                 str: "label",
///                 start: 0,
///                 end: 5,
///             },
///             inner: [],
///         },
///         Pair {
///             rule: eq,
///             span: Span {
///                 str: "=",
///                 start: 5,
///                 end: 6,
///             },
///             inner: [],
///         },
///         Pair {
///             rule: label,
///             span: Span {
///                 str: "test",
///                 start: 6,
///                 end: 10,
///             },
///             inner: [],
///         },
///     ],
/// }
pub fn label(section: Pair<'_, Rule>, query_result: &mut QueryResult) -> Result<()> {
    let mut filter = section.into_inner();
    // The first word should be the `label` keyword.
    let _label = filter.next().unwrap();

    // Get the operator that should be applied in this filter.
    // Can be either of [Rule::eq | Rule::neq].
    let operator = filter.next().unwrap().as_rule();

    // Get the name of the label we should filter for.
    let operand = filter.next().unwrap().as_str().to_string();

    // Build the label filter function.
    let filter_function = Box::new(move |task: &Task| -> bool {
        let Some(label) = &task.label else {
            return operator == Rule::neq;
        };

        match operator {
            Rule::eq => label == &operand,
            Rule::neq => label != &operand,
            Rule::contains => label.contains(&operand),
            _ => false,
        }
    });
    query_result.filters.push(filter_function);

    Ok(())
}

/// Parse a filter for the command field.
///
/// This filter syntax is exactly the same as the [label] filter.
/// Only the keyword changed from `label` to `command`.
pub fn command(section: Pair<'_, Rule>, query_result: &mut QueryResult) -> Result<()> {
    let mut filter = section.into_inner();
    // The first word should be the `command` keyword.
    let _command = filter.next().unwrap();

    // Get the operator that should be applied in this filter.
    // Can be either of [Rule::eq | Rule::neq].
    let operator = filter.next().unwrap().as_rule();

    // Get the name of the command we should filter for.
    let operand = filter.next().unwrap().as_str().to_string();

    // Build the command filter function.
    let filter_function = Box::new(move |task: &Task| -> bool {
        let command = task.command.as_str();
        match operator {
            Rule::eq => command == operand,
            Rule::neq => command != operand,
            Rule::contains => command.contains(&operand),
            _ => false,
        }
    });
    query_result.filters.push(filter_function);

    Ok(())
}

/// Parse a filter for the status field.
///
/// This filter syntax looks like this:
/// `status [=|!=] [status]`
///
/// The data structure looks something like this:
/// Pair {
///     rule: status_filter,
///     span: Span {
///         str: "status=success",
///         start: 0,
///         end: 14,
///     },
///     inner: [
///         Pair {
///             rule: column_status,
///             span: Span {
///                 str: "status",
///                 start: 0,
///                 end: 6,
///             },
///             inner: [],
///         },
///         Pair {
///             rule: eq,
///             span: Span {
///                 str: "=",
///                 start: 6,
///                 end: 7,
///             },
///             inner: [],
///         },
///         Pair {
///             rule: status_success,
///             span: Span {
///                 str: "success",
///                 start: 7,
///                 end: 14,
///             },
///             inner: [],
///         },
///     ],
/// }
pub fn status(section: Pair<'_, Rule>, query_result: &mut QueryResult) -> Result<()> {
    let mut filter = section.into_inner();
    // The first word should be the `status` keyword.
    let _status = filter.next().unwrap();

    // Get the operator that should be applied in this filter.
    // Can be either of [Rule::eq | Rule::neq]
    let operator = filter.next().unwrap().as_rule();

    // Get the status we should filter for.
    let operand = filter.next().unwrap().as_rule();

    // Build the filter function for the task's status.
    let filter_function = Box::new(move |task: &Task| -> bool {
        let matches = match operand {
            Rule::status_queued => matches!(task.status, TaskStatus::Queued { .. }),
            Rule::status_stashed => matches!(task.status, TaskStatus::Stashed { .. }),
            Rule::status_running => matches!(task.status, TaskStatus::Running { .. }),
            Rule::status_paused => matches!(task.status, TaskStatus::Paused { .. }),
            Rule::status_success => {
                matches!(
                    &task.status,
                    TaskStatus::Done {
                        result: TaskResult::Success,
                        ..
                    }
                )
            }
            Rule::status_failed => {
                let mut matches = false;
                if let TaskStatus::Done { result, .. } = &task.status {
                    if !matches!(result, TaskResult::Success) {
                        matches = true;
                    }
                }
                matches
            }
            _ => return false,
        };

        match operator {
            Rule::eq => matches,
            Rule::neq => !matches,
            _ => false,
        }
    });
    query_result.filters.push(filter_function);

    Ok(())
}
