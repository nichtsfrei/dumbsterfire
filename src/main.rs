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
use tracing_subscriber::{EnvFilter, fmt};

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

fn run(cli: Cli) -> Result<()> {
    match cli {
        Cli::Download(args) => {
            let config: Config = args.try_into()?;
            imap::download(&config)
        }
        Cli::Label(args) => {
            let output_dir = args.output_dir.unwrap_or_else(default_output_dir);
            let label_dir = args.label_dir.unwrap_or_else(default_label_dir);
            labels::process_emails(&output_dir, &label_dir)
        }
        Cli::Email(args) => email::process_from_path(&args.path),
    }
}
