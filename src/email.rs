use std::fs;
use std::path::Path;

use anyhow::Result;
use mailparse::parse_mail;

use crate::error::EmailError;

pub fn extract_email(path: &Path) -> Result<()> {
    let path_str = path.to_str().unwrap_or("<non-utf8 path>").to_string();

    let email_data = fs::read(path).map_err(|e| EmailError::ReadEmail {
        path: path_str.clone(),
        source: e,
    })?;
    let parsed = parse_mail(&email_data).map_err(|e| EmailError::ParseEmail {
        path: path_str.clone(),
        source: e,
    })?;

    let email_dir = path
        .parent()
        .ok_or_else(|| EmailError::NoParentDir {
            path: path_str.clone(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "No parent directory found"),
        })?
        .join("extracted");
    fs::create_dir_all(&email_dir)?;

    let body = extract_body(&parsed);
    let body_path = email_dir.join(format!("body.{}", body.0));
    fs::write(body_path, body.1)?;

    extract_attachments(&parsed, &email_dir)?;

    Ok(())
}

fn extract_body(parsed: &mailparse::ParsedMail) -> (&'static str, String) {
    // Parse subparts once and cache results
    let mut text_plain_body: Option<String> = None;
    let mut text_html_body: Option<String> = None;

    for part in std::iter::once(parsed).chain(parsed.subparts.iter()) {
        match part.ctype.mimetype.as_str() {
            "text/plain" => {
                if text_plain_body.is_none() {
                    text_plain_body = Some(part.get_body().unwrap_or_default())
                }
            }
            "text/html" => {
                if text_html_body.is_none() {
                    text_html_body = Some(part.get_body().unwrap_or_default())
                }
            }
            _ => {}
        }
    }

    if let Some(body) = text_plain_body {
        return ("txt", body);
    }

    if let Some(html) = text_html_body {
        match html2text::from_read(html.as_bytes(), 80) {
            Ok(x) => return ("md", x),
            Err(e) => {
                eprintln!("unable to parse HTML to markdown: {e}");
                return ("html", html);
            }
        }
    }

    ("txt", parsed.get_body().unwrap_or_default())
}

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
                println!("Extracted attachment: {}", attachment_path.display());
            }
        }
    }

    Ok(())
}

fn get_filename_from_headers(headers: &[mailparse::MailHeader]) -> Option<String> {
    for header in headers {
        if header.get_key().to_uppercase() == "CONTENT-DISPOSITION" {
            let value = header.get_value();
            if let Some(pos) = value.find("filename=") {
                let fname = &value[pos + 9..];
                let fname = fname.trim_matches('"');
                return Some(fname.to_string());
            }
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
