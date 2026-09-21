// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::{
    modules::{
        database::{
            async_find_impl, batch_delete_impl, delete_impl, list_all_impl, manager::DB_MANAGER,
            update_impl, upsert_impl,
        },
        error::{code::ErrorCode, RustMailerResult},
        settings::dir::DATA_DIR_MANAGER,
    },
    raise_error, utc_now,
};
use native_db::*;
use native_model::{native_model, Model};
use poem_openapi::Object;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Instant,
};
use sysinfo::Disks;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};

pub mod task;

const DISK_USAGE_THRESHOLD: f64 = 85.0;
pub const ONE_WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const MAX_ITEMS_THRESHOLD: usize = 10000;

pub static DISK_CACHE: LazyLock<DiskCache> = LazyLock::new(DiskCache::init);

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, Object)]
#[native_model(id = 12, version = 1)]
#[native_db]
pub struct CacheItem {
    #[primary_key]
    pub key: String,
    pub size: u64,
    pub pending: bool,
    pub write_at: i64,
    pub last_access_at: i64,
}

impl CacheItem {
    pub fn new(key: String, size: u64, pending: bool) -> Self {
        Self {
            key,
            size,
            pending,
            write_at: utc_now!(),
            last_access_at: utc_now!(),
        }
    }

    pub async fn save(self) -> RustMailerResult<()> {
        upsert_impl(DB_MANAGER.meta_db(), self).await
    }

    pub async fn clear() -> RustMailerResult<()> {
        const BATCH_SIZE: usize = 200;
        let mut total_deleted = 0usize;
        let start_time = Instant::now();
        loop {
            let deleted = batch_delete_impl(DB_MANAGER.meta_db(), move |rw| {
                let to_delete: Vec<CacheItem> = rw
                    .scan()
                    .primary()
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .all()
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .filter_map(Result::ok) // filter only Ok values
                    .take(BATCH_SIZE)
                    .collect();
                Ok(to_delete)
            })
            .await?;
            total_deleted += deleted;
            // If this batch is empty, break the loop
            if deleted == 0 {
                break;
            }
        }

        info!(
            "Finished deleting cacheitems total_deleted={} in {:?}",
            total_deleted,
            start_time.elapsed()
        );
        Ok(())
    }

    pub async fn check_exist(key: &str) -> RustMailerResult<bool> {
        let item = async_find_impl::<CacheItem>(DB_MANAGER.meta_db(), key.to_string()).await?;
        Ok(item.is_some())
    }

    pub async fn delete(&self) -> RustMailerResult<()> {
        let key = self.key.clone();
        delete_impl(DB_MANAGER.meta_db(), move |rw| {
            rw.get()
                .primary::<CacheItem>(key)
                .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                .ok_or_else(|| raise_error!("cache item miss".into(), ErrorCode::InternalError))
        })
        .await
    }

    pub async fn update_access(key: &str) -> RustMailerResult<()> {
        let key = key.to_string();
        update_impl(
            DB_MANAGER.meta_db(),
            move |rw| {
                rw.get()
                    .primary::<CacheItem>(key.clone())
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                    .ok_or_else(|| {
                        raise_error!(
                            format!(
                            "The CacheItem with id={key} that you want to modify was not found."
                        ),
                            ErrorCode::InternalError
                        )
                    })
            },
            |current| {
                let mut updated = current.clone();
                updated.last_access_at = utc_now!();
                Ok(updated)
            },
        )
        .await?;
        Ok(())
    }

    pub async fn list() -> RustMailerResult<Vec<CacheItem>> {
        list_all_impl(DB_MANAGER.meta_db()).await
    }
}

pub struct DiskCache {
    cache_dir: PathBuf,
}

impl DiskCache {
    pub fn init() -> Self {
        Self {
            cache_dir: DATA_DIR_MANAGER.disk_cache.clone(),
        }
    }

    pub async fn put_cache(&self, key: &str, data: &[u8], pending: bool) -> RustMailerResult<()> {
        let cache_dir = self.cache_dir.to_str().ok_or_else(|| {
            raise_error!(
                "Failed to convert cache_dir to str".into(),
                ErrorCode::InternalError
            )
        })?;
        let mut writer = cacache::Writer::create(cache_dir, key)
            .await
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        writer
            .write_all(data)
            .await
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        writer
            .commit()
            .await
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        let item = CacheItem::new(key.to_string(), data.len() as u64, pending);
        item.save().await?;
        Ok(())
    }

    pub async fn get_cache(&self, key: &str) -> RustMailerResult<Option<cacache::Reader>> {
        if !CacheItem::check_exist(key).await? {
            return Ok(None);
        }
        let cache_dir_str = self.cache_dir.to_str().ok_or_else(|| {
            raise_error!(
                "Failed to convert cache_dir to str".into(),
                ErrorCode::InternalError
            )
        })?;
        let reader = cacache::Reader::open(cache_dir_str, key)
            .await
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
        CacheItem::update_access(key).await?;
        Ok(Some(reader))
    }

    pub async fn clear(&self) -> RustMailerResult<()> {
        CacheItem::clear().await?;
        let cache_dir_str = match self.cache_dir.to_str() {
            Some(dir) => dir,
            None => {
                error!("Failed to convert cache_dir to string");
                return Err(raise_error!(
                    "Failed to convert cache_dir to string".into(),
                    ErrorCode::InternalError
                ));
            }
        };
        cacache::clear(cache_dir_str)
            .await
            .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))
    }

    pub async fn clean_cache_if_needed(&self) {
        let cache_items = match CacheItem::list().await {
            Ok(items) => {
                let mut items = items;
                items.sort_by_key(|item| item.last_access_at);
                items
            }
            Err(e) => {
                error!("Failed to fetch cache items: {}", e);
                return;
            }
        };
        // Early return if we can't get disk space
        let disk_space = match get_mount_disk_space(&self.cache_dir) {
            Some(space) => space,
            None => {
                warn!(
                    "Failed to get disk space info for cache directory: {:?}",
                    &self.cache_dir
                );
                return;
            }
        };
        let current_usage = calculate_disk_usage_percentage(&disk_space);
        if current_usage < DISK_USAGE_THRESHOLD {
            debug!("Disk usage is {}%, no cleaning needed", current_usage);
            return;
        }
        info!("Disk usage is {}%, initiating cache cleanup", current_usage);

        // Convert cache_dir to str once and handle early return
        let cache_dir_str = match self.cache_dir.to_str() {
            Some(dir) => dir,
            None => {
                error!("Failed to convert cache_dir to string");
                return;
            }
        };
        // Clean cache items
        let to_delete = select_items_to_delete(&cache_items, &disk_space);
        for item in to_delete {
            if let Err(e) = Self::remove_cache_item(cache_dir_str, &item).await {
                debug!("Cache item cleanup failed for key={}: {}", item.key, e);
                continue; // Continue with next item instead of returning
            }
        }
        let new_usage = calculate_disk_usage_percentage(&disk_space);
        info!("Cache cleaned, disk usage reduced to {}%", new_usage);
    }

    // Helper function to handle item removal
    async fn remove_cache_item(cache_dir: &str, item: &CacheItem) -> Result<(), String> {
        cacache::RemoveOpts::new()
            .remove_fully(true)
            .remove(cache_dir, &item.key)
            .await
            .map_err(|e| format!("Failed to remove cache item from disk: {:?}", e))?;
        item.delete()
            .await
            .map_err(|e| format!("Failed to delete cache item: {}", e))?;

        Ok(())
    }
}

// Helper function to calculate the disk usage percentage
fn calculate_disk_usage_percentage(disk_space: &DiskSpace) -> f64 {
    let used_space = disk_space.total_space - disk_space.available_space;
    (used_space as f64 / disk_space.total_space as f64) * 100.0
}

// Helper function to select which cache items to delete to free space
fn select_items_to_delete(cache_items: &[CacheItem], disk_space: &DiskSpace) -> Vec<CacheItem> {
    let now = utc_now!();
    let mut to_delete = Vec::with_capacity(16);
    let mut freed_space = 0;
    let total_items = cache_items.len();
    for item in cache_items {
        if item.pending && now < item.write_at + ONE_WEEK_MS {
            continue;
        }

        freed_space += item.size;
        to_delete.push(item.clone());
        let remaining_items = total_items.saturating_sub(to_delete.len());
        let used_space = disk_space.total_space - (disk_space.available_space + freed_space);
        let usage_percentage = (used_space as f64 / disk_space.total_space as f64) * 100.0;
        if usage_percentage < DISK_USAGE_THRESHOLD && remaining_items <= MAX_ITEMS_THRESHOLD {
            break;
        }
    }

    to_delete
}

#[derive(Debug)]
pub struct DiskSpace {
    pub total_space: u64,
    pub available_space: u64,
}

impl DiskSpace {
    pub fn new(total_space: u64, available_space: u64) -> Self {
        Self {
            total_space,
            available_space,
        }
    }
}

fn mount_points() -> Vec<(PathBuf, DiskSpace)> {
    let disks = Disks::new_with_refreshed_list();
    let mut mount_points = Vec::new();

    for disk in disks.list() {
        mount_points.push((
            disk.mount_point().to_path_buf(),
            DiskSpace::new(disk.total_space(), disk.available_space()),
        ));
    }

    mount_points
}

pub fn get_mount_disk_space(file_path: &Path) -> Option<DiskSpace> {
    let mount_points = mount_points();

    let mut mount_depths: Vec<(PathBuf, usize, DiskSpace)> = mount_points
        .into_iter()
        .map(|mount| (mount.0.clone(), mount.0.components().count(), mount.1))
        .collect();

    mount_depths.sort_by(|a, b| b.1.cmp(&a.1));
    for (mount, _, disk_space) in mount_depths {
        if file_path.starts_with(&mount) {
            return Some(disk_space);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_by_last_access_at() {
        let mut items = vec![
            CacheItem {
                key: "a".to_string(),
                size: 10,
                pending: false,
                write_at: 100,
                last_access_at: 300,
            },
            CacheItem {
                key: "b".to_string(),
                size: 20,
                pending: false,
                write_at: 200,
                last_access_at: 100,
            },
            CacheItem {
                key: "c".to_string(),
                size: 30,
                pending: true,
                write_at: 300,
                last_access_at: 200,
            },
        ];

        items.sort_by_key(|item| item.last_access_at);

        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, vec!["b", "c", "a"]); // 100 -> 200 -> 300

        items = items.drain(1..).collect();
        println!("{:#?}", items);
    }
}
