// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use std::hash::Hash;

use crate::{
    decode_mailbox_name, encode_mailbox_name,
    modules::{
        database::{
            async_find_impl, batch_delete_impl, batch_insert_impl, batch_upsert_impl, delete_impl,
            filter_by_secondary_key_impl, manager::DB_MANAGER,
        },
        error::{code::ErrorCode, RustMailerResult},
        utils::mailbox_id,
    },
    raise_error, validate_identifier,
};
use async_imap::types::{Flag, Name, NameAttribute};
use itertools::Itertools;
use native_db::*;
use native_model::{native_model, Model};
use poem_openapi::{Enum, Object};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
#[native_model(id = 2, version = 1)]
#[native_db]
pub struct MailBox {
    /// The unique identifier for the mailbox
    #[primary_key]
    pub id: u64,
    /// The ID of the account associated with the mailbox
    #[secondary_key]
    pub account_id: u64,
    /// The unique, decoded, human-readable name of the mailbox (e.g., "INBOX", "Sent Items").
    /// This is the decoded name as presented to users, derived from the IMAP server's mailbox name
    /// (e.g., after decoding UTF-7 or other encodings per RFC 3501).
    pub name: String,
    /// Optional delimiter used to separate mailbox names in a hierarchy (e.g., "/" or ".").
    /// Used in IMAP to structure nested mailboxes (e.g., "INBOX/Archive").
    pub delimiter: Option<String>,
    /// List of attributes associated with the mailbox (e.g., `\NoSelect`, `\Deleted`).
    /// These indicate special properties, such as whether the mailbox can hold messages.
    pub attributes: Vec<Attribute>,
    /// List of flags currently set on the mailbox or its messages (e.g., `\Seen`, `\Flagged`).
    /// Each flag is represented by an `EnvelopeFlag`, which may include standard or custom flags.
    pub flags: Vec<EnvelopeFlag>,
    /// The number of messages that currently exist in the mailbox.
    pub exists: u32,
    /// Optional number of unseen messages in the mailbox (i.e., messages without the `\Seen` flag).
    pub unseen: Option<u32>,
    /// List of permanent flags that can be set on messages in this mailbox (e.g., `\Seen`, `\Deleted`).
    /// Each flag is represented by an `EnvelopeFlag`, specifying allowed standard or custom flags.
    pub permanent_flags: Vec<EnvelopeFlag>,
    /// The next unique identifier (UID) that will be assigned to a new message in the mailbox.
    /// If `None`, the IMAP server has not provided this information.
    pub uid_next: Option<u32>,
    /// The validity identifier for UIDs in this mailbox, used to ensure UID consistency across sessions.
    /// If `None`, the IMAP server has not provided this information.
    pub uid_validity: Option<u32>,
    /// The highest modification sequence number for the mailbox, used for synchronization (CONDSTORE).
    /// If `None`, the mailbox does not support modification sequences or the value is unknown.
    pub highest_modseq: Option<u64>,
}

impl MailBox {
    pub fn encoded_name(&self) -> String {
        encode_mailbox_name!(&self.name)
    }

    pub async fn batch_delete(mailboxes: Vec<MailBox>) -> RustMailerResult<()> {
        batch_delete_impl(DB_MANAGER.envelope_db(), move |rw| {
            let mut to_deleted = Vec::new();
            for mailbox in mailboxes {
                let retrived = rw
                    .get()
                    .primary::<MailBox>(mailbox.id)
                    .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
                if let Some(retrived) = retrived {
                    to_deleted.push(retrived);
                }
            }
            Ok(to_deleted)
        })
        .await?;
        Ok(())
    }

    pub async fn delete(id: u64) -> RustMailerResult<()> {
        delete_impl(DB_MANAGER.envelope_db(), move |rw| {
            rw.get()
                .primary::<MailBox>(id)
                .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                .ok_or_else(|| raise_error!("mailbox missing".into(), ErrorCode::InternalError))
        })
        .await
    }

    pub async fn list_all(account_id: u64) -> RustMailerResult<Vec<MailBox>> {
        filter_by_secondary_key_impl(DB_MANAGER.envelope_db(), MailBoxKey::account_id, account_id)
            .await
    }

    pub async fn get(account_id: u64, mailbox_name: &str) -> RustMailerResult<MailBox> {
        let mailbox_id = mailbox_id(account_id, mailbox_name);
        let mailbox = async_find_impl::<MailBox>(DB_MANAGER.envelope_db(), mailbox_id).await?;
        mailbox.ok_or_else(|| {
            raise_error!(
                format!("Mailbox with name: {} not found.", mailbox_name),
                ErrorCode::ResourceNotFound
            )
        })
    }

    pub async fn batch_insert(mailboxes: &[MailBox]) -> RustMailerResult<()> {
        batch_insert_impl(DB_MANAGER.envelope_db(), mailboxes.to_vec()).await
    }

    pub async fn batch_upsert(mailboxes: &[MailBox]) -> RustMailerResult<()> {
        batch_upsert_impl(DB_MANAGER.envelope_db(), mailboxes.to_vec()).await
    }

    pub async fn clean(account_id: u64) -> RustMailerResult<()> {
        batch_delete_impl(DB_MANAGER.envelope_db(), move |rw| {
            let mailboxes: Vec<MailBox> = rw
                .scan()
                .secondary::<MailBox>(MailBoxKey::account_id)
                .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                .start_with(account_id)
                .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?
                .try_collect()
                .map_err(|e| raise_error!(format!("{:#?}", e), ErrorCode::InternalError))?;
            Ok(mailboxes)
        })
        .await?;
        Ok(())
    }

    pub fn has_attr(&self, attr: &AttributeEnum) -> bool {
        self.attributes.iter().any(|a| &a.attr == attr)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct Attribute {
    pub attr: AttributeEnum,
    pub extension: Option<String>,
}

impl Attribute {
    pub fn new(attr: AttributeEnum, extension: Option<String>) -> Self {
        Self { attr, extension }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Enum)]
pub enum AttributeEnum {
    NoInferiors,
    NoSelect,
    Marked,
    Unmarked,
    All,
    Archive,
    Drafts,
    Flagged,
    Junk,
    Sent,
    Trash,
    Extension,
    Unknown,
}

impl From<&Name> for MailBox {
    fn from(value: &Name) -> Self {
        let name = decode_mailbox_name!(value.name().to_string());
        let delimiter = value.delimiter().map(|f| f.to_owned());

        let attributes: Vec<Attribute> = value.attributes().iter().map(|na| na.into()).collect();
        //The remaining parts will be supplemented during the examine_mailbox process.
        MailBox {
            name,
            delimiter,
            attributes,
            ..Default::default() //has_synced is initialized to false here
        }
    }
}

impl From<&NameAttribute<'_>> for Attribute {
    fn from(value: &NameAttribute) -> Self {
        match value {
            NameAttribute::NoInferiors => Attribute::new(AttributeEnum::NoInferiors, None),
            NameAttribute::NoSelect => Attribute::new(AttributeEnum::NoSelect, None),
            NameAttribute::Marked => Attribute::new(AttributeEnum::Marked, None),
            NameAttribute::Unmarked => Attribute::new(AttributeEnum::Unmarked, None),
            NameAttribute::All => Attribute::new(AttributeEnum::All, None),
            NameAttribute::Archive => Attribute::new(AttributeEnum::Archive, None),
            NameAttribute::Drafts => Attribute::new(AttributeEnum::Drafts, None),
            NameAttribute::Flagged => Attribute::new(AttributeEnum::Flagged, None),
            NameAttribute::Junk => Attribute::new(AttributeEnum::Junk, None),
            NameAttribute::Sent => Attribute::new(AttributeEnum::Sent, None),
            NameAttribute::Trash => Attribute::new(AttributeEnum::Trash, None),
            NameAttribute::Extension(s) => {
                Attribute::new(AttributeEnum::Extension, Some(s.to_string()))
            }
            _ => Attribute::new(AttributeEnum::Unknown, None),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct EnvelopeFlag {
    /// The type of flag (standard or custom) as defined by `EmailFlag`.
    pub flag: EmailFlag,
    /// An optional string specifying the name of a custom flag when `flag` is `EmailFlag::Custom`.
    /// For standard flags, this is `None`.
    pub custom: Option<String>,
}

impl Hash for EnvelopeFlag {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.flag.hash(state);
        self.custom.hash(state);
    }
}

impl Hash for EmailFlag {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            EmailFlag::Seen => "\\Seen".hash(state),
            EmailFlag::Answered => "\\Answered".hash(state),
            EmailFlag::Flagged => "\\Flagged".hash(state),
            EmailFlag::Deleted => "\\Deleted".hash(state),
            EmailFlag::Draft => "\\Draft".hash(state),
            EmailFlag::Recent => "\\Recent".hash(state),
            EmailFlag::MayCreate => "\\MayCreate".hash(state),
            EmailFlag::Custom => "Custom".hash(state),
        }
    }
}

impl EnvelopeFlag {
    pub fn new(flag: EmailFlag, custom: Option<String>) -> Self {
        Self { flag, custom }
    }

    pub fn to_imap_string(&self) -> RustMailerResult<String> {
        match self.flag {
            EmailFlag::Custom => {
                let custom = self.custom.as_ref().ok_or_else(|| {
                    raise_error!(
                        "Custom flag requires a custom value".into(),
                        ErrorCode::InvalidParameter
                    )
                })?;
                validate_identifier!(custom, "Custom flag")?;
                Ok(format!("\\{}", custom))
            }
            _ => Ok(match self.flag {
                EmailFlag::Seen => "\\Seen".into(),
                EmailFlag::Answered => "\\Answered".into(),
                EmailFlag::Flagged => "\\Flagged".into(),
                EmailFlag::Deleted => "\\Deleted".into(),
                EmailFlag::Draft => "\\Draft".into(),
                EmailFlag::Recent => "\\Recent".into(),
                EmailFlag::MayCreate => "\\MayCreate".into(),
                EmailFlag::Custom => unreachable!("Handled above"),
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Enum)]
pub enum EmailFlag {
    Seen,
    Answered,
    Flagged,
    Deleted,
    Draft,
    Recent,
    MayCreate,
    Custom,
}

impl<'a> From<Flag<'a>> for EnvelopeFlag {
    fn from(value: Flag<'a>) -> Self {
        match value {
            Flag::Seen => EnvelopeFlag::new(EmailFlag::Seen, None),
            Flag::Answered => EnvelopeFlag::new(EmailFlag::Answered, None),
            Flag::Flagged => EnvelopeFlag::new(EmailFlag::Flagged, None),
            Flag::Deleted => EnvelopeFlag::new(EmailFlag::Deleted, None),
            Flag::Draft => EnvelopeFlag::new(EmailFlag::Draft, None),
            Flag::Recent => EnvelopeFlag::new(EmailFlag::Recent, None),
            Flag::MayCreate => EnvelopeFlag::new(EmailFlag::MayCreate, None),
            Flag::Custom(s) => EnvelopeFlag::new(EmailFlag::Custom, Some(s.to_string())),
        }
    }
}

impl std::fmt::Display for EnvelopeFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.custom {
            Some(custom) => write!(f, "{}", custom),
            None => write!(f, "{}", self.flag),
        }
    }
}

impl std::fmt::Display for EmailFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let flag_str = match self {
            EmailFlag::Seen => "Seen",
            EmailFlag::Answered => "Answered",
            EmailFlag::Flagged => "Flagged",
            EmailFlag::Deleted => "Deleted",
            EmailFlag::Draft => "Draft",
            EmailFlag::Recent => "Recent",
            EmailFlag::MayCreate => "MayCreate",
            EmailFlag::Custom => "Custom",
        };
        write!(f, "{}", flag_str)
    }
}
