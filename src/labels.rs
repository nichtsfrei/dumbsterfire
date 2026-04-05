use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::email::extract_email;
use crate::error::LabelError;
use crate::filter::{self, CompareResult};
use crate::models::{Email, EmailHeader};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Translation {
    pub title: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Labels {
    #[serde(flatten)]
    pub definitions: HashMap<String, HashMap<String, Translation>>,
}

fn create_filter(
    output_base: &Path,
    filter_base: &Path,
    labels: &Labels,
) -> Vec<(PathBuf, String)> {
    labels
        .definitions
        .keys()
        .filter_map(|label| {
            let rule_path = filter_base.join(label).join("rule.filter");
            if !rule_path.exists() {
                eprintln!(
                    "Warning: Filter file not found for label '{}', skipping",
                    label
                );
                return None;
            }

            let rule_content = fs::read_to_string(&rule_path).ok()?;

            Some((
                output_base.join(format!("label_{label}.files")),
                rule_content,
            ))
        })
        .collect()
}

struct PathContainer<'a>(&'a str);

impl filter::FieldComparer for PathContainer<'_> {
    fn compare_field<'a>(
        &'a self,
        op: &filter::Operator,
        field: &'a str,
        value: &'a str,
    ) -> filter::CompareResult {
        if field != "path" || op != &filter::Operator::Contains {
            return CompareResult::NotApplicable;
        }
        if self.0.contains(value) {
            CompareResult::Match
        } else {
            CompareResult::NoMatch
        }
    }
}

/// Process emails and apply labels. Optionally trigger extraction for matched emails.
///
/// # Arguments
/// * `base_dir` - Base directory containing downloaded emails
/// * `rule_path` - Path to labels directory containing rule.filter files
/// * `extract_on_match` - If true, extract emails when they match a filter
pub fn process_emails(base_dir: &Path, rule_path: &Path, extract_on_match: bool) -> Result<()> {
    println!("Using {}", rule_path.display());
    let labels = serde_json::from_reader(
        File::open(rule_path.join("labels.json")).map_err(LabelError::ReadLabels)?,
    )
    .map_err(LabelError::ParseLabels)?;
    let content: Vec<_> = create_filter(base_dir, rule_path, &labels);
    if content.is_empty() {
        eprintln!("Warning: No filter rules found in {rule_path:?}");
    }
    println!("aha");
    let filter: Vec<_> = content
        .iter()
        .map(|(p, f)| {
            let out = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(p)
                .map_err(LabelError::WriteLabel)?;
            let f = filter::parse(f).map_err(|e| LabelError::ParseFilter {
                path: p.display().to_string(),
                source: e,
            })?;
            Ok((out, f))
        })
        .collect::<Result<_>>()?;

    let shasums = base_dir.join("sha256sums");
    let file = File::open(&shasums)?;
    let reader = BufReader::new(file);

    let filter_count = filter.len();
    let mut matched_count = 0;

    for email_file in reader.lines() {
        let ef = email_file?;
        let (hash, path) = ef.split_once("  ").ok_or_else(|| {
            LabelError::ReadLabels(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid sha256sums line format: {ef}"),
            ))
        })?;

        let mut matched_any_label = false;
        for (o, filter) in &filter {
            let mut lookup_file = o;
            let mut found = filter.eval(&PathContainer(path));
            let email_path = base_dir.join(path);
            if !found {
                let email_data = fs::read(&email_path)?;
                let parsed = mailparse::parse_mail(&email_data)?;
                let email = Email::from(parsed);
                let header = EmailHeader::from(&email);
                found = filter.eval(&header);
            }

            if found {
                matched_any_label = true;
                writeln!(lookup_file, "{}  {}", hash, path)?;
            }
        }

        // Extract email if it matched any label and extraction is enabled
        if matched_any_label && extract_on_match {
            let email_path = base_dir.join(path);
            if let Err(e) = extract_email(&email_path) {
                eprintln!("Warning: Failed to extract email {path}: {e}");
            } else {
                matched_count += 1;
            }
        }
    }

    println!(
        "Processed {} emails, matched {} with filters",
        filter_count, matched_count
    );

    Ok(())
}
