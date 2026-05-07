use std::fs;
use std::path::Path;

use anyhow::Result;
use mailparse::parse_mail;
use tracing::{info, debug, warn, instrument};

use crate::error::EmailError;

#[instrument]
pub fn extract_email(path: &Path) -> Result<()> {
    info!("reading email file");
    let email_data = fs::read(path).map_err(|e| EmailError::ReadEmail {
        path: path.display().to_string(),
        source: e,
    })?;
    info!("parsing email");
    let parsed = parse_mail(&email_data).map_err(|e| EmailError::ParseEmail {
        path: path.display().to_string(),
        source: e,
    })?;

    let mut current = path.parent().ok_or_else(|| EmailError::NoParentDir {
        path: path.display().to_string(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "No parent directory found"),
    })?;

    // (subject -> date -> from -> to -> host)
    for _ in 0..5 {
        current = current.parent().ok_or_else(|| EmailError::NoParentDir {
            path: path.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "Unexpected path depth"),
        })?;
    }
    // current is now the host directory (e.g., "posteo.de")
    // Its parent is the output_dir which is our extracted_root
    let extracted_root = current.parent().ok_or_else(|| EmailError::NoParentDir {
        path: path.display().to_string(),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No parent directory found for host",
        ),
    })?;

    let rel_path = path
        .strip_prefix(extracted_root)
        .map_err(|e| EmailError::InvalidPath {
            path: path.display().to_string(),
            source: e,
        })?;

    // Remove the filename to get the directory structure
    let rel_dir = rel_path.parent().ok_or_else(|| EmailError::NoParentDir {
        path: path.display().to_string(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "No parent directory found"),
    })?;

    // Place extracted/ at the root level (same level as host folders)
    let email_dir = extracted_root.join("extracted").join(rel_dir);
    info!("creating output directory");
    fs::create_dir_all(&email_dir)?;

    info!("extracting body");
    let body = extract_body(&parsed);
    let body_path = email_dir.join(format!("body.{}", body.0));
    fs::write(body_path, body.1)?;

    info!("extracting attachments");
    extract_attachments(&parsed, &email_dir)?;

    Ok(())
}

#[instrument(skip(parsed))]
fn extract_body(parsed: &mailparse::ParsedMail) -> (&'static str, String) {
    fn handle_part(part: &mailparse::ParsedMail) -> Option<(&'static str, String)> {
        let content = part.get_body().unwrap_or_default();
        match part.ctype.mimetype.as_str() {
            "text/html" => Some(match html2text::from_read(content.as_bytes(), 80) {
                Ok(x) => ("md", x),
                Err(_e) => {
                    debug!(mimetype = ?part.ctype.mimetype, "unable to parse to markdown, returning html");
                    ("html", content)
                }
            }),
            "text/plain" => Some(("txt", content)),
            _ => None,
        }
    }
    fn find_bodies(parsed: &[mailparse::ParsedMail]) -> Vec<(&'static str, String)> {
        if parsed.is_empty() {
            return Vec::new();
        }
        let mut results = Vec::with_capacity(2);

        for part in parsed {
            if let Some(x) = handle_part(part)
                && !x.1.is_empty()
            {
                results.push((x.0, x.1.replace("\r\n", "\n").replace('\r', "\n")));
            }
            results.extend(find_bodies(&part.subparts));
        }
        results
    }

    let content = find_bodies(std::slice::from_ref(parsed));
    if content.len() > 2 {
        debug!(count = content.len(), "more than two content parts found");
    }
    if content.is_empty() {
        warn!("No content found in email");
    }

    content
        .iter()
        .filter_map(|(suffix, content)| {
            if suffix == &"md" {
                Some((*suffix, content.clone()))
            } else {
                None
            }
        })
        .next()
        .unwrap_or_else(|| content.into_iter().next().unwrap_or(("txt", String::new())))
}

#[instrument(skip(email, attachments_dir))]
fn extract_attachments(
    email: &mailparse::ParsedMail,
    attachments_dir: &std::path::Path,
) -> Result<()> {
    for part in &email.subparts {
        let headers = &part.headers;
        if let Some(fname) = get_filename_from_headers(headers) {
            let fname_str = fname.clone();
            let bytes = match part.get_body_encoded() {
                mailparse::body::Body::Base64(x) => x.get_decoded(),
                mailparse::body::Body::QuotedPrintable(x) => x.get_decoded(),
                mailparse::body::Body::SevenBit(x) => Ok(x.get_raw().to_vec()),
                mailparse::body::Body::EightBit(x) => Ok(x.get_raw().to_vec()),
                mailparse::body::Body::Binary(x) => Ok(x.get_raw().to_vec()),
            };

            let body = bytes.map_err(|e| EmailError::DecodeAttachment {
                path: fname_str.clone(),
                err_msg: e.to_string(),
            })?;

            if !body.is_empty() {
                let fname = sanitize_filename(&fname);
                let attachment_path = attachments_dir.join(fname);
                fs::write(&attachment_path, body).map_err(|e| EmailError::WriteAttachment {
                    path: attachment_path.display().to_string(),
                    source: e,
                })?;
                info!(path = ?attachment_path, "Extracted attachment");
            }
        }
    }

    Ok(())
}

#[instrument(skip(headers))]
fn get_filename_from_headers(headers: &[mailparse::MailHeader]) -> Option<String> {
    fn extract_filename(value: &str, prefix: &str) -> Option<String> {
        let pos = value.find(prefix)?;
        let fname = &value[pos + prefix.len()..];
        // Extract until first semicolon or end of string
        let fname = fname.split(';').next().unwrap_or(fname);
        // Trim whitespace and quotes
        let fname = fname.trim().trim_matches('"');
        Some(sanitize_filename(fname))
    }

    for header in headers {
        let key = header.get_key().to_uppercase();
        let value = header.get_value();

        // Check Content-Disposition for filename=, fall back to Content-Type for name=
        if key == "CONTENT-DISPOSITION"
            && let Some(fname) = extract_filename(&value, "filename=")
        {
            return Some(fname);
        } else if key == "CONTENT-TYPE"
            && let Some(fname) = extract_filename(&value, "name=")
        {
            return Some(fname);
        }
    }
    None
}

fn sanitize_filename(filename: &str) -> String {
    let mut out = String::with_capacity(filename.len());
    let mut prev_dash = false;

    for (i, mut c) in filename.to_lowercase().chars().enumerate() {
        if c.is_ascii_whitespace() {
            c = '-';
        }
        if c == '-' && (prev_dash || out.is_empty() || i == filename.len() - 1) {
            prev_dash = true;
        } else {
            out.push(c);
            prev_dash = c == '-';
        }
    }

    out
}

#[cfg(test)]
mod extracted_text {
    use super::*;

    #[test]
    fn unknown() {
        let raw_email = r#"Content-Transfer-Encoding: quoted-printable

hello"#;
        let parsed = mailparse::parse_mail(raw_email.as_bytes()).unwrap();
        let result = extract_body(&parsed);
        assert_eq!(result, ("txt", "hello".to_string()));
    }

    #[test]
    fn text_plain() {
        let raw_email = r#"Content-Type: text/plain; charset="iso-8859-1"
Content-Transfer-Encoding: quoted-printable

hello"#;
        let parsed = mailparse::parse_mail(raw_email.as_bytes()).unwrap();
        let result = extract_body(&parsed);
        assert_eq!(result, ("txt", "hello".to_string()));
    }

    #[test]
    fn markdown() {
        let raw_email = r#"Content-Type: text/html; charset="iso-8859-1"
Content-Transfer-Encoding: quoted-printable

<html><h1>Hello</h1></html>"#;
        let parsed = mailparse::parse_mail(raw_email.as_bytes()).unwrap();
        let result = extract_body(&parsed);
        assert_eq!(result, ("md", "# Hello\n".to_string()));
    }
}

#[cfg(test)]
mod attachment_header_parsing {
    use super::*;

    #[test]
    fn content_disposition() {
        let raw_email = r#"Content-Type: application/pdf; name="Name KT17_33.pdf"
Content-Description: Name KT17_33.pdf
Content-Disposition: attachment; filename="Name KT17_33.pdf"; size=342278
Content-Transfer-Encoding: base64

"#;
        let parsed = mailparse::parse_mail(raw_email.as_bytes()).unwrap();
        let result = super::get_filename_from_headers(&parsed.headers);
        assert_eq!(result, Some("name-kt17_33.pdf".to_string()));
    }

    #[test]
    fn no_filename() {
        let raw_email = r#"Content-Type: text/plain
Content-Disposition: inline

"#;
        let parsed = mailparse::parse_mail(raw_email.as_bytes()).unwrap();
        let result = get_filename_from_headers(&parsed.headers);
        assert_eq!(result, None);
    }

    #[test]
    fn use_from_content_type_when_no_disposition() {
        let raw_email = r#"Content-Type: application/pdf; name="file.pdf"
Content-Transfer-Encoding: base64

"#;
        let parsed = mailparse::parse_mail(raw_email.as_bytes()).unwrap();
        let result = get_filename_from_headers(&parsed.headers);
        assert_eq!(result, Some("file.pdf".to_string()));
    }
}
