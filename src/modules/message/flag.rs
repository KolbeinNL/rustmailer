// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::{
    modules::{
        account::migration::AccountModel,
        cache::imap::mailbox::EnvelopeFlag,
        cache::imap::manager::EnvelopeFlagsManager,
        context::executors::RUST_MAIL_CONTEXT,
        envelope::generate_uid_set,
        error::{code::ErrorCode, RustMailerResult},
        utils::mailbox_id,
    },
    raise_error,
};
use poem_openapi::Object;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct FlagMessageRequest {
    /// A list of unique identifiers (UIDs) of the messages to be flagged or unflagged.
    pub uids: Vec<u32>,

    /// The name of the mailbox where the messages are located.
    /// The decoded, human-readable name of the mailbox containing the email (e.g., "INBOX").
    /// This name is presented as it appears to users, with any encoding (e.g., UTF-7) automatically handled by the system,
    /// so no manual decoding is required.
    pub mailbox: String,

    /// The action to be performed on the message flags.
    pub action: FlagAction,
}

impl FlagMessageRequest {
    pub fn validate(&self) -> RustMailerResult<()> {
        if self.uids.is_empty() {
            return Err(raise_error!(
                "UIDs list cannot be empty".into(),
                ErrorCode::InvalidParameter
            ));
        }
        self.action.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct FlagAction {
    /// Flags to be added to the messages.
    pub add: Option<Vec<EnvelopeFlag>>,

    /// Flags to be removed from the messages.
    pub remove: Option<Vec<EnvelopeFlag>>,

    /// Flags to overwrite the existing flags on the messages.
    pub overwrite: Option<Vec<EnvelopeFlag>>,
}

impl FlagAction {
    pub fn validate(&self) -> RustMailerResult<()> {
        if self.add.is_none() && self.remove.is_none() && self.overwrite.is_none() {
            return Err(raise_error!(
                "At least one of 'add', 'remove', or 'overwrite' must be set.".into(),
                ErrorCode::InvalidParameter
            ));
        }

        let validate_field = |flags: &Option<Vec<EnvelopeFlag>>| -> RustMailerResult<()> {
            if let Some(ref flags) = flags {
                for (_, flag) in flags.iter().enumerate() {
                    flag.to_imap_string()?;
                }
            }
            Ok(())
        };

        validate_field(&self.add)?;
        validate_field(&self.remove)?;
        validate_field(&self.overwrite)?;
        Ok(())
    }
}

pub async fn modify_flags(account_id: u64, request: FlagMessageRequest) -> RustMailerResult<()> {
    AccountModel::check_account_active(account_id, true).await?;
    request.validate()?;

    let executor = RUST_MAIL_CONTEXT.imap(account_id).await?;
    let uid_set = generate_uid_set(request.uids.clone());

    executor
        .uid_set_flags(
            &uid_set,
            &request.mailbox,
            request.action.add.clone(),
            request.action.remove.clone(),
            request.action.overwrite.clone(),
        )
        .await?;
    
    let mailbox_id = mailbox_id(account_id, &request.mailbox);
    let _ = EnvelopeFlagsManager::
        modify_flags(
            request.uids.clone(), 
            account_id, 
            mailbox_id, 
            request.action.overwrite.clone(), 
            request.action.add.clone(), 
            request.action.remove.clone()
        )
        .await;
    Ok(())
}
