// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use access_token::AccessTokenApi;
use account::AccountApi;
use auto_config::AutoConfigApi;
use event_hook::EventHookApi;
use license::LicenseApi;
use mailbox::MailBoxApi;
use message::MessageApi;
use mta::MTAApi;
use oauth2::OAuth2Api;
use poem_openapi::{OpenApiService, Tags};
use send::SendMailApi;
use system::SystemApi;
use templates::TempaltesApi;

use crate::rustmailer_version;

pub mod access_token;
pub mod account;
pub mod auto_config;
pub mod event_hook;
pub mod license;
pub mod mailbox;
pub mod message;
pub mod mta;
pub mod oauth2;
pub mod send;
pub mod system;
pub mod templates;

#[derive(Tags)]
pub enum ApiTags {
    AccessToken,
    License,
    AutoConfig,
    Account,
    Template,
    Mta,
    Mailbox,
    OAuth2,
    Hook,
    Message,
    SendMail,
    System,
}

type RustMailOpenApi = (
    AccessTokenApi,
    LicenseApi,
    AutoConfigApi,
    AccountApi,
    TempaltesApi,
    MTAApi,
    EventHookApi,
    SystemApi,
    MailBoxApi,
    OAuth2Api,
    MessageApi,
    SendMailApi,
);

pub fn create_openapi_service() -> OpenApiService<RustMailOpenApi, ()> {
    OpenApiService::new(
        (
            AccessTokenApi,
            LicenseApi,
            AutoConfigApi,
            AccountApi,
            TempaltesApi,
            MTAApi,
            EventHookApi,
            SystemApi,
            MailBoxApi,
            OAuth2Api,
            MessageApi,
            SendMailApi,
        ),
        "RustMailerApi",
        rustmailer_version!(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Duplicate `operation_id`s and malformed paths only surface when the
    /// OpenAPI document is generated (the duplicate case panics), so building
    /// the document is the cheapest way to guard the whole API surface.
    #[test]
    fn openapi_document_builds() {
        let spec = create_openapi_service().spec();
        assert!(spec.contains("/api/v1/send-email-task/{id}/retry"));
        assert!(spec.contains("/api/v1/send-email-tasks/retry"));
    }
}
