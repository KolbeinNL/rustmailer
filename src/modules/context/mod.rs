// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::modules::error::RustMailerResult;

pub mod controller;
pub mod executors;
pub mod status;
pub mod tasks;


pub trait Initialize {
    async fn initialize() -> RustMailerResult<()>;
}

pub trait RustMailTask {
    fn start();
}
