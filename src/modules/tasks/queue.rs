// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::modules::error::code::ErrorCode;
use crate::modules::hook::task::{EventHookTask, SendEventHookTask, EVENTHOOK_QUEUE};
use crate::modules::rest::response::DataPage;
use crate::modules::scheduler::context::TaskContext;
use crate::modules::scheduler::model::TaskStatus;
use crate::modules::scheduler::nativedb::meta::NativeDbTaskStore;
use crate::modules::scheduler::nativedb::TaskMetaEntity;
use crate::modules::scheduler::task::Task;
use crate::modules::settings::cli::SETTINGS;
use crate::modules::smtp::queue::message::SendEmailTask;
use crate::modules::smtp::request::task::{SmtpTask, OUTBOX_QUEUE};
use crate::{
    modules::{context::Initialize, database::manager::DB_MANAGER, error::RustMailerResult},
    raise_error,
};
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

static TASK_QUEUE: OnceLock<RustMailerTaskQueue> = OnceLock::new();

impl Initialize for RustMailerTaskQueue {
    async fn initialize() -> RustMailerResult<()> {
        let scheduler = RustMailerTaskQueue::new().await;
        let _ = TASK_QUEUE.set(scheduler);
        Ok(())
    }
}

pub struct RustMailerTaskQueue {
    pub task_context: Arc<RwLock<TaskContext<NativeDbTaskStore>>>,
}

impl RustMailerTaskQueue {
    pub fn get() -> RustMailerResult<&'static RustMailerTaskQueue> {
        TASK_QUEUE.get().ok_or_else(|| {
            raise_error!("TaskQueue not initialized".into(), ErrorCode::InternalError)
        })
    }

    pub async fn new() -> Self {
        let task_store = Arc::new(NativeDbTaskStore::init(DB_MANAGER.tasks_db().clone()));
        NativeDbTaskStore::restore(DB_MANAGER.tasks_db())
            .await
            .expect("Failed to restore tasks from the scheduler metadata database");
        let task_context = TaskContext::with_arc_store(task_store.clone())
            .register::<SmtpTask>()
            .register::<EventHookTask>()
            .set_concurrency(OUTBOX_QUEUE, SETTINGS.rustmailer_send_mail_workers)
            .set_concurrency(EVENTHOOK_QUEUE, SETTINGS.rustmailer_event_hook_workers)
            .start_with_cleaner()
            .await;
        RustMailerTaskQueue {
            task_context: Arc::new(RwLock::new(task_context)),
        }
    }

    pub async fn submit_task<T>(&self, task: T, delay_seconds: Option<u32>) -> RustMailerResult<()>
    where
        T: Task + Send + Sync + 'static,
    {
        let context = self.task_context.write().await;
        context
            .add_task(task, delay_seconds)
            .await
            .map_err(|message| raise_error!(message, ErrorCode::InternalError))
    }

    pub async fn submit_tasks<T>(
        &self,
        tasks: &[T],
        delay_seconds: Option<u32>,
    ) -> RustMailerResult<()>
    where
        T: Task + Send + Sync + 'static,
    {
        let context = self.task_context.write().await;
        context
            .add_tasks(tasks, delay_seconds)
            .await
            .map_err(|message| raise_error!(message, ErrorCode::InternalError))
    }

    pub async fn stop_task(
        &self,
        task_id: u64,
        stop_reason: Option<String>,
    ) -> RustMailerResult<()> {
        let context = self.task_context.write().await;
        context
            .stop_task(task_id, stop_reason)
            .await
            .map_err(|message| raise_error!(message, ErrorCode::InternalError))
    }

    pub async fn list_paginated_email_tasks_by_status(
        &self,
        page: Option<u64>,
        page_size: Option<u64>,
        desc: Option<bool>,
        status: TaskStatus,
    ) -> RustMailerResult<DataPage<SendEmailTask>> {
        let paginated = NativeDbTaskStore::get_paginated_tasks_by_status(
            DB_MANAGER.tasks_db(),
            page,
            page_size,
            desc,
            SmtpTask::TASK_KEY,
            status,
        )
        .await
        .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let items: Vec<SendEmailTask> = paginated
            .items
            .iter()
            .map(SendEmailTask::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DataPage::new(
            paginated.page,
            paginated.page_size,
            paginated.total_items,
            paginated.total_pages,
            items,
        ))
    }

    pub async fn list_paginated_email_tasks(
        &self,
        page: Option<u64>,
        page_size: Option<u64>,
        desc: Option<bool>,
    ) -> RustMailerResult<DataPage<SendEmailTask>> {
        let paginated = NativeDbTaskStore::get_paginated_tasks(
            DB_MANAGER.tasks_db(),
            page,
            page_size,
            desc,
            SmtpTask::TASK_KEY,
        )
        .await
        .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let items: Vec<SendEmailTask> = paginated
            .items
            .iter()
            .map(SendEmailTask::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DataPage::new(
            paginated.page,
            paginated.page_size,
            paginated.total_items,
            paginated.total_pages,
            items,
        ))
    }

    pub async fn list_email_tasks_by_status(
        &self,
        status: TaskStatus,
    ) -> RustMailerResult<Vec<SendEmailTask>> {
        let all = NativeDbTaskStore::get_all_tasks_by_status(
            DB_MANAGER.tasks_db(),
            SmtpTask::TASK_KEY,
            status,
        )
        .await
        .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let items: Vec<SendEmailTask> = all
            .iter()
            .map(SendEmailTask::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub async fn list_all_email_tasks(&self) -> RustMailerResult<Vec<SendEmailTask>> {
        let all = NativeDbTaskStore::list_all(DB_MANAGER.tasks_db(), SmtpTask::TASK_KEY)
            .await
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let items: Vec<SendEmailTask> = all
            .iter()
            .map(SendEmailTask::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub async fn list_paged_hook_tasks_by_status(
        &self,
        page: Option<u64>,
        page_size: Option<u64>,
        desc: Option<bool>,
        status: TaskStatus,
    ) -> RustMailerResult<DataPage<SendEventHookTask>> {
        let paginated = NativeDbTaskStore::get_paginated_tasks_by_status(
            DB_MANAGER.tasks_db(),
            page,
            page_size,
            desc,
            EventHookTask::TASK_KEY,
            status,
        )
        .await
        .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let items: Vec<SendEventHookTask> = paginated
            .items
            .iter()
            .map(SendEventHookTask::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DataPage::new(
            paginated.page,
            paginated.page_size,
            paginated.total_items,
            paginated.total_pages,
            items,
        ))
    }

    pub async fn list_paginated_hook_tasks(
        &self,
        page: Option<u64>,
        page_size: Option<u64>,
        desc: Option<bool>,
    ) -> RustMailerResult<DataPage<SendEventHookTask>> {
        let paginated = NativeDbTaskStore::get_paginated_tasks(
            DB_MANAGER.tasks_db(),
            page,
            page_size,
            desc,
            EventHookTask::TASK_KEY,
        )
        .await
        .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let items: Vec<SendEventHookTask> = paginated
            .items
            .iter()
            .map(SendEventHookTask::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DataPage::new(
            paginated.page,
            paginated.page_size,
            paginated.total_items,
            paginated.total_pages,
            items,
        ))
    }

    pub async fn list_hook_tasks_by_status(
        &self,
        status: TaskStatus,
    ) -> RustMailerResult<Vec<SendEventHookTask>> {
        let all = NativeDbTaskStore::get_all_tasks_by_status(
            DB_MANAGER.tasks_db(),
            EventHookTask::TASK_KEY,
            status,
        )
        .await
        .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let items: Vec<SendEventHookTask> = all
            .iter()
            .map(SendEventHookTask::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub async fn list_all_hook_tasks(&self) -> RustMailerResult<Vec<SendEventHookTask>> {
        let all = NativeDbTaskStore::list_all(DB_MANAGER.tasks_db(), EventHookTask::TASK_KEY)
            .await
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let items: Vec<SendEventHookTask> = all
            .iter()
            .map(SendEventHookTask::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub async fn get_email_task(&self, id: u64) -> RustMailerResult<Option<SendEmailTask>> {
        NativeDbTaskStore::get(DB_MANAGER.tasks_db(), id)
            .await?
            .map(|t| SendEmailTask::try_from(&TaskMetaEntity::from(t)))
            .transpose()
    }

    pub async fn get_hook_task(&self, id: u64) -> RustMailerResult<Option<SendEventHookTask>> {
        NativeDbTaskStore::get(DB_MANAGER.tasks_db(), id)
            .await?
            .map(|t| SendEventHookTask::try_from(&TaskMetaEntity::from(t)))
            .transpose()
    }

    pub async fn remove_task(&self, id: u64) -> RustMailerResult<()> {
        NativeDbTaskStore::set_status(DB_MANAGER.tasks_db(), id, TaskStatus::Removed, None).await
    }

    /// Requeues a single failed email task so the scheduler sends it again.
    ///
    /// The task keeps its original parameters, so the email body is reloaded
    /// from the disk cache and nothing has to be resubmitted by the caller.
    pub async fn retry_email_task(&self, id: u64) -> RustMailerResult<()> {
        NativeDbTaskStore::reset_for_retry(DB_MANAGER.tasks_db(), id).await
    }

    /// Requeues several email tasks at once and returns how many were requeued.
    ///
    /// Ids that are unknown or no longer retryable are skipped rather than
    /// failing the whole batch.
    pub async fn retry_email_tasks(&self, ids: Vec<u64>) -> RustMailerResult<usize> {
        NativeDbTaskStore::reset_tasks_for_retry(DB_MANAGER.tasks_db(), ids).await
    }
}
