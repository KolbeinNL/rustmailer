// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::modules::context::tasks::ImapHeartBeatTask;
use crate::modules::context::RustMailTask;
use crate::modules::database::snapshot::task::DatabaseSnapshotTask;
use crate::modules::overview::clean::MetricsCleanTask;
use crate::modules::overview::saver::MetricsSaveTask;
use crate::{
    modules::cache::disk::task::DiskCacheCleanTask,
    modules::oauth2::{refresh::OAuth2RefreshTask, task::OAuth2CleanTask},
};

use crate::modules::database::backup::task::MetaBackupTask;

pub mod queue;

pub struct PeriodicTasks;

impl PeriodicTasks {
    pub fn start_background_tasks() {
        DiskCacheCleanTask::start();
        OAuth2CleanTask::start();
        OAuth2RefreshTask::start();
        MetaBackupTask::start();
        DatabaseSnapshotTask::start();
        MetricsSaveTask::start();
        MetricsCleanTask::start();
        ImapHeartBeatTask::start();
    }
}
