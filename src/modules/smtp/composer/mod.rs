// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::modules::cache::model::Envelope;
use scraper::{Html, Selector};
use time::{macros::format_description, OffsetDateTime};
use time_tz::timezones;
use time_tz::OffsetDateTimeExt;

pub struct BodyComposer;

impl BodyComposer {
    fn format_timestamp_with_timezone(timestamp_ms: i64, timezone_name: &str) -> Option<String> {
        let timestamp_sec = timestamp_ms.checked_div(1000)?;
        let utc_datetime = OffsetDateTime::from_unix_timestamp(timestamp_sec).ok()?;
        let timezone = timezones::get_by_name(timezone_name)?;
        let local_datetime = utc_datetime.to_timezone(timezone);
        let format = format_description!(
            "[year]-[month]-[day]T[hour repr:24]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"
        );
        local_datetime.format(&format).ok()
    }

    //
    fn get_html_body(html: &str) -> String {
        let html = html.trim();
        if html.is_empty() {
            return String::new();
        }

        let document = Html::parse_document(html);
        let body_selector =
            Selector::parse("body").unwrap_or_else(|_| Selector::parse("*").unwrap());

        let body = document.select(&body_selector).next();
        let body_or_html = if body.is_some() {
            body
        } else {
            let html_selector =
                Selector::parse("html").unwrap_or_else(|_| Selector::parse("*").unwrap());
            document.select(&html_selector).next()
        };

        body_or_html
            .map(|element| element.inner_html().trim().to_string())
            .unwrap_or_else(|| document.root_element().inner_html().trim().to_string())
    }

    pub fn generate_html(
        original_html: &str,
        reply_content: &str,
        envelope: &Envelope,
        timezone_name: &str,
        reply: bool,
    ) -> String {
        // Get original message body
        let original_body = Self::get_html_body(original_html);

        let reply_content = Self::get_html_body(reply_content);

        // Format metadata headers
        let mut headers = Vec::new();

        if let Some(from) = &envelope.from {
            headers.push(format!(
                "From: <span style=\"color: rgb(157, 41, 252);\">{}</span>",
                html_escape::encode_text(&from.to_string())
            ));
        }

        if let Some(timestamp) = &envelope.date {
            let date_str = Self::format_timestamp_with_timezone(*timestamp, timezone_name);
            if let Some(date_str) = date_str {
                headers.push(format!("Date: {}", html_escape::encode_text(&date_str)));
            }
        }

        if let Some(subject) = &envelope.subject {
            headers.push(format!("Subject: {}", html_escape::encode_text(subject)));
        }

        if let Some(to) = &envelope.to {
            headers.push(format!(
                "To: <span style=\"color: rgb(157, 41, 252);\">{}</span>",
                html_escape::encode_text(
                    &to.iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            ));
        }

        if let Some(cc) = &envelope.cc {
            headers.push(format!(
                "CC: <span style=\"color: rgb(157, 41, 252);\">{}</span>",
                html_escape::encode_text(
                    &cc.iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            ));
        }

        if let Some(bcc) = &envelope.bcc {
            headers.push(format!(
                "BCC: <span style=\"color: rgb(157, 41, 252);\">{}</span>",
                html_escape::encode_text(
                    &bcc.iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            ));
        }

        let message_type = if reply {
            "Replied message"
        } else {
            "Forwarded message"
        };

        // Add other metadata if present

        // Construct the full HTML
        format!(
            r#"<!DOCTYPE html>
            <html>
            <head>
                <meta http-equiv="Content-Type" content="text/html; charset=utf-8">
            </head>
            <body style="word-wrap: break-word;">
                <div>{}</div>
                <div><br></div>
                <blockquote style="margin: 0 0 0 40px; border-left: 2px solid #777; padding-left: 10px;">
                    <div>
                    ---------- {} ---------
                    <br>
                    {}
                    </div>
                    <div>{}</div>
                </blockquote>
            </body>
            </html>"#,
            reply_content,
            message_type,
            headers.join("<br>"),
            original_body
        )
    }

    fn format_text_body(text: &str) -> String {
        text.lines()
            .map(|line| format!("> {}", line))
            .collect::<Vec<String>>()
            .join("\n")
    }

    pub fn generate_text(
        original_text: &str,
        reply_content: &str,
        envelope: &Envelope,
        timezone_name: &str,
        reply: bool,
    ) -> String {
        let formatted_original = Self::format_text_body(original_text);

        let mut headers = Vec::new();
        if let Some(from) = &envelope.from {
            headers.push(format!("From: {}", from.to_string()));
        }
        if let Some(timestamp) = &envelope.date {
            let date_str = Self::format_timestamp_with_timezone(*timestamp, timezone_name);
            if let Some(date_str) = date_str {
                headers.push(format!("Date: {}", html_escape::encode_text(&date_str)));
            }
        }
        if let Some(subject) = &envelope.subject {
            headers.push(format!("Subject: {}", subject));
        }
        if let Some(to) = &envelope.to {
            headers.push(format!(
                "To: {}",
                to.iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ));
        }
        if let Some(cc) = &envelope.cc {
            headers.push(format!(
                "CC: {}",
                cc.iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ));
        }
        if let Some(bcc) = &envelope.bcc {
            headers.push(format!(
                "BCC: {}",
                bcc.iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ));
        }
        let message_type = if reply {
            "Replied message"
        } else {
            "Forwarded message"
        };

        format!(
            "{}\n\n---------- {} ---------\n{}\n\n{}",
            reply_content,
            message_type,
            headers.join("\n"),
            formatted_original
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        id,
        modules::{
            cache::imap::{
                mailbox::{EmailFlag, EnvelopeFlag},
                migration::EmailEnvelopeV3,
            },
            common::Addr,
        },
    };

    use super::*; // Import the Reply struct and its methods

    // Test 1: Basic HTML with body content
    #[test]
    fn test_basic_html_body() {
        let html = r#"
            <html>
                <head><title>Test</title></head>
                <body>
                    <p>Hello World</p>
                </body>
            </html>
        "#;

        let result = BodyComposer::get_html_body(html);
        assert_eq!(result.trim(), "<p>Hello World</p>");
    }

    #[test]
    fn test_complex_body_content() {
        let html = r#"
            <html>
                <body>
                    <div>
                        <h1>Title</h1>
                        <p>Paragraph with <strong>bold</strong> text</p>
                    </div>
                </body>
            </html>
        "#;

        let result = BodyComposer::get_html_body(html);
        let expected = r#"
                    <div>
                        <h1>Title</h1>
                        <p>Paragraph with <strong>bold</strong> text</p>
                    </div>
        "#;
        assert_eq!(result.trim(), expected.trim());
    }

    #[test]
    fn test_generate_reply() {
        let original_html = r#"
            <html>
                <body>
                    <p>Original message here</p>
                </body>
            </html>
        "#;

        let reply_content = "Thanks for your message!";

        let envelope = EmailEnvelopeV3 {
            account_id: 0,
            mailbox_id: 0,
            mailbox_name: "inbox_001".to_string(),
            uid: 42,
            internal_date: Some(1709510400000), // 2024-03-04 00:00:00 UTC
            size: 1024,
            flags: vec![EnvelopeFlag::new(EmailFlag::Seen, None)],
            flags_hash: 0x123456789abcdef,

            date: Some(1709424000000), // 2024-03-03 00:00:00 UTC
            from: Some(Addr {
                name: Some("John Doe".to_string()),
                address: Some("john.doe@example.com".to_string()),
            }),
            subject: Some("Meeting Agenda for Monday".to_string()),
            to: Some(vec![Addr {
                name: Some("Jane Smith".to_string()),
                address: Some("jane.smith@example.com".to_string()),
            }]),
            cc: Some(vec![Addr {
                name: Some("Alice Johnson".to_string()),
                address: Some("alice.j@example.com".to_string()),
            }]),
            bcc: Some(vec![Addr {
                name: Some("Bob Wilson".to_string()),
                address: Some("bob.wilson@example.com".to_string()),
            }]),

            in_reply_to: None,
            sender: None,
            return_address: None,
            message_id: Some("msg123@server.example.com".to_string()),
            thread_name: None,
            thread_id: id!(64),
            mime_version: Some("1.0".to_string()),
            references: None,
            reply_to: None,
            attachments: None,
            body_meta: None,
            received: None,
            mid: None,
            labels: vec![],
        };

        let result = BodyComposer::generate_html(
            original_html,
            reply_content,
            &envelope.into(),
            "Asia/Shanghai",
            true,
        );
        println!("{}", &result);
    }

    #[test]
    fn test_generate_text_reply() {
        let original_text = "Hello,\nThis is a test email.\nRegards,\nJohn";
        let reply_content = "Hi John,\nThanks for your email!";

        let envelope = EmailEnvelopeV3 {
            from: Some(Addr {
                name: Some("John Doe".to_string()),
                address: Some("john@example.com".to_string()),
            }),
            date: Some(1709424000000), // 2024-03-03 00:00:00 UTC
            subject: Some("Test Email".to_string()),
            to: None,
            cc: None,
            bcc: None,
            account_id: 0,
            mailbox_id: 0,
            mailbox_name: "inbox".to_string(),
            uid: 1,
            internal_date: Some(0),
            size: 0,
            flags: vec![],
            flags_hash: 0,
            mime_version: None,
            message_id: None,
            in_reply_to: None,
            sender: None,
            return_address: None,
            thread_name: None,
            thread_id: id!(64),
            references: None,
            reply_to: None,
            attachments: None,
            body_meta: None,
            received: None,
            mid: None,
            labels: vec![],
        };

        let result = BodyComposer::generate_text(
            original_text,
            reply_content,
            &envelope.into(),
            "Asia/Shanghai",
            true,
        );
        println!("{}", result);
        // let expected = "Hi John,\nThanks for your email!\n\n---------- Replied message ---------\nFrom: John Doe <john@example.com>\nDate: March 03, 2024 at 12:00 AM UTC\nSubject: Test Email\n\n> Hello,\n> This is a test email.\n> Regards,\n> John";

        // assert_eq!(result.trim(), expected);
    }
}
