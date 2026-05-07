use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Result, bail};
use thiserror::Error;
use tracing::warn;

use crate::config::Config;

#[derive(Error, Debug)]
pub enum Sha256Error {
    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error("Invalid SHA256 line format: {line}")]
    InvalidLineFormat { line: String },

    #[error("Failed to rename '{from}' to '{to}': {source}")]
    RenameError {
        from: String,
        to: String,
        source: std::io::Error,
    },
}

pub fn merge_sha256_files(config: &Config, last_cheked: Option<String>) -> Result<()> {
    fn split_line(line: &str) -> Option<(&str, &str)> {
        let line = line.trim();
        line.split_once("  ")
    }

    let base_dir = PathBuf::from(&config.output_dir).join(&config.host);

    let sha_new_path = base_dir.join("sha256sums.new");
    let sha_old_path = base_dir.join("sha256sums");
    let sha_temp_path = base_dir.join("sha256sums.tmp");

    if !sha_new_path.exists() {
        return Ok(());
    }
    if !sha_old_path.exists() {
        fs::rename(&sha_new_path, &sha_old_path).map_err(|e| Sha256Error::RenameError {
            from: sha_new_path.display().to_string(),
            to: sha_old_path.display().to_string(),
            source: e,
        })?;
        return Ok(());
    }

    let new_content = fs::read_to_string(&sha_new_path)?;
    let old_content = fs::read_to_string(&sha_old_path)?;

    let mut entries: HashMap<&str, &str> = HashMap::new();

    for line in new_content.lines() {
        if let Some((hash_sum, path)) = split_line(line) {
            entries.insert(path, hash_sum);
        } else {
            bail!(Sha256Error::InvalidLineFormat {
                line: line.to_string(),
            });
        }
    }

    for line in old_content.lines() {
        if let Some((hash_sum, path)) = split_line(line) {
            if !entries.contains_key(path) {
                entries.insert(path, hash_sum);
            }
        } else {
            warn!("Skipping invalid line in old SHA256 file");
        }
    }

    let mut sha_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&sha_temp_path)?;

    let mut sorted_entries: Vec<_> = entries.into_iter().collect();
    sorted_entries.sort_by(|a, b| a.0.cmp(b.0));

    for (path, hash) in sorted_entries {
        writeln!(sha_file, "{}  {}", hash, path)?;
    }

    if let Some(ts) = last_cheked
        && let Err(error) = fs::rename(&sha_old_path, base_dir.join(format!("sha256sums.{ts}")))
    {
        warn!(%error, ts, "Unable to create backup shasum")
    }

    fs::rename(&sha_temp_path, &sha_old_path).map_err(|e| Sha256Error::RenameError {
        from: sha_temp_path.display().to_string(),
        to: sha_old_path.display().to_string(),
        source: e,
    })?;

    fs::remove_file(&sha_new_path)?;

    Ok(())
}
