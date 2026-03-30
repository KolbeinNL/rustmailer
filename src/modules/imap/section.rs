// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use crate::{generate_token, modules::imap::decoder::try_decode_string};
use async_imap::{
    imap_proto::{
        BodyContentCommon, BodyContentSinglePart, BodyStructure, ContentEncoding, SectionPath,
    },
    types::Fetch,
};
use imap_proto::{ContentDisposition, ContentType};
use mail_parser::decoders::{
    base64::base64_decode_stream, quoted_printable::quoted_printable_decode,
};
use poem_openapi::{Enum, Object};
use serde::{Deserialize, Serialize};
use std::fmt::{self};

/// Represents the content transfer encoding used in email messages.
///
/// This enum defines common encoding types that may be used for encoding
/// email message bodies or headers, typically to support transmission of
/// non-ASCII data over protocols that only support ASCII.
///
/// ### Variants
///
/// - `None`: No encoding is applied. The content is assumed to be plain ASCII or UTF-8.
/// - `QuotedPrintable`: The content is encoded using the Quoted-Printable encoding,
///   which is suitable for data with mostly ASCII characters and a few non-ASCII bytes.
/// - `Base64`: The content is encoded using Base64, commonly used for binary data
///   or mostly non-ASCII text such as attachments.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Enum)]
pub enum Encoding {
    /// No encoding applied.
    #[default]
    None,
    /// Quoted-Printable encoding, suitable for mostly ASCII text with occasional non-ASCII characters.
    QuotedPrintable,
    /// Base64 encoding, typically used for binary data or non-ASCII content.
    Base64,
}

impl<'a> From<&ContentEncoding<'a>> for Encoding {
    fn from(content_encoding: &ContentEncoding<'a>) -> Self {
        match content_encoding {
            ContentEncoding::SevenBit | ContentEncoding::EightBit | ContentEncoding::Binary => {
                Encoding::None // These encodings do not require special handling, map to None
            }
            ContentEncoding::Base64 => Encoding::Base64,
            ContentEncoding::QuotedPrintable => Encoding::QuotedPrintable,
            ContentEncoding::Other(_) => Encoding::None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct EmailBodyPart {
    /// A unique identifier for the MIME part within the email's body structure (e.g., "1.1").
    pub id: String,
    /// The type of the MIME part (e.g., `Plain` for `text/plain`, `Html` for `text/html`).
    pub part_type: PartType,
    /// The path to the MIME part in the email's hierarchy (e.g., [1, 1] for a nested part).
    pub path: SegmentPath,
    /// Optional parameters for the MIME part (e.g., `charset`, `name`).
    pub params: Option<Vec<Param>>,
    /// The size of the MIME part in bytes.
    pub size: usize,
    /// The transfer encoding used for the MIME part (e.g., `base64`, `quoted-printable`).
    pub transfer_encoding: Encoding,
}

impl EmailBodyPart {
    pub fn new(
        part_type: PartType,
        path: SegmentPath,
        params: Option<Vec<Param>>,
        size: usize,
        transfer_encoding: Encoding,
    ) -> Self {
        Self {
            id: generate_token!(64),
            part_type,
            path,
            params,
            size,
            transfer_encoding,
        }
    }

    pub fn decode(&self, fetch: &Fetch) -> Option<Vec<u8>> {
        decode_impl(fetch, &self.transfer_encoding, &self.path)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct ImapAttachment {
    /// The unique identifier of the attachment.
    ///
    /// This field specifies a unique ID for the attachment within the IMAP mailbox, used for
    /// referencing or retrieving the attachment.
    pub id: String,

    /// The path to the attachment within the email message.
    ///
    /// This field specifies the segment path (e.g., MIME part path) within the email message
    /// where the attachment is located, encapsulated in a `SegmentPath` type.
    pub path: SegmentPath,

    /// The name of the attached file.
    ///
    /// This optional field specifies the file name (e.g., "document.pdf") as it appears in the
    /// email. If not provided, a default or generated name may be used.
    pub filename: Option<String>,

    /// Indicates whether the attachment is inline.
    ///
    /// If `true`, the attachment is intended to be displayed within the email body (e.g., an
    /// embedded image). If `false`, it is treated as a regular file attachment.
    pub inline: bool,

    /// The content ID for inline attachments.
    ///
    /// This optional field specifies a unique identifier for inline attachments (e.g., for
    /// referencing in HTML email content using `cid:<content_id>`). It is typically used when
    /// `inline` is `true`.
    pub content_id: Option<String>,

    /// The size of the attachment in bytes.
    ///
    /// This field specifies the size of the attachment data, useful for validation or display
    /// purposes.
    pub size: usize,

    /// The MIME type of the attachment.
    ///
    /// This field specifies the content type of the attachment (e.g., "application/pdf" or
    /// "image/png") to inform email clients how to handle the file.
    pub file_type: String,

    /// The transfer encoding used for the attachment.
    ///
    /// This field specifies the encoding (e.g., Base64, Quoted-Printable) applied to the
    /// attachment data, encapsulated in an `Encoding` type.
    pub transfer_encoding: Encoding,
}

impl ImapAttachment {
    pub fn new(
        path: SegmentPath,
        filename: Option<String>,
        size: usize,
        file_type: String,
        transfer_encoding: Encoding,
        inline: bool,
        content_id: Option<String>,
    ) -> Self {
        Self {
            id: generate_token!(72),
            path,
            filename,
            size,
            file_type,
            transfer_encoding,
            inline,
            content_id,
        }
    }

    pub fn decode(&self, fetch: &Fetch) -> Option<Vec<u8>> {
        decode_impl(fetch, &self.transfer_encoding, &self.path)
    }

    pub fn encoded(&self, fetch: &Fetch) -> Option<Vec<u8>> {
        let encoded_data = fetch.section(&self.path.clone().section_path())?;
        Some(encoded_data.to_vec())
    }
}

fn decode_impl(fetch: &Fetch, transfer_encoding: &Encoding, path: &SegmentPath) -> Option<Vec<u8>> {
    // Attempt to fetch the data section, return None if it doesn't exist
    let encoded_data = fetch.section(&path.clone().section_path())?;

    match transfer_encoding {
        Encoding::Base64 => {
            // Decode Base64
            base64_decode_stream(encoded_data.iter(), encoded_data.len(), u8::MAX)
        }
        Encoding::QuotedPrintable => {
            // Decode Quoted-Printable
            quoted_printable_decode(encoded_data)
        }
        Encoding::None => {
            // If encoding is None, return the original data
            Some(encoded_data.to_vec())
        }
    }
}

/// A structure representing a key-value pair for MIME part parameters.
///
/// The `Param` struct encapsulates a single parameter associated with a MIME part, such as
/// `charset` or `name`, as defined in RFC 2045. It is used in `EmailBodyPart` to specify additional
/// metadata for email body parts, such as character encoding or filename for attachments.
///
/// # Purpose
/// - **MIME Metadata**: Stores parameters like `charset` or `name` for MIME parts in an email.
/// - **IMAP Integration**: Supports IMAP's ability to parse MIME part attributes (RFC 3501).
/// - **Flexibility**: Allows for arbitrary key-value pairs to accommodate various MIME headers.
///
/// # Usage
/// Used within `EmailBodyPart` to define parameters for a MIME part, such as `charset="UTF-8"` for
/// a text part or `name="file.pdf"` for an attachment. Typically populated when fetching email
/// content via `MessageApi::fetch_message_content` or `MessageApi::fetch_message_attachment`.
///
/// # Example
/// ```
/// let param = Param {
///     key: "charset".to_string(),
///     value: "UTF-8".to_string(),
/// };
/// // Use in EmailBodyPart to specify MIME part parameters.
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct Param {
    /// The key of the MIME parameter (e.g., "charset", "name").
    pub key: String,
    /// The value of the MIME parameter (e.g., "UTF-8", "file.pdf").
    pub value: String,
}

/// An enumeration representing the type of an email body part.
///
/// The `PartType` enum specifies whether an email body part is plain text or HTML, as defined in
/// MIME content types (e.g., `text/plain`, `text/html` per RFC 2045). It is used in `EmailBodyPart`
/// to indicate the format of a specific MIME part in an email's body structure.
///
/// # Purpose
/// - **Content Identification**: Distinguishes between plain text and HTML content in email parts.
/// - **IMAP Compliance**: Aligns with IMAP's BODYSTRUCTURE response for parsing MIME parts (RFC 3501).
/// - **Client Rendering**: Helps clients determine how to render or process the body part.
///
/// # Usage
/// Used in `EmailBodyPart` to specify the type of a MIME part, typically set to `Plain` (default)
/// for `text/plain` or `Html` for `text/html`. Populated when fetching email content via
/// `MessageApi::fetch_message_content`.
///
/// # Default
/// The default value is `Plain`, representing a `text/plain` MIME part.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Enum)]
pub enum PartType {
    /// Represents a plain text body part (`text/plain`).
    #[default]
    Plain,
    /// Represents an HTML body part (`text/html`).
    Html,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize, Object)]
pub struct SegmentPath {
    /// Represents a path of segments indicating the section index of an attachment
    /// within a MIME email structure.
    ///
    /// Each value in `segments` corresponds to a level in the MIME hierarchy,
    /// allowing precise identification of a specific part of the email (e.g., an attachment).
    ///
    /// For example:
    /// - `segments = [1]` refers to the first top-level MIME part.
    /// - `segments = [1, 2]` refers to the second subpart of the first part.
    ///
    /// This is commonly used to locate attachments or specific parts within multipart messages.
    pub segments: Vec<u64>,
}

impl SegmentPath {
    pub fn new(segments: Vec<u64>) -> Self {
        Self { segments }
    }

    pub fn add(&mut self, segment: u64) {
        self.segments.push(segment);
    }

    pub fn with_added_segment(&self, segment: u64) -> Self {
        let mut cloned = self.clone();
        cloned.add(segment);
        cloned
    }

    pub fn section_path(self) -> SectionPath {
        let segments = if self.segments.is_empty() {
            vec![1]
        } else {
            self.segments.into_iter().map(|u| u as u32).collect()
        };

        SectionPath::Part(segments, None)
    }
}

impl std::fmt::Display for SegmentPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.segments.is_empty() {
            write!(f, "1")
        } else {
            write!(
                f,
                "{}",
                self.segments
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            )
        }
    }
}

#[derive(Clone, Debug)]
/// A struct to extract file attachments from a given body structure.
pub struct SectionExtractor<'a> {
    structure: &'a BodyStructure<'a>,
}

impl<'a> SectionExtractor<'a> {
    /// Creates a new `AttachmentExtractor` with the specified body structure.
    pub fn new(structure: &'a BodyStructure<'a>) -> Self {
        Self { structure }
    }

    /// Extracts a vector of attachments from the body structure.
    pub fn get_attachments(&self) -> Option<Vec<ImapAttachment>> {
        Self::recursive_parse_attachments(self.structure, SegmentPath::new(Vec::new()))
    }

    pub fn get_body_parts(&self) -> Option<Vec<EmailBodyPart>> {
        Self::recursive_parse_body(self.structure, SegmentPath::new(Vec::new()))
    }

    /// Retrieves the file name from the content type if it exists.
    fn get_file_name(ty: &ContentType<'a>) -> Option<String> {
        ty
            .params
            .as_ref()?
            .iter()
            .find_map(|(key, value)| {
                if key.eq_ignore_ascii_case("name") {
                    Some(try_decode_string(value.trim()))
                } else {
                    None
                }
            })
    }

    /// Retrieves the file name from the content disposition if it exists.
    fn get_file_name_disposition(disposition_option: &Option<ContentDisposition<'a>>) -> Option<String> {
        if let Some(disposition) = &disposition_option {
            disposition
                .params
                .as_ref()?
                .iter()
                .find_map(|(key, value)| {
                    if key.eq_ignore_ascii_case("filename") {
                        Some(try_decode_string(value.trim()))
                    } else {
                        None
                    }
                })
        } else {
            return None;
        }
    }

    /// Parses a single attachment from the body content.
    #[inline]
    fn parse_attachment(
        segment: SegmentPath,
        common: &BodyContentCommon<'a>,
        other: &BodyContentSinglePart,
    ) -> Option<ImapAttachment> {
        common.disposition.as_ref().and_then(|disposition| {
            if disposition.ty.eq_ignore_ascii_case("attachment")
                || disposition.ty.eq_ignore_ascii_case("inline")
            {
                let attachment_encoding: Encoding = (&other.transfer_encoding).into();
                let content_id = other.id.clone().map(|cow| cow.into_owned());
                let inline = disposition.ty.eq_ignore_ascii_case("inline");
                let mut filename = Self::get_file_name_disposition(&common.disposition);
                if filename.is_none() {
                    filename = Self::get_file_name(&common.ty)
                }

                Some(ImapAttachment::new(
                    segment,
                    filename,
                    other.octets as usize,
                    common.ty.subtype.to_string(),
                    attachment_encoding,
                    inline,
                    content_id,
                ))
            } else {
                None
            }
        })
    }

    #[inline]
    fn parse_body(
        segment: SegmentPath,
        common: &BodyContentCommon<'a>,
        other: &BodyContentSinglePart,
    ) -> Option<EmailBodyPart> {
        // Check if type is "TEXT" and subtype is either "PLAIN" or "HTML"
        if common.ty.ty.eq_ignore_ascii_case("TEXT") {
            // Match subtype to determine PartType
            let part_type = match common.ty.subtype.to_ascii_uppercase().as_str() {
                "PLAIN" => PartType::Plain,
                "HTML" => PartType::Html,
                _ => return None, // Exit if subtype is unsupported
            };

            // Perform encoding conversion and size calculation
            let txt_encoding: Encoding = (&other.transfer_encoding).into();
            let size = other.octets as usize;

            // Map params from Cow<'a, str> to owned Strings
            let params = Self::convert_params(common);

            // Return the parsed EmailBodyPart with the appropriate PartType
            Some(EmailBodyPart::new(
                part_type,
                segment,
                params,
                size,
                txt_encoding,
            ))
        } else {
            None
        }
    }

    // Helper function to convert params from Cow<'a, str> to owned strings
    #[inline]
    fn convert_params(common: &BodyContentCommon<'a>) -> Option<Vec<Param>> {
        common.ty.params.clone().map(|vec| {
            vec.into_iter()
                .map(|(k, v)| Param {
                    key: k.into_owned(),
                    value: v.into_owned(),
                })
                .collect()
        })
    }

    /// Recursively parses attachments from the body structure.
    fn recursive_parse_attachments(
        body_structure: &'a BodyStructure<'a>,
        segment: SegmentPath,
    ) -> Option<Vec<ImapAttachment>> {
        match body_structure {
            BodyStructure::Multipart { bodies, .. } => {
                // Recursively parse each part and collect attachments, if any part returns None, return None
                let mut attachments = Vec::new();
                for (i, body) in bodies.iter().enumerate() {
                    if let Some(mut part_attachments) = Self::recursive_parse_attachments(
                        body,
                        segment.with_added_segment(i as u64 + 1),
                    ) {
                        attachments.append(&mut part_attachments);
                    }
                }
                if attachments.is_empty() {
                    None
                } else {
                    Some(attachments)
                }
            }
            BodyStructure::Basic { common, other, .. }
            | BodyStructure::Message { common, other, .. }
            | BodyStructure::Text { common, other, .. } => {
                // Parse the attachment, and return as a single-element vector if Some
                Self::parse_attachment(segment, common, other).map(|a| vec![a])
            }
        }
    }

    fn recursive_parse_body(
        body_structure: &'a BodyStructure<'a>,
        segment: SegmentPath,
    ) -> Option<Vec<EmailBodyPart>> {
        match body_structure {
            BodyStructure::Multipart { bodies, .. } => {
                // Recursively parse each part and collect attachments, if any part returns None, return None
                let mut parts = Vec::new();
                for (i, body) in bodies.iter().enumerate() {
                    if let Some(mut p) =
                        Self::recursive_parse_body(body, segment.with_added_segment(i as u64 + 1))
                    {
                        parts.append(&mut p);
                    }
                }
                if parts.is_empty() {
                    None
                } else {
                    Some(parts)
                }
            }
            BodyStructure::Basic { common, other, .. }
            | BodyStructure::Message { common, other, .. }
            | BodyStructure::Text { common, other, .. } => {
                // Parse the attachment, and return as a single-element vector if Some
                Self::parse_body(segment, common, other).map(|p| vec![p])
            }
        }
    }
}
