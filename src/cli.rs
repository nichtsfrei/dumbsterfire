use std::{io::stdin, path::PathBuf};

use crate::config::{Config, default_output_dir};
use anyhow::Result;
use clap::{Args, Parser};

#[derive(Debug, Parser)]
#[command(
    name = "dumbsterfire",
    about = "Download and label emails from IMAP",
    long_about = "A tool to download emails from IMAP folders and apply labels based on filter rules"
)]
pub enum Cli {
    #[command(name = "download", about = "Download emails from IMAP folders")]
    Download(DownloadArgs),
    #[command(
        name = "label",
        about = "Label downloaded emails based on filter rules"
    )]
    Label(LabelArgs),
    #[command(name = "email", about = "Extract email from .eml file")]
    Email(EmailArgs),
}

#[derive(Debug, Args)]
pub struct DownloadArgs {
    #[arg(long, env = "IMAP_HOST")]
    pub host: String,

    #[arg(long, default_value = "993", env = "IMAP_PORT")]
    pub port: u16,

    #[arg(long, env = "IMAP_USER")]
    pub username: Option<String>,

    #[arg(long, env = "IMAP_PASS")]
    pub password: Option<String>,

    #[arg(long, env = "OUTPUT_DIR")]
    pub output_dir: Option<PathBuf>,

    /// Read password from stdin instead of CLI flag or env var
    #[arg(long, conflicts_with = "password")]
    pub password_from_stdin: bool,
}

#[derive(Debug, Args)]
pub struct LabelArgs {
    #[arg(long, env = "LABEL_DIR")]
    pub label_dir: Option<PathBuf>,

    #[arg(long, env = "OUTPUT_DIR")]
    pub output_dir: Option<PathBuf>,

    /// Extract matched emails (attachments, body) when applying labels
    #[arg(long)]
    pub extract: bool,
}

impl TryFrom<DownloadArgs> for Config {
    type Error = anyhow::Error;

    fn try_from(value: DownloadArgs) -> Result<Self> {
        let username = value
            .username
            .ok_or_else(|| anyhow::anyhow!("Missing --username or IMAP_USERNAME"))?;
        let password = if value.password_from_stdin {
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            input.trim_end().to_string()
        } else {
            value.password.ok_or_else(|| {
                anyhow::anyhow!("Missing --password, IMAP_PASSWORD, or --password-from-stdin")
            })?
        };
        let output_dir = value.output_dir.unwrap_or_else(default_output_dir);
        Ok(Config {
            host: value.host,
            port: value.port,
            username,
            password,
            output_dir,
        })
    }
}

#[derive(Debug, Args)]
pub struct EmailArgs {
    pub path: PathBuf,
}
