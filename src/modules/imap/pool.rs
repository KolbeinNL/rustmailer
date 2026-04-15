// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::modules::error::code::ErrorCode;
use crate::modules::error::{RustMailerError, RustMailerResult};
use crate::modules::imap::{manager::ImapConnectionManager, session::SessionStream};
use crate::raise_error;
use async_imap::Session;
use bb8::Pool;
use std::time::Duration;
use tracing::{error, warn};

impl bb8::ManageConnection for ImapConnectionManager {
    type Connection = MyImapConnection;

    type Error = RustMailerError;

    async fn connect(&self) -> RustMailerResult<Self::Connection> {
        let session = self.build().await?;
        Ok(MyImapConnection {
            session,
            is_bad: false,
        })
    }
    // call this function before using the connection
    async fn is_valid(&self, conn: &mut Self::Connection) -> RustMailerResult<()> {
        if conn.is_bad {
            return Err(raise_error!(
                format!("Connection marked broken"),
                ErrorCode::ImapCommandFailed
            ));
        }
        match tokio::time::timeout(
            Duration::from_secs(5),
            conn.run_command_and_check_ok("NOOP"),
        )
        .await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                error!("IMAP connection validation failed: {:?}", e);
                conn.is_bad = true;
                Err(raise_error!(
                    format!("{:#?}", e),
                    ErrorCode::ImapCommandFailed
                ))
            }
            Err(_) => {
                warn!("IMAP NOOP timeout");
                conn.is_bad = true;
                Err(raise_error!(
                    "NOOP timeout".into(),
                    ErrorCode::ImapCommandFailed
                ))
            }
        }
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.is_bad
    }
}

pub async fn build_imap_pool(account_id: u64) -> RustMailerResult<Pool<ImapConnectionManager>> {
    let manager = ImapConnectionManager::new(account_id);
    let pool = Pool::builder()
        .connection_timeout(Duration::from_secs(20))
        .idle_timeout(Duration::from_secs(60))
        .max_lifetime(Duration::from_secs(60))
        .retry_connection(false)
        .max_size(8)
        .min_idle(Some(2))
        .test_on_check_out(true)
        .build(manager)
        .await?;

    Ok(pool)
}

pub struct MyImapConnection {
    pub session: Session<Box<dyn SessionStream>>,
    pub is_bad: bool,
}

impl std::ops::Deref for MyImapConnection {
    type Target = Session<Box<dyn SessionStream>>;
    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl std::ops::DerefMut for MyImapConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}
