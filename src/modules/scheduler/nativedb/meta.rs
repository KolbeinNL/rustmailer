// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use std::sync::Arc;
use std::time::Instant;

use itertools::Itertools;
use native_db::Database;
use tracing::warn;

use crate::{
    modules::{
        database::{
            batch_delete_impl, batch_insert_impl, batch_update_impl, filter_by_secondary_key_impl,
            insert_impl, paginate_secondary_scan_impl, secondary_find_impl, update_impl, Paginated,
        },
        error::{code::ErrorCode, RustMailerResult},
        hook::{
            channel::{Event, EVENT_CHANNEL},
            events::{payload::EmailSendingError, EventPayload, EventType, RustMailerEvent},
            task::EventHookTask,
        },
        metrics::{EMAIL, HOOK, RUSTMAILER_TASK_FETCH_DURATION, RUSTMAILER_TASK_QUEUE_LENGTH},
        scheduler::{
            model::{TaskMeta, TaskStatus},
            nativedb::{TaskMetaEntity, TaskMetaEntityKey},
            store::TaskStore,
            task::Task,
        },
        settings::cli::SETTINGS,
        smtp::request::task::SmtpTask,
    },
    raise_error, utc_now,
};

const HOUR_TO_MS: u64 = 60 * 60 * 1000;

#[derive(Clone)]
pub struct NativeDbTaskStore {
    pub store: Arc<Database<'static>>,
}

impl NativeDbTaskStore {
    pub fn init(database: Arc<Database<'static>>) -> Self {
        Self {
            store: database.clone(),
        }
    }

    pub async fn fetch_pending_tasks(
        database: &Arc<Database<'static>>,
    ) -> RustMailerResult<Vec<TaskMeta>> {
        let start = Instant::now();
        let result: Vec<TaskMetaEntity> = batch_update_impl(
            database,
            |rw| {
                let candidates: Vec<TaskMetaEntity> = rw
                    .scan()
                    .secondary(TaskMetaEntityKey::status)
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .start_with(TaskStatus::Scheduled.code())
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .try_collect()
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;

                let email = candidates
                    .iter()
                    .filter(|t| t.task_key == SmtpTask::TASK_KEY)
                    .count();
                RUSTMAILER_TASK_QUEUE_LENGTH
                    .with_label_values(&[EMAIL])
                    .set(email as i64);

                let hook = candidates
                    .iter()
                    .filter(|t| t.task_key == EventHookTask::TASK_KEY)
                    .count();
                RUSTMAILER_TASK_QUEUE_LENGTH
                    .with_label_values(&[HOOK])
                    .set(hook as i64);

                Ok(candidates
                    .into_iter()
                    .filter(|c| c.next_run <= utc_now!())
                    .take(500)
                    .collect())
            },
            move |data| {
                let mut result = Vec::new();
                for entity in data.iter() {
                    let mut updated = entity.clone();
                    updated.status = TaskStatus::Running;
                    updated.updated_at = utc_now!();
                    result.push((entity.clone(), updated));
                }
                Ok(result)
            },
        )
        .await?;

        let elapsed = start.elapsed();
        RUSTMAILER_TASK_FETCH_DURATION.observe(elapsed.as_secs_f64());
        //debug!("Time taken to fetch task from native_db: {:#?}", elapsed);

        Ok(result.into_iter().map(Into::into).collect())
    }

    async fn update_status(
        database: &Arc<Database<'static>>,
        task_id: u64,
        is_success: bool,
        last_error: Option<String>,
        last_duration_ms: Option<usize>,
        retry_count: Option<usize>,
        next_run: Option<i64>,
    ) -> RustMailerResult<()> {
        update_impl(
            database,
            move |rw| {
                rw.get()
                    .secondary::<TaskMetaEntity>(TaskMetaEntityKey::id, task_id)
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .ok_or_else(|| {
                        raise_error!(
                            format!(
                                "The task with id={} that you want to modify was not found.",
                                &task_id
                            ),
                            ErrorCode::ResourceNotFound
                        )
                    })
            },
            move |current| {
                let mut updated = current.clone();
                updated.last_duration_ms = last_duration_ms;
                updated.retry_count = retry_count;
                updated.updated_at = utc_now!();
                match (updated.status.clone(), is_success) {
                    (TaskStatus::Stopped | TaskStatus::Removed, false) => {
                        updated.last_error = last_error;
                    }
                    (_, true) => {
                        updated.status = TaskStatus::Success;
                    }
                    (_, false) => {
                        updated.status = TaskStatus::Failed;
                        updated.last_error = last_error;
                        if let Some(next_run) = next_run {
                            updated.next_run = next_run;
                            updated.status = TaskStatus::Scheduled;
                        }
                    }
                }
                Ok(updated)
            },
        )
        .await?;
        Ok(())
    }

    pub async fn clean_up(database: &Arc<Database<'static>>) -> RustMailerResult<()> {
        let statuses_to_clean = [
            TaskStatus::Removed,
            TaskStatus::Success,
            TaskStatus::Failed,
            TaskStatus::Stopped,
        ];

        let cleanup_interval_ms =
            SETTINGS.rustmailer_cleanup_interval_hours as i64 * HOUR_TO_MS as i64;
        // Failed tasks are retained longer than the other terminal states so that
        // they can still be inspected and manually retried after an outage.
        let failed_retention_ms =
            SETTINGS.rustmailer_failed_task_retention_hours as i64 * HOUR_TO_MS as i64;
        let now = utc_now!();

        for status in statuses_to_clean {
            let retention_ms = match status {
                TaskStatus::Failed => failed_retention_ms,
                _ => cleanup_interval_ms,
            };
            // let status_str = status.to_string();
            let task_ids: Vec<u64> = {
                let rw = database
                    .r_transaction()
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
                rw.scan()
                    .secondary::<TaskMetaEntity>(TaskMetaEntityKey::status)
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .start_with(status.code())
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .filter_map(|result| match result {
                        Ok(t) if now - t.created_at > retention_ms => Some(Ok(t.id)),
                        Ok(_) => None,
                        Err(e) => Some(Err(e)),
                    })
                    .try_collect()
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
            };

            let chunks: Vec<Vec<u64>> = task_ids.chunks(100).map(|chunk| chunk.to_vec()).collect();

            for chunk in chunks {
                batch_delete_impl(database, move |rw| {
                    let to_delete: Vec<TaskMetaEntity> = chunk
                        .iter()
                        .filter_map(|task_id| {
                            rw.get()
                                .secondary(TaskMetaEntityKey::id, *task_id)
                                .map_err(|e| {
                                    raise_error!(format!("{:#?}", e), ErrorCode::InternalError)
                                })
                                .ok()
                                .flatten()
                        })
                        .collect();
                    Ok(to_delete)
                })
                .await?;
            }
        }

        Ok(())
    }

    pub async fn set_status(
        database: &Arc<Database<'static>>,
        task_id: u64,
        status: TaskStatus,
        reason: Option<String>,
    ) -> RustMailerResult<()> {
        assert!(matches!(status, TaskStatus::Removed | TaskStatus::Stopped));
        update_impl(
            database,
            move |rw| {
                rw.get()
                    .secondary::<TaskMetaEntity>(TaskMetaEntityKey::id, task_id)
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .ok_or_else(|| {
                        raise_error!(
                            format!(
                                "The task with id={} that you want to modify was not found.",
                                &task_id
                            ),
                            ErrorCode::ResourceNotFound
                        )
                    })
            },
            |current| {
                let mut updated = current.clone();
                updated.status = status;
                updated.stopped_reason = reason;
                updated.updated_at = utc_now!();
                Ok(updated)
            },
        )
        .await?;
        Ok(())
    }

    /// Returns `true` when a task in this state may be manually retried.
    ///
    /// Only terminal states that still represent an attempt worth repeating are
    /// retryable. In particular `Scheduled`/`Running` are excluded so a manual
    /// retry can never race with the scheduler.
    fn is_retryable(status: &TaskStatus) -> bool {
        matches!(status, TaskStatus::Failed | TaskStatus::Stopped)
    }

    /// Clears the terminal state of a task so the scheduler picks it up again on
    /// its next tick: the retry counter is reset (handing the task a full
    /// automatic retry budget again) and `next_run` is pulled forward to now.
    fn rewind_for_retry(entity: &TaskMetaEntity) -> TaskMetaEntity {
        let mut updated = entity.clone();
        updated.status = TaskStatus::Scheduled;
        updated.retry_count = None;
        updated.last_error = None;
        updated.stopped_reason = None;
        updated.next_run = utc_now!();
        updated.updated_at = utc_now!();
        updated
    }

    /// Rewinds a single task so it is executed again.
    ///
    /// Fails with `InvalidParameter` when the task exists but is not in a
    /// retryable state, and with `ResourceNotFound` when it does not exist.
    pub async fn reset_for_retry(
        database: &Arc<Database<'static>>,
        task_id: u64,
    ) -> RustMailerResult<()> {
        update_impl(
            database,
            move |rw| {
                rw.get()
                    .secondary::<TaskMetaEntity>(TaskMetaEntityKey::id, task_id)
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .ok_or_else(|| {
                        raise_error!(
                            format!("The task with id={} was not found.", task_id),
                            ErrorCode::ResourceNotFound
                        )
                    })
            },
            |current| {
                if !Self::is_retryable(&current.status) {
                    return Err(raise_error!(
                        format!(
                            "The task with id={} is in state '{}' and cannot be retried. Only failed or stopped tasks can be retried.",
                            current.id, current.status
                        ),
                        ErrorCode::InvalidParameter
                    ));
                }
                Ok(Self::rewind_for_retry(current))
            },
        )
        .await?;
        Ok(())
    }

    /// Rewinds a batch of tasks and returns how many were actually requeued.
    ///
    /// Unknown ids and ids that are not in a retryable state are skipped, so a
    /// batch retry is safe to fire at a list that changed in the meantime.
    pub async fn reset_tasks_for_retry(
        database: &Arc<Database<'static>>,
        task_ids: Vec<u64>,
    ) -> RustMailerResult<usize> {
        if task_ids.is_empty() {
            return Ok(0);
        }
        let updated = batch_update_impl(
            database,
            move |rw| {
                let mut retryable = Vec::new();
                for task_id in task_ids.iter() {
                    let entity = rw
                        .get()
                        .secondary::<TaskMetaEntity>(TaskMetaEntityKey::id, *task_id)
                        .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
                    if let Some(entity) = entity {
                        if Self::is_retryable(&entity.status) {
                            retryable.push(entity);
                        }
                    }
                }
                Ok(retryable)
            },
            |targets| {
                Ok(targets
                    .iter()
                    .map(|entity| (entity.clone(), Self::rewind_for_retry(entity)))
                    .collect::<Vec<_>>())
            },
        )
        .await?;
        Ok(updated.len())
    }

    pub async fn heartbeat(
        database: &Arc<Database<'static>>,
        task_id: u64,
    ) -> RustMailerResult<()> {
        update_impl(
            database,
            move |rw| {
                rw.get()
                    .secondary::<TaskMetaEntity>(TaskMetaEntityKey::id, task_id)
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .ok_or_else(|| {
                        raise_error!(
                            format!(
                                "The task with id={} that you want to modify was not found.",
                                task_id
                            ),
                            ErrorCode::ResourceNotFound
                        )
                    })
            },
            move |current| {
                let mut updated = current.clone();
                updated.heartbeat_at = utc_now!();
                Ok(updated)
            },
        )
        .await?;
        Ok(())
    }

    pub async fn restore(database: &Arc<Database<'static>>) -> RustMailerResult<()> {
        tracing::info!("starting task restore...");
        let running_tasks = filter_by_secondary_key_impl::<TaskMetaEntity>(
            database,
            TaskMetaEntityKey::status,
            TaskStatus::Running.code(),
        )
        .await?;
        let rw = database
            .rw_transaction()
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        for task in running_tasks {
            // After restart, find tasks marked as Running
            let mut updated = task.clone();
            if let Some(retry_count) = task.retry_count {
                // Check if there are retry attempts
                if retry_count >= task.max_retries.unwrap_or(0) as usize {
                    // If retry count exceeds maximum allowed
                    updated.status = TaskStatus::Removed; // Mark as removed
                    updated.stopped_reason = Some(
                        "Max retries exceeded, Automatically stopped during task restoration"
                            .into(),
                    ); // Provide stop reason
                } else {
                    // If within max retry limit, mark as Scheduled to re-enter scheduling
                    // No need to set next_run - will be scheduled immediately if < current time
                    // Otherwise will be handled according to actual value
                    updated.status = TaskStatus::Scheduled;
                }
            } else {
                // Not even one retry completed yet, reschedule directly
                updated.status = TaskStatus::Scheduled;
            }
            updated.updated_at = utc_now!(); // Update timestamp
            rw.update(task.clone(), updated)
                .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        }
        rw.commit()
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        tracing::info!("finished task restore.");
        Ok(())
    }

    pub async fn get(
        database: &Arc<Database<'static>>,
        task_id: u64,
    ) -> RustMailerResult<Option<TaskMeta>> {
        secondary_find_impl::<TaskMetaEntity>(database, TaskMetaEntityKey::id, task_id)
            .await
            .map(|opt| opt.map(Into::into))
    }

    pub async fn list_all(
        database: &Arc<Database<'static>>,
        task_key: &str,
    ) -> RustMailerResult<Vec<TaskMetaEntity>> {
        filter_by_secondary_key_impl(database, TaskMetaEntityKey::task_key, task_key.to_string())
            .await
    }

    pub async fn store_one(
        database: &Arc<Database<'static>>,
        task: TaskMeta,
    ) -> RustMailerResult<()> {
        let entity: TaskMetaEntity = task.into();
        insert_impl(database, entity).await
    }

    pub async fn store_many(
        database: &Arc<Database<'static>>,
        tasks: Vec<TaskMeta>,
    ) -> RustMailerResult<()> {
        let batch: Vec<TaskMetaEntity> = tasks.into_iter().map(Into::into).collect();
        batch_insert_impl(database, batch).await
    }

    pub async fn get_paginated_tasks_by_status(
        database: &Arc<Database<'static>>,
        page: Option<u64>,
        page_size: Option<u64>,
        desc: Option<bool>,
        task_key: &str,
        status: TaskStatus,
    ) -> RustMailerResult<Paginated<TaskMetaEntity>> {
        paginate_secondary_scan_impl(
            database,
            page,
            page_size,
            desc,
            TaskMetaEntityKey::typed_status,
            TaskMetaEntity::status_filter_key(task_key, status),
        )
        .await
    }

    pub async fn get_paginated_tasks(
        database: &Arc<Database<'static>>,
        page: Option<u64>,
        page_size: Option<u64>,
        desc: Option<bool>,
        task_key: &str,
    ) -> RustMailerResult<Paginated<TaskMetaEntity>> {
        paginate_secondary_scan_impl(
            database,
            page,
            page_size,
            desc,
            TaskMetaEntityKey::task_key,
            task_key.to_string(),
        )
        .await
    }

    pub async fn get_all_tasks_by_status(
        database: &Arc<Database<'static>>,
        task_key: &str,
        status: TaskStatus,
    ) -> RustMailerResult<Vec<TaskMetaEntity>> {
        filter_by_secondary_key_impl(
            database,
            TaskMetaEntityKey::typed_status,
            TaskMetaEntity::status_filter_key(task_key, status),
        )
        .await
    }
}

impl TaskStore for NativeDbTaskStore {
    async fn store_task(&self, task: TaskMeta) -> RustMailerResult<()> {
        let db = self.store.clone();
        Self::store_one(&db, task).await
    }

    async fn store_tasks(&self, tasks: Vec<TaskMeta>) -> RustMailerResult<()> {
        let db = self.store.clone();
        Self::store_many(&db, tasks).await
    }

    async fn fetch_pending_tasks(&self) -> RustMailerResult<Vec<TaskMeta>> {
        let db = self.store.clone();
        Self::fetch_pending_tasks(&db).await
    }

    async fn update_task_execution_status(
        &self,
        task_id: u64,
        is_success: bool,
        last_error: Option<String>,
        last_duration_ms: Option<usize>,
        retry_count: Option<usize>,
        next_run: Option<i64>,
    ) -> RustMailerResult<()> {
        let db = self.store.clone();
        let task = Self::get(&db, task_id)
            .await?
            .ok_or_else(|| raise_error!("Task not found".into(), ErrorCode::ResourceNotFound))?;
        Self::update_status(
            &db,
            task_id,
            is_success,
            last_error.clone(),
            last_duration_ms,
            retry_count,
            next_run,
        )
        .await?;

        if !is_success && task.task_key == SmtpTask::TASK_KEY {
            let task_params = task.task_params.clone();
            tokio::spawn(async move {
                if let Ok(smtp_task) = serde_json::from_str::<SmtpTask>(&task_params) {
                    if let Ok(true) =
                        EventHookTask::is_watching_email_sending_error(smtp_task.account_id).await
                    {
                        let max_retries = smtp_task.retry_policy().max_retries;
                        EVENT_CHANNEL
                            .queue(Event::new(
                                smtp_task.account_id,
                                &smtp_task.account_email.clone(),
                                RustMailerEvent::new(
                                    EventType::EmailSendingError,
                                    EventPayload::EmailSendingError(EmailSendingError {
                                        account_id: smtp_task.account_id,
                                        account_email: smtp_task.account_email,
                                        from: smtp_task.from,
                                        to: smtp_task.to,
                                        subject: smtp_task.subject,
                                        message_id: smtp_task.message_id,
                                        error_msg: last_error,
                                        retry_count: retry_count,
                                        scheduled_at: next_run,
                                        task_id,
                                        max_retries,
                                    }),
                                ),
                            ))
                            .await;
                    }
                } else {
                    warn!(
                        "Failed to parse smtp_task from task_params for task {}",
                        task_id
                    );
                }
            });
        }
        Ok(())
    }

    async fn heartbeat(&self, task_id: u64) -> RustMailerResult<()> {
        let db = self.store.clone();
        Self::heartbeat(&db, task_id).await
    }

    async fn set_task_stopped(&self, task_id: u64, reason: Option<String>) -> RustMailerResult<()> {
        let db = self.store.clone();
        Self::set_status(&db, task_id, TaskStatus::Stopped, reason).await
    }

    async fn cleanup(&self) -> RustMailerResult<()> {
        let db = self.store.clone();
        Self::clean_up(&db).await
    }
}
