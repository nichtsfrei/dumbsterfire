mod checksum;
mod cli;
mod config;
mod email;
mod filter;
mod imap;
mod labels;
mod matcher;
mod models;

mod error;

use crate::cli::Cli;
use anyhow::Result;
use clap::Parser;
use tracing::{info, instrument};
use tracing_subscriber::{fmt, EnvFilter};

use crate::config::{Config, default_label_dir, default_output_dir};

fn main() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(env_filter).init();

    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        tracing::error!(error = ?e, "Error");
        for cause in e.chain() {
            tracing::debug!(cause = ?cause, "Caused by");
        }
        std::process::exit(1);
    }
}

#[instrument]
fn run(cli: Cli) -> Result<()> {
    match cli {
        Cli::Download(args) => {
            let config: Config = args.try_into()?;
            info!("Starting download");
            imap::download(&config)
        }
        Cli::Label(args) => {
            // Use provided paths or fall back to defaults
            let output_dir = args.output_dir.unwrap_or_else(default_output_dir);
            let label_dir = args.label_dir.unwrap_or_else(default_label_dir);
            info!("Processing labels");
            labels::process_emails(&output_dir, &label_dir, args.extract)
        }
        Cli::Email(args) => {
            info!("Extracting email");
            email::extract_email(&args.path)
        }
    }
}
