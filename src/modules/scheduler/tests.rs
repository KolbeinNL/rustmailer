// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use std::sync::Arc;

use native_db::Builder;
use serde::{Deserialize, Serialize};

use crate::{
    generate_token,
    modules::{
        error::{code::ErrorCode, RustMailerError},
        scheduler::{
            context::TaskContext,
            model::{TaskMeta, TaskStatus},
            nativedb::{meta::NativeDbTaskStore, TASK_MODELS},
            retry::{RetryPolicy, RetryStrategy},
        },
    },
    raise_error,
};

use super::task::Task;

/// Builds a stored-shape task sitting in the given terminal (or non-terminal)
/// state, as if a previous delivery attempt had just given up.
fn task_in_state(status: TaskStatus) -> TaskMeta {
    let mut meta = TaskMeta::new(
        "retry_test_task_key".to_string(),
        "{}".to_string(),
        "retry_test_queue".to_string(),
        RetryPolicy {
            strategy: RetryStrategy::Linear { interval: 1 },
            max_retries: Some(3),
        },
        0,
    );
    meta.status = status;
    meta.retry_count = Some(3);
    meta.last_error = Some("smtp connection refused".to_string());
    meta.next_run = 0;
    meta
}

#[tokio::test]
async fn reset_for_retry_requeues_a_failed_task() {
    let db = Arc::new(Builder::new().create_in_memory(&TASK_MODELS).unwrap());
    let failed = task_in_state(TaskStatus::Failed);
    let id = failed.id;
    NativeDbTaskStore::store_one(&db, failed).await.unwrap();

    NativeDbTaskStore::reset_for_retry(&db, id).await.unwrap();

    let requeued = NativeDbTaskStore::get(&db, id).await.unwrap().unwrap();
    assert_eq!(requeued.status, TaskStatus::Scheduled);
    // The retry counter is cleared so the task gets a full automatic retry
    // budget again, and the stale error is dropped.
    assert_eq!(requeued.retry_count, None);
    assert_eq!(requeued.last_error, None);
    // `next_run` is pulled forward so the scheduler picks the task up on its
    // next tick rather than keeping the stale timestamp.
    assert!(requeued.next_run > 0);
}

#[tokio::test]
async fn reset_for_retry_rejects_a_task_that_is_not_terminal() {
    let db = Arc::new(Builder::new().create_in_memory(&TASK_MODELS).unwrap());
    let scheduled = task_in_state(TaskStatus::Scheduled);
    let id = scheduled.id;
    NativeDbTaskStore::store_one(&db, scheduled).await.unwrap();

    let err = NativeDbTaskStore::reset_for_retry(&db, id)
        .await
        .unwrap_err();
    match err {
        RustMailerError::Generic { code, .. } => assert_eq!(code, ErrorCode::InvalidParameter),
    }
    // The rejected task is left exactly as it was.
    let untouched = NativeDbTaskStore::get(&db, id).await.unwrap().unwrap();
    assert_eq!(untouched.status, TaskStatus::Scheduled);
    assert_eq!(untouched.retry_count, Some(3));
}

/// The whole point of a manual retry is that the scheduler picks the task up
/// again, which depends on the fetch scan matching the reset status *and* the
/// rewritten `next_run`. Assert it end to end rather than trusting the fields.
#[tokio::test]
async fn a_retried_task_is_picked_up_by_the_scheduler_scan() {
    let db = Arc::new(Builder::new().create_in_memory(&TASK_MODELS).unwrap());
    let failed = task_in_state(TaskStatus::Failed);
    let id = failed.id;
    NativeDbTaskStore::store_one(&db, failed).await.unwrap();

    // A failed task is invisible to the fetch loop...
    assert!(NativeDbTaskStore::fetch_pending_tasks(&db)
        .await
        .unwrap()
        .is_empty());

    NativeDbTaskStore::reset_for_retry(&db, id).await.unwrap();

    // ...but a retried one is handed to a worker on the next tick.
    let pending = NativeDbTaskStore::fetch_pending_tasks(&db).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, id);

    // The fetch hands back a pre-claim snapshot of each task, so the status on
    // the returned value is the old one; what matters is that the store itself
    // has already claimed the task as `Running`.
    let claimed = NativeDbTaskStore::get(&db, id).await.unwrap().unwrap();
    assert_eq!(claimed.status, TaskStatus::Running);

    // ...which also means it is not dispatched a second time.
    assert!(NativeDbTaskStore::fetch_pending_tasks(&db)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn reset_tasks_for_retry_skips_unknown_and_unretryable_ids() {
    let db = Arc::new(Builder::new().create_in_memory(&TASK_MODELS).unwrap());
    let failed = task_in_state(TaskStatus::Failed);
    let succeeded = task_in_state(TaskStatus::Success);
    let failed_id = failed.id;
    let succeeded_id = succeeded.id;
    NativeDbTaskStore::store_one(&db, failed).await.unwrap();
    NativeDbTaskStore::store_one(&db, succeeded).await.unwrap();

    let retried =
        NativeDbTaskStore::reset_tasks_for_retry(&db, vec![failed_id, succeeded_id, u64::MAX])
            .await
            .unwrap();

    assert_eq!(retried, 1);
    let requeued = NativeDbTaskStore::get(&db, failed_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(requeued.status, TaskStatus::Scheduled);
    let untouched = NativeDbTaskStore::get(&db, succeeded_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(untouched.status, TaskStatus::Success);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RetryTestTask;

#[tokio::test]
async fn test_retry_policy() {
    // Initialize database and task store
    let db = Builder::new().create_in_memory(&TASK_MODELS).unwrap();
    let task_store = Arc::new(NativeDbTaskStore::init(Arc::new(db)));

    // Create task context
    let task_context = TaskContext::with_arc_store(task_store.clone())
        .register::<RetryTestTask>()
        .set_concurrency("retry_test_queue", 1)
        .start_with_cleaner()
        .await;

    // Add test task
    task_context.add_task(RetryTestTask, None).await.unwrap();

    // Wait for all retries to complete (5s interval * 3 retries = 15s + buffer)
    tokio::time::sleep(std::time::Duration::from_secs(50)).await;
}

#[test]
fn test1() {
    println!("task-{}", generate_token!(48).to_lowercase())
}

impl Task for RetryTestTask {
    const TASK_KEY: &'static str = "retry_test_task_key";
    const TASK_QUEUE: &'static str = "retry_test_queue";

    fn delay_seconds(&self) -> u32 {
        0
    }

    fn retry_policy(&self) -> super::retry::RetryPolicy {
        super::retry::RetryPolicy {
            strategy: super::retry::RetryStrategy::Exponential { base: 2 },
            max_retries: Some(5),
        }
    }

    fn run(self, _task_id: u64) -> super::task::TaskFuture {
        Box::pin(async move {
            // In a real test, you would pass the metrics through some context mechanism
            // For this example, we'll just simulate the failure without recording
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            println!(
                "current time: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            );
            Err(raise_error!("Task failed".into(), ErrorCode::InternalError))
        })
    }
}
