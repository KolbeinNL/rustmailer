// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::{
    modules::{
        database::{async_find_impl, delete_impl, manager::DB_MANAGER, update_impl, upsert_impl},
        error::{code::ErrorCode, RustMailerResult},
    },
    raise_error, utc_now,
};
use native_db::*;
use native_model::{native_model, Model};
use poem_openapi::Object;
use serde::{Deserialize, Serialize};

const ERROR_COUNT_PER_ACCOUNT: usize = 20;

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
#[native_model(id = 13, version = 1)]
#[native_db]
pub struct AccountRunningState {
    #[primary_key]
    pub account_id: u64,
    pub last_full_sync_start: i64,
    pub last_full_sync_end: Option<i64>,
    pub last_incremental_sync_start: i64,
    pub last_incremental_sync_end: Option<i64>,
    pub errors: Vec<AccountError>,
    pub is_initial_sync_completed: bool,
    pub initial_sync_folders: Vec<String>,
    pub current_syncing_folder: Option<String>,
    pub current_batch_number: Option<u32>,
    pub current_total_batches: Option<u32>,
    pub initial_sync_start_time: Option<i64>,
    pub initial_sync_end_time: Option<i64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct AccountError {
    pub error: String,
    pub at: i64,
}

impl AccountRunningState {
    pub async fn add(account_id: u64) -> RustMailerResult<()> {
        let info = AccountRunningState {
            account_id,
            last_full_sync_start: utc_now!(),
            last_full_sync_end: None,
            last_incremental_sync_start: 0,
            last_incremental_sync_end: None,
            errors: vec![],
            is_initial_sync_completed: false,
            initial_sync_folders: vec![],
            current_syncing_folder: None,
            current_batch_number: None,
            current_total_batches: None,
            initial_sync_start_time: None,
            initial_sync_end_time: None,
        };
        upsert_impl(DB_MANAGER.meta_db(), info).await
    }

    pub async fn get(account_id: u64) -> RustMailerResult<Option<AccountRunningState>> {
        async_find_impl(DB_MANAGER.meta_db(), account_id).await
    }

    /// Timestamp of the most recent completed sync of any kind (full,
    /// incremental, or the initial one). Backs the staleness watchdog in
    /// `cache::imap::sync::report_sync_staleness`.
    pub fn last_completed_sync_at(&self) -> Option<i64> {
        [
            self.last_full_sync_end,
            self.last_incremental_sync_end,
            self.initial_sync_end_time,
        ]
        .into_iter()
        .flatten()
        .max()
    }

    async fn update_account_running_state(
        account_id: u64,
        updater: impl FnOnce(&AccountRunningState) -> RustMailerResult<AccountRunningState>
            + Send
            + 'static,
    ) -> RustMailerResult<()> {
        update_impl(
            DB_MANAGER.meta_db(),
            move |rw| {
                rw.get()
                    .primary::<AccountRunningState>(account_id)
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .ok_or_else(|| {
                        raise_error!(
                            format!("Cannot find sync info of account={}", account_id),
                            ErrorCode::ResourceNotFound
                        )
                    })
            },
            updater,
        )
        .await?;
        Ok(())
    }

    pub async fn delete(account_id: u64) -> RustMailerResult<()> {
        if Self::get(account_id).await?.is_none() {
            return Ok(());
        }

        delete_impl(DB_MANAGER.meta_db(), move |rw| {
            rw.get()
                .primary::<AccountRunningState>(account_id)
                .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                .ok_or_else(|| {
                    raise_error!(
                        format!(
                            "AccountRunningState '{}' not found during deletion process.",
                            account_id
                        ),
                        ErrorCode::ResourceNotFound
                    )
                })
        })
        .await
    }

    pub async fn set_initial_sync_folders(
        account_id: u64,
        initial_sync_folders: Vec<String>,
    ) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            updated.initial_sync_folders = initial_sync_folders;
            updated.initial_sync_start_time = Some(utc_now!());
            Ok(updated)
        })
        .await
    }

    pub async fn set_initial_sync_completed(account_id: u64) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            updated.is_initial_sync_completed = true;
            updated.last_full_sync_end = Some(utc_now!());
            updated.initial_sync_end_time = Some(utc_now!());
            Ok(updated)
        })
        .await
    }

    pub async fn set_current_sync_batch_number(
        account_id: u64,
        batch_number: u32,
    ) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            updated.current_batch_number = Some(batch_number);
            Ok(updated)
        })
        .await
    }

    pub async fn set_initial_current_syncing_folder(
        account_id: u64,
        current_syncing_folder: String,
        total_sync_batches: Option<u32>,
    ) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            updated.current_syncing_folder = Some(current_syncing_folder);
            updated.current_total_batches = total_sync_batches;
            Ok(updated)
        })
        .await
    }

    pub async fn set_full_sync_start(account_id: u64) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            // `last_full_sync_end` is deliberately preserved: a dispatched sync
            // that has not finished yet must not erase the record of when the
            // last sync completed. Clearing it here made the staleness
            // watchdog fall back to `initial_sync_end_time` and report months
            // of dormancy for an account whose latest sync merely failed or
            // was still running (report from 2026-10-07). "In progress" is
            // already expressed by `last_full_sync_start > last_full_sync_end`.
            updated.last_full_sync_start = utc_now!();
            Ok(updated)
        })
        .await
    }

    pub async fn set_full_sync_end(account_id: u64) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            updated.last_full_sync_end = Some(utc_now!());
            Ok(updated)
        })
        .await
    }

    pub async fn set_incremental_sync_start(account_id: u64) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            // See `set_full_sync_start`: the end timestamp must survive a new
            // sync being dispatched, or the staleness watchdog loses its
            // baseline whenever a sync starts but does not complete.
            updated.last_incremental_sync_start = utc_now!();
            Ok(updated)
        })
        .await
    }

    pub async fn set_incremental_sync_end(account_id: u64) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            updated.last_incremental_sync_end = Some(utc_now!());
            Ok(updated)
        })
        .await
    }

    pub async fn append_error_message(account_id: u64, error: String) -> RustMailerResult<()> {
        Self::update_account_running_state(account_id, move |current| {
            let mut updated = current.clone();
            updated.append_error_log(error);
            Ok(updated)
        })
        .await
    }

    pub fn append_error_log(&mut self, error: String) {
        let new_error = AccountError {
            error,
            at: utc_now!(),
        };

        self.errors.push(new_error);
        if self.errors.len() > ERROR_COUNT_PER_ACCOUNT {
            self.errors.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_last_completed_sync_at_takes_max_of_all_kinds() {
        let account_state = AccountRunningState {
            account_id: 1000u64,
            last_full_sync_end: Some(2000),
            last_incremental_sync_end: Some(5000),
            initial_sync_end_time: Some(1000),
            ..Default::default()
        };
        assert_eq!(account_state.last_completed_sync_at(), Some(5000));
    }

    #[test]
    fn test_last_completed_sync_at_ignores_none_ends() {
        // A dispatched-but-unfinished sync leaves the `*_end` timestamps at
        // their previous values; the baseline must survive that.
        let account_state = AccountRunningState {
            account_id: 1000u64,
            last_full_sync_start: 9000,
            last_full_sync_end: Some(2000),
            last_incremental_sync_start: 8000,
            last_incremental_sync_end: None,
            initial_sync_end_time: Some(1000),
            ..Default::default()
        };
        assert_eq!(account_state.last_completed_sync_at(), Some(2000));
    }

    #[test]
    fn test_last_completed_sync_at_all_none() {
        let account_state = AccountRunningState {
            account_id: 1000u64,
            last_full_sync_start: 9000,
            last_incremental_sync_start: 8000,
            ..Default::default()
        };
        assert_eq!(account_state.last_completed_sync_at(), None);
    }

    #[test]
    fn test_insert_single_error() {
        let mut account_state = AccountRunningState {
            account_id: 1000u64,
            last_full_sync_start: 1000,
            last_full_sync_end: Some(2000),
            last_incremental_sync_start: 1000,
            last_incremental_sync_end: Some(2000),
            errors: Vec::new(),
            ..Default::default()
        };

        account_state.append_error_log(String::from("Error 1"));
        assert_eq!(account_state.errors.len(), 1);
        assert_eq!(account_state.errors[0].error, "Error 1");
    }

    #[test]
    fn test_insert_multiple_errors() {
        let mut account_state = AccountRunningState {
            account_id: 1000u64,
            last_full_sync_start: 1000,
            last_incremental_sync_start: 1000,
            last_full_sync_end: Some(2000),
            last_incremental_sync_end: Some(2000),
            errors: Vec::new(),
            ..Default::default()
        };

        for i in 1..=5 {
            account_state.append_error_log(format!("Error {}", i));
        }

        assert_eq!(account_state.errors.len(), 5);
        assert_eq!(account_state.errors[4].error, "Error 5");
    }

    #[test]
    fn test_error_limit_exceeded() {
        let mut account_state = AccountRunningState {
            account_id: 1000u64,
            last_full_sync_start: 1000,
            last_incremental_sync_start: 1000,
            last_full_sync_end: Some(2000),
            last_incremental_sync_end: Some(2000),
            errors: Vec::new(),
            ..Default::default()
        };

        for i in 1..=25 {
            account_state.append_error_log(format!("Error {}", i));
        }

        // Should only keep the last 10 errors
        assert_eq!(account_state.errors.len(), ERROR_COUNT_PER_ACCOUNT);
        assert_eq!(account_state.errors[0].error, "Error 6");
        assert_eq!(account_state.errors[19].error, "Error 25");
    }

    #[test]
    fn test_insert_error_after_limit() {
        let mut account_state = AccountRunningState {
            account_id: 1000u64,
            last_full_sync_start: 1000,
            last_incremental_sync_start: 1000,
            last_full_sync_end: Some(2000),
            last_incremental_sync_end: Some(2000),
            errors: Vec::new(),
            ..Default::default()
        };

        // Insert exactly 10 errors
        for i in 1..=20 {
            account_state.append_error_log(format!("Error {}", i));
        }

        // Insert one more error to exceed the limit
        account_state.append_error_log(String::from("Error 21"));

        assert_eq!(account_state.errors.len(), ERROR_COUNT_PER_ACCOUNT);
        assert_eq!(account_state.errors[0].error, "Error 2"); // The first error is removed
        assert_eq!(account_state.errors[19].error, "Error 21"); // The last inserted error
    }
}
