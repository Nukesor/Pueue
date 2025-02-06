use crate::prelude::*;

use std::{collections::HashMap, path::PathBuf};

use assert_matches::assert_matches;
use chrono::{Local, TimeZone};
use pretty_assertions::assert_eq;
use rstest::rstest;

use pueue::client::query::{apply_query, Rule};
use pueue_lib::state::PUEUE_DEFAULT_GROUP;
use pueue_lib::task::{Task, TaskResult, TaskStatus};

const TEST_COMMAND_SLEEP: &str = "sleep 60";
const TEST_COMMAND_HELLO: &str = "echo Hello Pueue";

/// A small helper function to reduce a bit of boilerplate.
pub fn build_task() -> Task {
    Task::new(
        TEST_COMMAND_SLEEP.to_owned(),
        PathBuf::from("/tmp"),
        HashMap::new(),
        PUEUE_DEFAULT_GROUP.to_owned(),
        TaskStatus::Queued {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
        },
        Vec::new(),
        0,
        None,
    )
}

/// Build a list of some pre-build tasks that are used to test.
pub fn test_tasks() -> Vec<Task> {
    let mut tasks = Vec::new();

    // Failed task
    let mut failed = build_task();
    failed.id = 0;
    failed.status = TaskStatus::Done {
        result: TaskResult::Failed(255),
        enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
        start: Local.with_ymd_and_hms(2022, 1, 10, 10, 5, 0).unwrap(),
        end: Local.with_ymd_and_hms(2022, 1, 10, 10, 10, 0).unwrap(),
    };
    failed.label = Some("label-10-0".to_string());
    tasks.insert(failed.id, failed);

    // Successful task
    let mut successful = build_task();
    successful.id = 1;
    successful.status = TaskStatus::Done {
        result: TaskResult::Success,
        enqueued_at: Local.with_ymd_and_hms(2022, 1, 8, 10, 0, 0).unwrap(),
        start: Local.with_ymd_and_hms(2022, 1, 8, 10, 5, 0).unwrap(),
        end: Local.with_ymd_and_hms(2022, 1, 8, 10, 10, 0).unwrap(),
    };
    successful.label = Some("label-10-1".to_string());
    tasks.insert(successful.id, successful);

    // Stashed task
    let mut stashed = build_task();
    stashed.status = TaskStatus::Stashed { enqueue_at: None };
    stashed.id = 2;
    stashed.label = Some("label-10-2".to_string());
    tasks.insert(stashed.id, stashed);

    // Scheduled task
    let mut scheduled = build_task();
    scheduled.status = TaskStatus::Stashed {
        enqueue_at: Some(Local.with_ymd_and_hms(2022, 1, 10, 11, 0, 0).unwrap()),
    };
    scheduled.id = 3;
    scheduled.group = "testgroup".to_string();
    tasks.insert(scheduled.id, scheduled);

    // Running task
    let mut running = build_task();
    running.status = TaskStatus::Running {
        enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
        start: Local.with_ymd_and_hms(2022, 1, 2, 12, 0, 0).unwrap(),
    };
    running.id = 4;
    tasks.insert(running.id, running);

    // Add two queued tasks with different command
    let mut queued = build_task();
    queued.id = 5;
    queued.command = TEST_COMMAND_HELLO.to_string();
    tasks.insert(queued.id, queued.clone());

    // Task 6 depends on task 5
    queued.id = 6;
    queued.dependencies.push(5);
    tasks.insert(queued.id, queued);

    tasks
}

fn test_tasks_with_query(query: &str, group: &Option<String>) -> Result<Vec<Task>> {
    let mut tasks = test_tasks();

    let query_result = apply_query(query, group)?;
    tasks = query_result.apply_filters(tasks);
    tasks = query_result.order_tasks(tasks);
    tasks = query_result.limit_tasks(tasks);

    Ok(tasks)
}

/// Select only specific columns for printing
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn column_selection() -> Result<()> {
    let result = apply_query("columns=id,status,command", &None)?;
    assert_eq!(
        result.selected_columns,
        [Rule::column_id, Rule::column_status, Rule::column_command]
    );

    Ok(())
}

/// Select the first few entries of the list
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn limit_first() -> Result<()> {
    let tasks = test_tasks_with_query("first 4", &None)?;

    assert!(tasks.len() == 4);
    assert_eq!(tasks[0].id, 0);
    assert_eq!(tasks[3].id, 3);

    Ok(())
}

/// Select the last few entries of the list
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn limit_last() -> Result<()> {
    let tasks = test_tasks_with_query("last 4", &None)?;

    assert!(tasks.len() == 4);
    assert_eq!(tasks[0].id, 3);
    assert_eq!(tasks[3].id, 6);

    Ok(())
}

/// Filter by start date
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_start() -> Result<()> {
    let tasks = test_tasks_with_query("start>2022-01-10 09:00:00", &None)?;

    assert!(tasks.len() == 1);
    assert_eq!(tasks[0].id, 0);

    Ok(())
}

/// Filtering in combination with groups works as expected
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_with_group() -> Result<()> {
    let tasks = test_tasks_with_query("status=stashed", &Some("testgroup".to_string()))?;

    assert!(tasks.len() == 1);
    assert_eq!(tasks[0].id, 3);

    Ok(())
}

/// Filter by end date with the current time as a time and a date.
#[rstest]
#[case("2022-01-10")]
#[case("2022-01-10 09:00:00")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_end_with_time(#[case] format: &'static str) -> Result<()> {
    let tasks = test_tasks_with_query(&format!("end<{format}"), &None)?;

    assert!(tasks.len() == 1);
    assert_eq!(tasks[0].id, 1);

    Ok(())
}

/// Filter tasks by status
#[rstest]
#[case("queued", 2)]
#[case("running", 1)]
#[case("paused", 0)]
#[case("success", 1)]
#[case("failed", 1)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_status(#[case] status_filter: &str, #[case] match_count: usize) -> Result<()> {
    // Get the correct query keyword for the given status.

    let tasks = test_tasks_with_query(&format!("status={status_filter}"), &None)?;

    for task in tasks.iter() {
        match status_filter {
            "queued" => {
                assert_matches!(
                    task.status,
                    TaskStatus::Queued { .. },
                    "Only Queued tasks are allowed"
                );
            }
            "stashed" => assert_matches!(
                task.status,
                TaskStatus::Stashed { .. },
                "Only Stashed tasks are allowed"
            ),
            "running" => assert_matches!(
                task.status,
                TaskStatus::Running { .. },
                "Only Running tasks are allowed"
            ),
            "paused" => assert_matches!(
                task.status,
                TaskStatus::Paused { .. },
                "Only Paused tasks are allowed"
            ),
            "success" => assert_matches!(
                task.status,
                TaskStatus::Done {
                    result: TaskResult::Success,
                    ..
                },
                "Only Succesful tasks are allowed"
            ),
            "failed" => assert_matches!(
                task.status,
                TaskStatus::Done {
                    result: TaskResult::Failed(_),
                    ..
                },
                "Only Failed tasks are allowed"
            ),
            _ => bail!("Got unexpected TaskStatus in filter_status"),
        };
    }

    assert_eq!(
        tasks.len(),
        match_count,
        "Got a different amount of tasks than expected for the status filter {status_filter:?}."
    );

    Ok(())
}

/// Order the test state by task status.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn order_by_status() -> Result<()> {
    let tasks = test_tasks_with_query("order_by status", &None)?;

    let expected = vec![
        TaskStatus::Stashed { enqueue_at: None },
        TaskStatus::Stashed {
            enqueue_at: Some(Local.with_ymd_and_hms(2022, 1, 10, 11, 0, 0).unwrap()),
        },
        TaskStatus::Queued {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
        },
        TaskStatus::Queued {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
        },
        TaskStatus::Running {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
            start: Local.with_ymd_and_hms(2022, 1, 2, 12, 0, 0).unwrap(),
        },
        TaskStatus::Done {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
            start: Local.with_ymd_and_hms(2022, 1, 10, 10, 5, 0).unwrap(),
            end: Local.with_ymd_and_hms(2022, 1, 10, 10, 10, 0).unwrap(),
            result: TaskResult::Failed(255),
        },
        TaskStatus::Done {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 8, 10, 0, 0).unwrap(),
            start: Local.with_ymd_and_hms(2022, 1, 8, 10, 5, 0).unwrap(),
            end: Local.with_ymd_and_hms(2022, 1, 8, 10, 10, 0).unwrap(),
            result: TaskResult::Success,
        },
    ];

    let actual: Vec<TaskStatus> = tasks.iter().map(|task| task.status.clone()).collect();
    assert_eq!(actual, expected);

    Ok(())
}

/// Order the tasks by enqueue(d) date.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn order_by_enqueue_at() -> Result<()> {
    let tasks = test_tasks_with_query("order_by enqueue_at asc", &None)?;

    let expected = vec![
        TaskStatus::Done {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 8, 10, 0, 0).unwrap(),
            start: Local.with_ymd_and_hms(2022, 1, 8, 10, 5, 0).unwrap(),
            end: Local.with_ymd_and_hms(2022, 1, 8, 10, 10, 0).unwrap(),
            result: TaskResult::Success,
        },
        TaskStatus::Done {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
            start: Local.with_ymd_and_hms(2022, 1, 10, 10, 5, 0).unwrap(),
            end: Local.with_ymd_and_hms(2022, 1, 10, 10, 10, 0).unwrap(),
            result: TaskResult::Failed(255),
        },
        TaskStatus::Running {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
            start: Local.with_ymd_and_hms(2022, 1, 2, 12, 0, 0).unwrap(),
        },
        TaskStatus::Queued {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
        },
        TaskStatus::Queued {
            enqueued_at: Local.with_ymd_and_hms(2022, 1, 10, 10, 0, 0).unwrap(),
        },
        TaskStatus::Stashed {
            enqueue_at: Some(Local.with_ymd_and_hms(2022, 1, 10, 11, 0, 0).unwrap()),
        },
        TaskStatus::Stashed { enqueue_at: None },
    ];

    let actual: Vec<TaskStatus> = tasks.iter().map(|task| task.status.clone()).collect();
    assert_eq!(actual, expected);

    Ok(())
}

/// Filter tasks by label with the "eq" `=` "ne" `!=` and "contains" `%=`filter.
#[rstest]
#[case("%=", "label", 3)]
#[case("%=", "label-10", 3)]
#[case("%=", "label-10-1", 1)]
#[case("=", "label-10-1", 1)]
#[case("!=", "label-10-1", 6)]
#[case("!=", "label-10", 7)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_label(
    #[case] operator: &'static str,
    #[case] label_filter: &'static str,
    #[case] match_count: usize,
) -> Result<()> {
    let tasks = test_tasks_with_query(&format!("label{operator}{label_filter}"), &None)?;

    for task in tasks.iter() {
        // Make sure the task either has no label or the label doesn't match the filter.
        if operator == "!=" {
            if let Some(label) = &task.label {
                assert_ne!(
                    label, label_filter,
                    "Label '{label}' matched exact filter '{label_filter}'"
                );
            }
            continue;
        }

        let label = task.label.as_ref().expect("Expected task to have a label");
        if operator == "%=" {
            // Make sure the label contained our filter.
            assert!(
                label.contains(label_filter),
                "Label '{label}' didn't contain filter '{label_filter}'"
            );
        } else if operator == "=" {
            // Make sure the label exactly matches the filter.
            assert_eq!(
                label, &label_filter,
                "Label '{label}' didn't match exact filter '{label_filter}'"
            );
        }
    }

    assert_eq!(
        tasks.len(),
        match_count,
        "Got a different amount of tasks than expected for the label filter: {label_filter}."
    );

    Ok(())
}

/// Filter tasks by command with the "eq" `=` "ne" `!=` and "contains" `%=`filter.
#[rstest]
#[case("=", TEST_COMMAND_SLEEP, 5)]
#[case("!=", TEST_COMMAND_SLEEP, 2)]
#[case("%=", &TEST_COMMAND_SLEEP[..4], 5)]
#[case("=", TEST_COMMAND_HELLO, 2)]
#[case("!=", TEST_COMMAND_HELLO, 5)]
#[case("%=", &TEST_COMMAND_HELLO[..4], 2)]
#[case("!=", "nonexist", 7)]
#[case("%=", "nonexist", 0)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filter_command(
    #[case] operator: &'static str,
    #[case] command_filter: &'static str,
    #[case] match_count: usize,
) -> Result<()> {
    let tasks = test_tasks_with_query(&format!("command{operator}{command_filter}"), &None)?;

    for task in tasks.iter() {
        let command = task.command.as_str();
        if operator == "!=" {
            // Make sure the task's command doesn't match the filter.
            assert_ne!(
                command, command_filter,
                "Command '{command}' matched exact filter '{command_filter}'"
            );
        } else if operator == "%=" {
            // Make sure the command contained our filter.
            assert!(
                command.contains(command_filter),
                "Command '{command}' didn't contain filter '{command_filter}'"
            );
        } else if operator == "=" {
            // Make sure the command exactly matches the filter.
            assert_eq!(
                command, command_filter,
                "Command '{command}' didn't match exact filter '{command_filter}'"
            );
        }
    }

    assert_eq!(
        tasks.len(),
        match_count,
        "Got a different amount of tasks than expected for the command filter: {command_filter}."
    );

    Ok(())
}
