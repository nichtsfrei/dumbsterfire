use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset};
use mailparse::ParsedMail;

use crate::filter;

pub struct Sanitized(String);

impl Sanitized {
    pub fn to_str(&self) -> &str {
        &self.0
    }
}

impl From<Sanitized> for String {
    fn from(value: Sanitized) -> Self {
        value.0
    }
}

impl<T: AsRef<str>> From<T> for Sanitized {
    fn from(value: T) -> Self {
        let mut out = String::with_capacity(value.as_ref().len());
        let mut prev_dash = false;

        for (i, mut c) in value.as_ref().to_lowercase().chars().enumerate() {
            if c.is_ascii_punctuation() || c.is_whitespace() {
                c = '-';
            }
            if c == '-' && (prev_dash || out.is_empty() || i == value.as_ref().len() - 1) {
                prev_dash = true;
            } else {
                out.push(c);
                prev_dash = c == '-';
            }
        }

        Self(out)
    }
}

#[derive(Debug, Default)]
pub struct EmailHeader {
    subject: String,
    from: String,
    to: String,
    date: DateTime<FixedOffset>,
    body: String,
}

impl EmailHeader {
    pub fn to_path(&self, root: &Path) -> PathBuf {
        root.join(Self::normalize(&self.to))
            .join(Self::normalize(&self.from))
            .join(self.date.to_rfc3339())
            .join(Self::normalize(&self.subject))
    }

    fn normalize(to: &str) -> String {
        Sanitized::from(to).into()
    }
}

impl From<&Email<'_>> for EmailHeader {
    fn from(value: &Email<'_>) -> Self {
        let mut header = EmailHeader::default();
        value.0.headers.iter().fold(&mut header, |a, b| {
            match &b.get_key().to_lowercase() as &str {
                "subject" => a.subject = b.get_value(),
                "date" => match chrono::DateTime::parse_from_rfc2822(&b.get_value()) {
                    Ok(x) => a.date = x,
                    Err(error) => {
                        dbg!(error);
                    }
                },
                "from" => a.from = b.get_value(),
                "to" => a.to = b.get_value(),
                _ => {}
            }
            a
        });
        header.body = value.0.get_body().unwrap_or_default();
        header
    }
}

impl filter::FieldComparer for EmailHeader {
    fn compare_field<'a>(
        &'a self,
        op: &filter::Operator,
        field: &'a str,
        value: &'a str,
    ) -> filter::CompareResult {
        let field_value = match field {
            "subject" => &self.subject,
            "from" => &self.from,
            "date" => &self.date.to_rfc3339(),
            "to" => &self.to,
            "path" => &self.to_path(&PathBuf::new()).to_string_lossy().into_owned(),
            "body" | "content" => &self.body,
            unknown => {
                eprintln!("Unknown field: {unknown}");
                return filter::CompareResult::NotApplicable;
            }
        };
        let result = match op {
            filter::Operator::Contains => {
                field_value.to_lowercase().contains(&value.to_lowercase())
            }
            filter::Operator::Is => field_value == value,
        };
        result.into()
    }
}

pub struct Email<'a>(ParsedMail<'a>);

impl<'a> From<ParsedMail<'a>> for Email<'a> {
    fn from(value: ParsedMail<'a>) -> Self {
        Self(value)
    }
}

impl<'a> AsRef<ParsedMail<'a>> for Email<'a> {
    fn as_ref(&self) -> &ParsedMail<'a> {
        &self.0
    }
}
